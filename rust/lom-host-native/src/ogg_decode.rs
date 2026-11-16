//! Stateful Vorbis decoding. The managed owner retains worker/device/loop
//! lifetime. Encoded input is copied once; no managed pointer is retained.
use lewton::{
    audio::{read_audio_packet_generic, PreviousWindowRight},
    header::{read_header_ident, read_header_setup, IdentHeader, SetupHeader},
};
use std::{
    io::{self, Cursor, Read, Seek, SeekFrom},
    panic::{catch_unwind, AssertUnwindSafe},
    ptr, slice,
    sync::Arc,
};
use symphonia::core::{
    checksum::Crc32,
    codecs::CODEC_TYPE_VORBIS,
    errors::Error,
    formats::{FormatOptions, FormatReader},
    io::{MediaSource, MediaSourceStream, Monitor},
    meta::MetadataOptions,
    probe::Hint,
};

const BAD_ARGUMENT: i32 = -1;
const INVALID_AUDIO: i32 = -2;
const DECODER_FAILED: i32 = -3;

struct Input {
    bytes: Arc<[u8]>,
    cursor: Cursor<Arc<[u8]>>,
    length: usize,
}
impl Read for Input {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        let at = self.cursor.position().min(self.length as u64) as usize;
        let count = destination.len().min(self.length - at);
        destination[..count].copy_from_slice(&self.bytes[at..at + count]);
        self.cursor.set_position((at + count) as u64);
        Ok(count)
    }
}
impl Seek for Input {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.cursor.seek(position)
    }
}
impl MediaSource for Input {
    // Probe only forwards. Seeking to a corrupt final page before the first
    // read would otherwise discard a valid prefix which NVorbis used to play.
    fn is_seekable(&self) -> bool {
        false
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.length as u64)
    }
}

#[derive(Debug)]
struct PageIndex {
    length: usize,
    first_vorbis: Option<u32>,
    end_granule: Option<u64>,
}

/// This is a page-boundary index, not a second packet/codec decoder. Use the
/// demuxer's own Ogg CRC implementation. NVorbis stops at the first incomplete
/// or bad-CRC page; allowing the demuxer to resync past it changes playback.
fn index_pages(bytes: &[u8]) -> PageIndex {
    let mut index = PageIndex {
        length: 0,
        first_vorbis: None,
        end_granule: None,
    };
    let mut at = 0;
    let mut found_page = false;
    while at + 27 <= bytes.len() {
        if &bytes[at..at + 4] != b"OggS" || bytes[at + 4] != 0 {
            at += 1;
            continue;
        }
        let header = at + 27 + bytes[at + 26] as usize;
        if header > bytes.len() {
            if found_page {
                break;
            }
            at += 1;
            continue;
        }
        let end = header
            + bytes[at + 27..header]
                .iter()
                .map(|&value| value as usize)
                .sum::<usize>();
        if end > bytes.len() {
            if found_page {
                break;
            }
            at += 1;
            continue;
        }
        let mut crc = Crc32::new(0);
        crc.process_buf_bytes(&bytes[at..at + 22]);
        crc.process_buf_bytes(&[0; 4]);
        crc.process_buf_bytes(&bytes[at + 26..end]);
        if crc.crc() != u32::from_le_bytes(bytes[at + 22..at + 26].try_into().unwrap()) {
            if found_page {
                break;
            }
            at += 1;
            continue;
        }
        found_page = true;
        index.length = end;
        let serial = u32::from_le_bytes(bytes[at + 14..at + 18].try_into().unwrap());
        if index.first_vorbis.is_none()
            && bytes[at + 5] & 2 != 0
            && bytes[header..end].starts_with(b"\x01vorbis")
        {
            index.first_vorbis = Some(serial);
        }
        if Some(serial) == index.first_vorbis && bytes[at + 5] & 4 != 0 {
            let granule = u64::from_le_bytes(bytes[at + 6..at + 14].try_into().unwrap());
            index.end_granule = (granule != u64::MAX).then_some(granule);
            break; // The original OggSource selects only the first logical stream.
        }
        at = end;
    }
    index
}

