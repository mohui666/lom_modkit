//! Streaming PCM16 interpolation/mixing. The Host still owns reads, looping,
//! EOF and disposal; this entry only consumes the currently readable buffer.

use std::{mem::size_of, slice};

pub const REJECTED: u32 = 0;
pub const DONE: u32 = 1;
pub const NEED_READ: u32 = 2;
pub const STOPPED: u32 = 3;

/// Exactly 32 bytes on both supported Windows ABIs. `advance` preserves the
/// remainder of the *current* output frame while the Host refills a source.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MixState {
    pub phase: f64,
    pub left: f32,
    pub right: f32,
    pub next_left: f32,
    pub next_right: f32,
    pub has_next: u32,
    pub advance: u32,
}

#[inline]
fn accumulate_mono(output: f32, sample: f64) -> f32 {
    // Original Mono's add preserves the left/output NaN payload, quieting
    // sNaN. LLVM can commute fadd; make this observed precedence explicit.
    if output.is_nan() {
        f32::from_bits(output.to_bits() | 0x0040_0000)
    } else {
        (output as f64 + sample) as f32
    }
}

/// Batch equivalent of VideoAudioMixer.Channel's original inner loop.
///
/// `input_frames` counts whole physical frames at the supplied pointer. The
/// Host deliberately includes the stale tail of a non-frame-aligned ReadPcm16
/// result, matching the original managed reader rather than changing decoding.
/// `NEED_READ` has already promoted next->current and subtracted one phase;
/// call the original managed Read, update next/has_next, then resume this state.
/// Even when all requested output was produced, finish pending advancement so
/// the final pre-read/EOF/dispose happens before the managed Mix call returns.
///
/// # Safety
/// Nonempty input/output and all state/count pointers must be valid for their
/// declared lengths, appropriately aligned, nonoverlapping and exclusively
/// owned/pinned for this synchronous call. No pointer is retained. On rejection
/// no buffers, state or counters are changed. No allocation or I/O is performed.
#[no_mangle]
pub unsafe extern "C" fn lom_audio_mix_pcm16_v1(
    input: *const u8,
    input_frames: i32,
    channels: i32,
    step: f64,
    volume: f32,
    output: *mut f32,
    output_frames: i32,
    state: *mut MixState,
    consumed: *mut i32,
    produced: *mut i32,
) -> u32 {
    if input_frames < 0
        || output_frames < 0
        || !(channels == 1 || channels == 2)
        || !step.is_finite()
        || state.is_null()
        || !state.is_aligned()
        || consumed.is_null()
        || !consumed.is_aligned()
        || produced.is_null()
        || !produced.is_aligned()
        || (input_frames != 0 && input.is_null())
        || (output_frames != 0 && (output.is_null() || !output.is_aligned()))
        || input_frames as usize > isize::MAX as usize / (channels as usize * 2)
        || output_frames as usize > isize::MAX as usize / (2 * size_of::<f32>())
    {
        return REJECTED;
    }
    let mut current = unsafe { state.read() };
    if current.has_next > 1 || current.advance > 1 || !current.phase.is_finite() {
        return REJECTED;
    }
    let source = if input_frames == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(input, input_frames as usize * channels as usize * 2) }
    };
    let target = if output_frames == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(output, output_frames as usize * 2) }
    };
    let mut read = 0;
    let mut written = 0;
    let status = loop {
        if current.advance != 0 {
            while current.phase >= 1.0 {
                current.phase -= 1.0;
                if current.has_next == 0 {
                    unsafe {
                        state.write(current);
                        consumed.write(read);
                        produced.write(written);
                    }
                    return STOPPED;
                }
                current.left = current.next_left;
                current.right = current.next_right;
                if read == input_frames {
                    unsafe {
                        state.write(current);
                        consumed.write(read);
                        produced.write(written);
                    }
                    return NEED_READ;
                }
                let at = read as usize * channels as usize * 2;
                current.next_left =
                    i16::from_le_bytes([source[at], source[at + 1]]) as f32 / 32768.0;
                current.next_right = if channels == 1 {
                    current.next_left
                } else {
                    i16::from_le_bytes([source[at + 2], source[at + 3]]) as f32 / 32768.0
                };
                current.has_next = 1;
                read += 1;
            }
            current.advance = 0;
        }
        if written == output_frames {
            break DONE;
        }
        let fraction = current.phase as f32;
        let next_left = if current.has_next != 0 {
            current.next_left
        } else {
            current.left
        };
        let next_right = if current.has_next != 0 {
            current.next_right
        } else {
            current.right
        };
        // Unity Mono 5.11 evaluates the original float expression in Double:
        // only the explicit (float)phase and final stelem.r4 narrow precision.
        // Keep the output accumulation in that same tree; no FMA/fast math.
        target[written as usize * 2] = accumulate_mono(
            target[written as usize * 2],
            (current.left as f64 + (next_left as f64 - current.left as f64) * fraction as f64)
                * volume as f64,
        );
        target[written as usize * 2 + 1] = accumulate_mono(
            target[written as usize * 2 + 1],
            (current.right as f64 + (next_right as f64 - current.right as f64) * fraction as f64)
                * volume as f64,
        );
        written += 1;
        current.phase += step;
        current.advance = 1;
    };
    unsafe {
        state.write(current);
        consumed.write(read);
        produced.write(written);
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eof_after_final_output_still_promotes_before_host_read() {
        assert_eq!(size_of::<MixState>(), 32);
        let mut state = MixState {
            phase: 0.0,
            left: 0.25,
            right: 0.5,
            next_left: -0.25,
            next_right: -0.5,
            has_next: 1,
            advance: 0,
        };
        let mut output = [0.0; 2];
        let (mut consumed, mut produced) = (-1, -1);
        let status = unsafe {
            lom_audio_mix_pcm16_v1(
                std::ptr::null(),
                0,
                2,
                1.0,
                1.0,
                output.as_mut_ptr(),
                1,
                &mut state,
                &mut consumed,
                &mut produced,
            )
        };
        assert_eq!((status, consumed, produced), (NEED_READ, 0, 1));
        assert_eq!(output, [0.25, 0.5]);
        assert_eq!(
            (state.phase, state.left, state.right, state.advance),
            (0.0, -0.25, -0.5, 1)
        );
        state.has_next = 0;
        let status = unsafe {
            lom_audio_mix_pcm16_v1(
                std::ptr::null(),
                0,
                2,
                1.0,
                1.0,
                std::ptr::null_mut(),
                0,
                &mut state,
                &mut consumed,
                &mut produced,
            )
        };
        assert_eq!((status, consumed, produced), (DONE, 0, 0));
    }

    #[test]
    fn negative_lengths_are_rejected_without_mutating_counters() {
        let mut state = MixState {
            phase: 0.0,
            left: 0.0,
            right: 0.0,
            next_left: 0.0,
            next_right: 0.0,
            has_next: 0,
            advance: 0,
        };
        let (mut read, mut written) = (17, 19);
        let status = unsafe {
            lom_audio_mix_pcm16_v1(
                std::ptr::null(),
                -1,
                1,
                1.0,
                1.0,
                std::ptr::null_mut(),
                0,
                &mut state,
                &mut read,
                &mut written,
            )
        };
        assert_eq!((status, read, written), (REJECTED, 17, 19));
    }
}
