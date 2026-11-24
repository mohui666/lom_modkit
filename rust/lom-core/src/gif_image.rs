//! Shared GIF canvas composition for import, preview, and the Windows Host.
use image::{
    codecs::gif::GifDecoder, metadata::LoopCount, AnimationDecoder, Frame, Frames, ImageDecoder,
    ImageError, ImageResult, Limits,
};
use std::io::Cursor;

// GIF compression can represent many full-size canvases in a small input file.
const MAX_RGBA_BYTES: u64 = 256 * 1024 * 1024;

pub struct Animation {
    pub width: u32,
    pub height: u32,
    pub loops: u32,
    pub frames: Vec<Frame>,
}

pub fn is_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

pub fn frame_seconds(frame: &Frame) -> f32 {
    let (numerator, denominator) = frame.delay().numer_denom_ms();
    (numerator as f32 / denominator as f32 / 1000.0).max(0.01)
}

fn limit_error() -> ImageError {
    ImageError::Limits(image::error::LimitError::from_kind(
        image::error::LimitErrorKind::InsufficientMemory,
    ))
}

fn read(bytes: &[u8]) -> ImageResult<(u32, u32, u32, Frames<'_>)> {
    let mut decoder = GifDecoder::new(Cursor::new(bytes))?;
    let (width, height) = decoder.dimensions();
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_RGBA_BYTES);
    decoder.set_limits(limits)?;
    let loops = match decoder.loop_count() {
        LoopCount::Infinite => 0,
        LoopCount::Finite(count) => count.get(),
    };
    Ok((width, height, loops, decoder.into_frames()))
}

fn no_frames() -> ImageError {
    ImageError::Decoding(image::error::DecodingError::new(
        image::ImageFormat::Gif.into(),
        "GIF 没有可显示的帧",
    ))
}

pub fn first_frame(bytes: &[u8]) -> ImageResult<Frame> {
    read(bytes)?.3.next().transpose()?.ok_or_else(no_frames)
}

fn bounded_frames(
    width: u32,
    height: u32,
    frames: Frames<'_>,
) -> impl Iterator<Item = ImageResult<Frame>> + '_ {
    let canvas_bytes = u64::from(width) * u64::from(height) * 4;
    frames.enumerate().map(move |(index, frame)| {
        if (index as u64 + 1).saturating_mul(canvas_bytes) > MAX_RGBA_BYTES {
            return Err(limit_error());
        }
        frame
    })
}

pub fn validate(bytes: &[u8]) -> ImageResult<()> {
    let (width, height, _, frames) = read(bytes)?;
    let mut frames = bounded_frames(width, height, frames);
    frames.next().transpose()?.ok_or_else(no_frames)?;
    frames.try_for_each(|frame| frame.map(|_| ()))
}

pub fn decode(bytes: &[u8], max_side: Option<u32>) -> ImageResult<Animation> {
    let (width, height, loops, frames) = read(bytes)?;
    let mut result = Vec::new();
    for frame in bounded_frames(width, height, frames) {
        let frame = frame?;
        let resized = max_side.filter(|side| width > *side || height > *side);
        result.push(if let Some(side) = resized {
            let delay = frame.delay();
            let pixels = image::DynamicImage::ImageRgba8(frame.into_buffer())
                .thumbnail(side, side)
                .into_rgba8();
            Frame::from_parts(pixels, 0, 0, delay)
        } else {
            frame
        });
    }
    if result.is_empty() {
        return Err(no_frames());
    }
    Ok(Animation {
        width,
        height,
        loops,
        frames: result,
    })
}