fn container(bytes: Arc<[u8]>, index: &PageIndex) -> Result<(Box<dyn FormatReader>, u32), i32> {
    let input = Input {
        cursor: Cursor::new(bytes.clone()),
        bytes,
        length: index.length,
    };
    let media = MediaSourceStream::new(Box::new(input), Default::default());
    let probe = symphonia::default::get_probe()
        .format(
            &Hint::new().with_extension("ogg"),
            media,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| INVALID_AUDIO)?;
    let track = probe
        .format
        .tracks()
        .iter()
        .find(|track| {
            track.codec_params.codec == CODEC_TYPE_VORBIS
                && index.first_vorbis.is_none_or(|serial| track.id == serial)
        })
        .ok_or(INVALID_AUDIO)?;
    let id = track.id;
    Ok((probe.format, id))
}

pub struct OggDecoder {
    bytes: Arc<[u8]>,
    index: PageIndex,
    format: Box<dyn FormatReader>,
    track: u32,
    ident: IdentHeader,
    setup: SetupHeader,
    window: PreviousWindowRight,
    pending: Vec<Vec<f32>>,
    pending_at: usize,
    emitted_frames: u64,
    done: bool,
    failure: i32,
}
impl OggDecoder {
    fn open(input: &[u8]) -> Result<Self, i32> {
        let bytes: Arc<[u8]> = Arc::from(input);
        let index = index_pages(&bytes);
        let (format, track) = container(bytes.clone(), &index)?;
        let parameters = &format
            .tracks()
            .iter()
            .find(|entry| entry.id == track)
            .ok_or(INVALID_AUDIO)?
            .codec_params;
        let extra = parameters.extra_data.as_ref().ok_or(INVALID_AUDIO)?;
        let ident =
            read_header_ident(extra.get(..30).ok_or(INVALID_AUDIO)?).map_err(|_| INVALID_AUDIO)?;
        if ident.audio_channels == 0
            || ident.audio_sample_rate == 0
            || ident.audio_sample_rate > i32::MAX as u32
        {
            return Err(INVALID_AUDIO);
        }
        let setup = read_header_setup(
            &extra[30..],
            ident.audio_channels,
            (ident.blocksize_0, ident.blocksize_1),
        )
        .map_err(|_| INVALID_AUDIO)?;
        Ok(Self {
            bytes,
            index,
            format,
            track,
            ident,
            setup,
            window: PreviousWindowRight::new(),
            pending: Vec::new(),
            pending_at: 0,
            emitted_frames: 0,
            done: false,
            failure: 0,
        })
    }

