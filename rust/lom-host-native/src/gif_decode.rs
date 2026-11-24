use crate::gif_image;
use std::{ptr, slice};

pub struct TextureFrames {
    frames: Vec<(f32, Vec<u8>)>,
}

#[repr(C)]
pub struct GifInfo {
    width: i32,
    height: i32,
    count: i32,
    loops: i32,
}

/// The input must be readable for its declared length during this call. The
/// returned handle owns the composed frames and must be dropped exactly once.
#[no_mangle]
pub unsafe extern "C" fn lom_gif_open_v1(
    bytes: *const u8,
    len: i32,
    info: *mut GifInfo,
) -> *mut TextureFrames {
    if bytes.is_null() || info.is_null() || len <= 0 {
        return ptr::null_mut();
    }
    let input = unsafe { slice::from_raw_parts(bytes, len as usize) };
    let Ok(image) = gif_image::decode(input, None) else {
        return ptr::null_mut();
    };
    let count = image.frames.len() as i32;
    let frames = image
        .frames
        .into_iter()
        .map(|frame| {
            let seconds = gif_image::frame_seconds(&frame);
            let mut pixels = frame.into_buffer();
            // Unity's raw rows start at the bottom of the canvas.
            image::imageops::flip_vertical_in_place(&mut pixels);
            (seconds, pixels.into_raw())
        })
        .collect();
    unsafe {
        *info = GifInfo {
            width: image.width as i32,
            height: image.height as i32,
            count,
            loops: image.loops as i32,
        }
    };
    Box::into_raw(Box::new(TextureFrames { frames }))
}

/// The returned pointer is borrowed until the owning handle is dropped.
#[no_mangle]
pub unsafe extern "C" fn lom_gif_frame_v1(
    handle: *const TextureFrames,
    index: i32,
    seconds: *mut f32,
) -> *const u8 {
    if handle.is_null() || seconds.is_null() || index < 0 {
        return ptr::null();
    }
    let Some((delay, pixels)) = (unsafe { &*handle }).frames.get(index as usize) else {
        return ptr::null();
    };
    unsafe { *seconds = *delay };
    pixels.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn lom_gif_drop_v1(handle: *mut TextureFrames) {
    if !handle.is_null() {
        unsafe { drop(Box::from_raw(handle)) };
    }
}
