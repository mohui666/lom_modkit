//! NativePcmPlayer's sample conversion only. Decoding, framing, playback and
//! cursors remain in the managed owner; no buffers or pointers are retained.
use std::ptr;

#[inline]
fn sample_to_i16(mut sample: f32) -> i16 {
    if sample > 1.0 {
        sample = 1.0;
    }
    if sample < -1.0 {
        sample = -1.0;
    }
    // Unity Mono 5.11 keeps a double intermediate for the original
    // (short)(sample * 32767f). Rounding the product back to f32 first changes
    // quantization boundaries. Its unchecked conv.i2 produces zero for NaNs.
    (f64::from(sample) * 32767.0) as i16
}

#[inline]
unsafe fn write_sample(destination: *mut u8, index: usize, sample: f32) {
    let bytes = sample_to_i16(sample).to_le_bytes();
    unsafe {
        destination.add(index * 2).write(bytes[0]);
        destination.add(index * 2 + 1).write(bytes[1]);
    }
}

/// Convert decoded float samples to signed little-endian PCM16, preserving the
/// installed Unity Mono's double intermediate before truncation to i16. This
/// deliberately differs from runtimes which round the product to f32 first.
/// Returns the byte count, or -1 for invalid arguments without writing output.
/// Zero samples accept null pointers. Lengths/capacities count samples/bytes.
///
/// # Safety
/// Nonempty buffers must be valid for the supplied lengths and live until the
/// synchronous call returns. Source and destination must not be concurrently
/// mutated. Unaligned input/output addresses are accepted; nothing is retained.
#[no_mangle]
pub unsafe extern "C" fn lom_pcm_f32_to_i16_v1(
    samples: *const f32,
    sample_count: i32,
    destination: *mut u8,
    destination_capacity: i32,
) -> i32 {
    if sample_count < 0
        || sample_count > i32::MAX / 2
        || sample_count as usize > isize::MAX as usize / 4
        || destination_capacity < sample_count * 2
    {
        return -1;
    }
    if sample_count == 0 {
        return 0;
    }
    if samples.is_null() || destination.is_null() {
        return -1;
    }
    for index in 0..sample_count as usize {
        let sample = unsafe { samples.add(index).read_unaligned() };
        unsafe { write_sample(destination, index, sample) };
    }
    sample_count * 2
}

/// Convert an exact span of WAV samples to little-endian PCM16. Supported
/// sample widths are 1 (unsigned PCM8), 2 (unchanged PCM16), and 4 (float32),
/// using Unity Mono's double intermediate for the PCM16 scale multiplication.
/// WAV tag 1 with 32-bit samples intentionally keeps the historical float
/// interpretation; RIFF parsing and frame-tail truncation belong to the caller.
/// Returns the byte count, or -1 without writing for invalid lengths/formats.
///
/// # Safety
/// Source and destination must address initialized/readable and writable spans
/// for their declared lengths, with no concurrent mutation. Unaligned buffers
/// are accepted. Width 2 uses memmove semantics; other widths preserve the
/// managed loop's forward sample-by-sample read/write order even when aliased.
/// Nothing is allocated or retained. The caller must supply valid addresses;
/// non-null invalid addresses cannot be checked by this computation function.
#[no_mangle]
pub unsafe extern "C" fn lom_pcm_wav_to_i16_v1(
    source: *const u8,
    source_length: i32,
    bytes_per_sample: i32,
    destination: *mut u8,
    destination_capacity: i32,
) -> i32 {
    if source_length < 0
        || !matches!(bytes_per_sample, 1 | 2 | 4)
        || source_length % bytes_per_sample != 0
        || source_length / bytes_per_sample > i32::MAX / 2
    {
        return -1;
    }
    let samples = source_length / bytes_per_sample;
    let written = samples * 2;
    if destination_capacity < written {
        return -1;
    }
    if samples == 0 {
        return 0;
    }
    if source.is_null() || destination.is_null() {
        return -1;
    }
    if bytes_per_sample == 2 {
        unsafe { ptr::copy(source, destination, written as usize) };
    } else {
        for index in 0..samples as usize {
            let sample = if bytes_per_sample == 1 {
                (unsafe { source.add(index).read() } as f32 - 128.0) / 128.0
            } else {
                let bits = unsafe { source.add(index * 4).cast::<u32>().read_unaligned() };
                f32::from_bits(u32::from_le(bits))
            };
            unsafe { write_sample(destination, index, sample) };
        }
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_special_values_match_the_installed_mono_reference() {
        assert_eq!(sample_to_i16(f32::from_bits(0xbf7f_bfff)), -32734);
        let samples = [
            f32::NAN,
            f32::from_bits(0xff80_0001),
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.0,
            f32::from_bits(1),
            1.0,
            -1.0,
            f32::from_bits(0x3f7f_ffff),
            f32::from_bits(0xbf7f_ffff),
        ];
        let mut output = [0u8; 20];
        assert_eq!(
            unsafe { lom_pcm_f32_to_i16_v1(samples.as_ptr(), 10, output.as_mut_ptr(), 20) },
            20
        );
        let actual: Vec<_> = output
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
            .collect();
        assert_eq!(
            actual,
            [0, 0, 32767, -32767, 0, 0, 32767, -32767, 32766, -32766]
        );
    }

    #[test]
    fn pcm_rejection_is_atomic_and_pcm16_overlap_keeps_blockcopy_semantics() {
        let samples = [0.5f32];
        let mut output = [0x5au8; 8];
        for (length, width, capacity) in [(-1, 1, 8), (1, 3, 8), (3, 4, 8), (8, 4, 3)] {
            assert_eq!(
                unsafe {
                    lom_pcm_wav_to_i16_v1(
                        samples.as_ptr().cast(),
                        length,
                        width,
                        output.as_mut_ptr(),
                        capacity,
                    )
                },
                -1
            );
            assert_eq!(output, [0x5a; 8]);
        }
        assert_eq!(
            unsafe { lom_pcm_f32_to_i16_v1(samples.as_ptr(), 1, output.as_mut_ptr(), 1) },
            -1
        );
        assert_eq!(output, [0x5a; 8]);
        let mut bytes = [1u8, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(
            unsafe { lom_pcm_wav_to_i16_v1(bytes.as_ptr(), 6, 2, bytes.as_mut_ptr().add(2), 6) },
            6
        );
        assert_eq!(bytes, [1, 2, 1, 2, 3, 4, 5, 6]);
        assert_eq!(
            unsafe { lom_pcm_f32_to_i16_v1(ptr::null(), 0, ptr::null_mut(), 0) },
            0
        );
        assert_eq!(
            unsafe { lom_pcm_wav_to_i16_v1(ptr::null(), 0, 4, ptr::null_mut(), 0) },
            0
        );
    }
}