    fn packet(&mut self) -> Result<bool, i32> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(Error::ResetRequired) => return Ok(false),
                Err(Error::IoError(error)) if error.kind() == io::ErrorKind::UnexpectedEof => {
                    return Ok(false)
                }
                Err(_) => return Err(INVALID_AUDIO),
            };
            if packet.track_id() != self.track {
                continue;
            }
            self.pending =
                read_audio_packet_generic(&self.ident, &self.setup, &packet.data, &mut self.window)
                    .map_err(|_| INVALID_AUDIO)?;
            self.pending_at = 0;
            if self.pending.first().is_some_and(|plane| !plane.is_empty()) {
                return Ok(true);
            }
        }
    }

    unsafe fn read(&mut self, destination: *mut u8, capacity: usize) -> i32 {
        if self.failure != 0 {
            return self.failure;
        }
        let channels = self.ident.audio_channels as usize;
        let frames = capacity / (channels * 2);
        let mut written = 0;
        while written < frames && !self.done {
            if self
                .index
                .end_granule
                .is_some_and(|end| self.emitted_frames >= end)
            {
                self.done = true;
                break;
            }
            if self
                .pending
                .first()
                .is_none_or(|plane| self.pending_at >= plane.len())
            {
                match self.packet() {
                    Ok(true) => {}
                    Ok(false) => {
                        self.done = true;
                        break;
                    }
                    Err(error) => {
                        self.failure = error;
                        break;
                    }
                }
            }
            let count = (frames - written)
                .min(self.pending[0].len() - self.pending_at)
                .min(self.index.end_granule.map_or(usize::MAX, |end| {
                    (end - self.emitted_frames).min(usize::MAX as u64) as usize
                }));
            for frame in 0..count {
                for channel in 0..channels {
                    let sample = legacy_pcm(self.pending[channel][self.pending_at + frame]);
                    let at = ((written + frame) * channels + channel) * 2;
                    unsafe {
                        destination.add(at).write(sample[0]);
                        destination.add(at + 1).write(sample[1]);
                    }
                }
            }
            self.pending_at += count;
            self.emitted_frames += count as u64;
            written += count;
        }
        if written == 0 && self.failure != 0 {
            self.failure
        } else {
            (written * channels * 2) as i32
        }
    }

    fn rewind(&mut self) -> Result<(), i32> {
        let (format, track) = container(self.bytes.clone(), &self.index)?;
        self.format = format;
        self.track = track;
        self.window = PreviousWindowRight::new();
        self.pending.clear();
        self.pending_at = 0;
        self.emitted_frames = 0;
        self.done = false;
        self.failure = 0;
        Ok(())
    }
}

fn legacy_pcm(sample: f32) -> [u8; 2] {
    // NVorbis.ClipSamples clamps float bits to ±0x3F7FFFFF, including NaNs.
    let bits = sample.to_bits();
    let clipped = if bits & 0x7fff_ffff > 0x3f7f_ffff {
        f32::from_bits(0x3f7f_ffff | (bits & 0x8000_0000))
    } else {
        sample
    };
    // Preserve the original Unity Mono double intermediate before conv.i2.
    ((f64::from(clipped) * 32767.0) as i16).to_le_bytes()
}

/// Create an owned streaming decoder. Status 0 means success; -1 invalid ABI,
/// -2 invalid/unsupported audio, -3 an isolated decoder panic. No failure calls
/// the managed decoder. Source is borrowed only for this call and copied once.
/// # Safety
/// Nonempty input must be readable for its declared length. Three output
/// pointers must be aligned/writable. Successful handles must be dropped once.
#[no_mangle]
pub unsafe extern "C" fn lom_ogg_open_v1(
    source: *const u8,
    length: i32,
    handle: *mut *mut OggDecoder,
    channels: *mut i32,
    rate: *mut i32,
) -> i32 {
    if handle.is_null()
        || channels.is_null()
        || rate.is_null()
        || !handle.is_aligned()
        || !channels.is_aligned()
        || !rate.is_aligned()
    {
        return BAD_ARGUMENT;
    }
    unsafe {
        handle.write(ptr::null_mut());
        channels.write(0);
        rate.write(0);
    }
    if source.is_null() || length <= 0 {
        return BAD_ARGUMENT;
    }
    let input = unsafe { slice::from_raw_parts(source, length as usize) };
    match catch_unwind(AssertUnwindSafe(|| OggDecoder::open(input))) {
        Ok(Ok(decoder)) => {
            unsafe {
                channels.write(i32::from(decoder.ident.audio_channels));
                rate.write(decoder.ident.audio_sample_rate as i32);
                handle.write(Box::into_raw(Box::new(decoder)));
            }
            0
        }
        Ok(Err(error)) => error,
        Err(_) => DECODER_FAILED,
    }
}

