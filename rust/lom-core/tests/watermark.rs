use image::{Rgb, RgbImage};
use lom_core::watermark::*;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02X}")).collect()
}
#[test]
fn protocol_and_carrier_cross_language_vectors() {
    let packet = encode_packet("demo_mod", 1).unwrap();
    assert_eq!(
        hex(&packet),
        "4C4F4D5701010000720435D441F942141A10BE8AA833C8741C08EE6D"
    );
    assert_eq!(
        decode_packet(&packet).unwrap().mod_id_hash_hex(),
        "720435D441F942141A10BE8AA833C874"
    );
    assert_eq!(
        bits_to_packet(&packet_to_bits(&packet).unwrap()).unwrap(),
        packet
    );
    let mut encoded = hamming_encode(&packet).unwrap();
    for offset in (0..encoded.len()).step_by(7) {
        encoded[offset + (offset / 7) % 7] ^= 1;
    }
    let (decoded, corrections) = hamming_decode(&encoded).unwrap();
    assert_eq!(decoded, packet);
    assert_eq!(corrections, 56);
    assert_eq!(
        &carrier_layout().0[..8],
        &[388, 301, 111, 85, 164, 305, 22, 72]
    );
    let tile = tile_rgba(&packet, 4).unwrap();
    assert_eq!(
        hex(&Sha256::digest(&tile)),
        "D075861FB031C39D390AD27C45C4FF3B858E7804CC0BC8510E3B75D5AA68831C"
    );
    let mut damaged = packet;
    damaged[12] ^= 1;
    assert!(!parse_packet(&damaged).unwrap().checksum_valid);
    assert!(decode_packet(&damaged).is_err());
    for id in ["", "Official.Mod", "../evil"] {
        assert!(encode_packet(id, 1).is_err());
    }
    assert!(encode_packet("demo_mod", 0).is_err());
    assert!(tile_rgba(&packet, 0).is_err());
    assert!(hamming_decode(&[0; ECC_BITS - 1]).is_err());
}
fn synthetic(marked: bool, index: usize) -> RgbImage {
    let tile = tile_rgba(&encode_packet("demo_mod", 1).unwrap(), 4).unwrap();
    RgbImage::from_fn((TILE_WIDTH * 2) as u32, (TILE_HEIGHT * 2) as u32, |x, y| {
        let base = 112.
            + 36. * ((x as f64 + index as f64 * 41.) / 67.).sin()
            + 28. * ((y as f64 - index as f64 * 27.) / 49.).cos()
            + 14. * ((x as f64 + y as f64) / 31.).sin();
        let channels = [
            base + 12. * (y as f64 / 23.).sin(),
            base,
            base - 10. * (x as f64 / 29.).cos(),
        ];
        let carrier =
            tile[((y as usize % TILE_HEIGHT) * TILE_WIDTH + x as usize % TILE_WIDTH) * 4] as f64;
        Rgb(channels.map(|v| {
            let result = if marked {
                v * (1. - 4. / 255.) + carrier * 4. / 255.
            } else {
                v
            };
            result.clamp(0., 255.) as u8
        }))
    })
}
fn assert_detected(path: &Path, scale: f64) {
    let result = detect_image(path, &[scale]).unwrap();
    assert_eq!(result["detected"], true, "{} {result}", path.display());
    assert_eq!(result["checksum_status"], "valid");
    assert_eq!(result["mod_hash"], "720435D441F942141A10BE8AA833C874");
}
#[test]
fn screenshot_transforms_and_clean_negative() {
    let dir = tempfile::tempdir().unwrap();
    let original = synthetic(true, 0);
    let png = dir.path().join("original.png");
    original.save(&png).unwrap();
    assert_detected(&png, 1.);
    let jpg = dir.path().join("compressed.jpg");
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
        std::fs::File::create(&jpg).unwrap(),
        85,
    );
    encoder.encode_image(&original).unwrap();
    assert_detected(&jpg, 1.);
    let resize = dir.path().join("resized.png");
    image::imageops::resize(&original, 672, 336, image::imageops::FilterType::CatmullRom)
        .save(&resize)
        .unwrap();
    assert_detected(&resize, 0.75);
    let crop = dir.path().join("crop.png");
    image::imageops::crop_imm(&original, 37, 23, 830, 408)
        .to_image()
        .save(&crop)
        .unwrap();
    assert_detected(&crop, 1.);
    for (name, multiply, offset) in [("brightness", 1.12, 0.), ("contrast", 0.85, 18.)] {
        let image = RgbImage::from_fn(original.width(), original.height(), |x, y| {
            Rgb(original
                .get_pixel(x, y)
                .0
                .map(|v| (v as f64 * multiply + offset).clamp(0., 255.) as u8))
        });
        let path = dir.path().join(format!("{name}.png"));
        image.save(&path).unwrap();
        assert_detected(&path, 1.);
    }
    let clean = dir.path().join("clean.png");
    synthetic(false, 0).save(&clean).unwrap();
    let result = detect_image(&clean, &[1.]).unwrap();
    assert_eq!(result["detected"], false);
    assert_eq!(result["mod_hash"], serde_json::Value::Null);
    assert!(detect_image(&png, &[f64::NAN]).is_err());
    assert!(detect_luminance(&[0.; 32 * 32], 32, 32, &[1.]).is_err());
}
#[test]
fn normalized_multiple_frames_recover_marked_only() {
    let dir = tempfile::tempdir().unwrap();
    for marked in [true, false] {
        let frames = (0..4)
            .map(|i| {
                let path = dir.path().join(format!("{marked}-{i}.png"));
                synthetic(marked, i).save(&path).unwrap();
                path
            })
            .collect::<Vec<_>>();
        let result = detect_video_frames(&frames, 2., &[1.]).unwrap();
        assert_eq!(result["detected"], marked, "{result}");
        assert_eq!(result["frames_sampled"], 4);
        if marked {
            assert_eq!(result["mod_hash"], "720435D441F942141A10BE8AA833C874");
        }
    }
}
#[test]
fn invalid_video_inputs_fail_explicitly() {
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("input.mp4");
    std::fs::write(&video, b"not a video").unwrap();
    assert!(detect_video(
        &video,
        Some(&dir.path().join("missing-ffmpeg")),
        2.,
        3,
        &[1.]
    )
    .unwrap_err()
    .to_string()
    .contains("FFmpeg"));
    assert!(detect_video(&video, None, 0., 3, &[1.]).is_err());
    assert!(detect_video(&video, None, 2., 0, &[1.]).is_err());
}
#[test]
#[ignore = "requires a FFmpeg executable; run with LOM_FFMPEG and --ignored"]
fn ffmpeg_real_video_extraction() {
    let dir = tempfile::tempdir().unwrap();
    let frames = (0..4)
        .map(|i| {
            let path = dir.path().join(format!("frame-{i:02}.png"));
            synthetic(true, i).save(&path).unwrap();
            path
        })
        .collect::<Vec<_>>();
    let executable = std::env::var_os("LOM_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let video = dir.path().join("source video.mkv");
    let status = std::process::Command::new(&executable)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-framerate",
            "1",
            "-i",
        ])
        .arg(dir.path().join("frame-%02d.png"))
        .args(["-c:v", "ffv1"])
        .arg(&video)
        .status()
        .unwrap();
    assert!(status.success());
    let result = detect_video(&video, Some(&executable), 1., frames.len(), &[1.]).unwrap();
    assert_eq!(result["detected"], true, "{result}");
    assert_eq!(result["frames_sampled"], 4);
    assert_eq!(result["mod_hash"], "720435D441F942141A10BE8AA833C874");
}