/// Read whole interleaved PCM16 frames directly into the caller's buffer.
/// Returns bytes written or a negative status; zero means selected stream EOF.
/// # Safety
/// Handle must be live and exclusively used until this call returns. Output
/// must be writable for capacity bytes and not alias the decoder. No retention.
#[no_mangle]
pub unsafe extern "C" fn lom_ogg_read_pcm16_v1(
    handle: *mut OggDecoder,
    output: *mut u8,
    capacity: i32,
) -> i32 {
    if handle.is_null() || capacity < 0 || capacity > 0 && output.is_null() {
        return BAD_ARGUMENT;
    }
    if capacity == 0 {
        return 0;
    }
    let decoder = unsafe { &mut *handle };
    match catch_unwind(AssertUnwindSafe(|| unsafe {
        decoder.read(output, capacity as usize)
    })) {
        Ok(result) => result,
        Err(_) => {
            decoder.failure = DECODER_FAILED;
            DECODER_FAILED
        }
    }
}

/// Rewind the selected logical stream, retaining input and immutable codebooks.
/// # Safety
/// Handle must be live and exclusively used until this synchronous call returns.
#[no_mangle]
pub unsafe extern "C" fn lom_ogg_rewind_v1(handle: *mut OggDecoder) -> i32 {
    if handle.is_null() {
        return BAD_ARGUMENT;
    }
    let decoder = unsafe { &mut *handle };
    match catch_unwind(AssertUnwindSafe(|| decoder.rewind())) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            decoder.failure = error;
            error
        }
        Err(_) => {
            decoder.failure = DECODER_FAILED;
            DECODER_FAILED
        }
    }
}

/// # Safety
/// Drop exactly one successful handle, after reads/rewinds have finished. Null
/// is a no-op; the encoded bytes, stream state and codebooks are all released.
#[no_mangle]
pub unsafe extern "C" fn lom_ogg_drop_v1(handle: *mut OggDecoder) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page(serial: u32, flags: u8, granule: u64, body: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 28];
        bytes[..4].copy_from_slice(b"OggS");
        bytes[5] = flags;
        bytes[6..14].copy_from_slice(&granule.to_le_bytes());
        bytes[14..18].copy_from_slice(&serial.to_le_bytes());
        bytes[26] = 1;
        bytes[27] = body.len() as u8;
        bytes.extend_from_slice(body);
        let mut crc = Crc32::new(0);
        crc.process_buf_bytes(&bytes);
        bytes[22..26].copy_from_slice(&crc.crc().to_le_bytes());
        bytes
    }
    #[test]
    fn ogg_index_stops_at_first_stream_eos_or_bad_page() {
        let first = page(9, 2, 0, b"\x01vorbis");
        let last = page(9, 4, 17, &[0]);
        let mut bytes = b"prefix".to_vec();
        bytes.extend(&first);
        bytes.extend(&last);
        let end = bytes.len();
        bytes.extend(page(10, 2 | 4, 500, b"\x01vorbis"));
        let index = index_pages(&bytes);
        assert_eq!(index.length, end);
        assert_eq!(index.end_granule, Some(17));
        let mut damaged = first.clone();
        let mut bad = last.clone();
        *bad.last_mut().unwrap() ^= 1;
        damaged.extend(bad);
        damaged.extend(last);
        assert_eq!(index_pages(&damaged).length, first.len());
        assert_eq!(index_pages(&damaged).end_granule, None);
        assert_eq!(index_pages(&first[..first.len() - 1]).length, 0);
    }
    #[test]
    fn ogg_clip_keeps_nvorbis_half_open_range_and_mono_quantization() {
        assert_eq!(i16::from_le_bytes(legacy_pcm(1.0)), 32766);
        assert_eq!(i16::from_le_bytes(legacy_pcm(-1.0)), -32766);
        assert_eq!(i16::from_le_bytes(legacy_pcm(f32::NAN)), 32766);
        assert_eq!(i16::from_le_bytes(legacy_pcm(f32::NEG_INFINITY)), -32766);
        assert_eq!(
            i16::from_le_bytes(legacy_pcm(f32::from_bits(0xbf7f_bfff))),
            -32734
        );
    }
}
