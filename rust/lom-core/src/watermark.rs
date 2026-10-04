//! Protocol-compatible offline provenance watermark codec and image/video detector.
//! A valid CRC identifies a carrier payload; it does not authenticate an author.
use anyhow::{bail, ensure, Context, Result};
use image::{DynamicImage, ImageDecoder, ImageReader};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
    time::{Duration, Instant},
};

pub const PROTOCOL_VERSION: u8 = 1;
pub const ALGORITHM_VERSION: u8 = 1;
pub const PAYLOAD_SIZE: usize = 28;
pub const ECC_BITS: usize = 392;
pub const GRID_COLUMNS: usize = 28;
pub const GRID_ROWS: usize = 14;
pub const CELL_SIZE: usize = 16;
pub const TILE_WIDTH: usize = 448;
pub const TILE_HEIGHT: usize = 224;
pub const OVERLAY_ALPHA: u8 = 4;
pub const DEFAULT_SCALE_FACTORS: &[f64] = &[1.0, 0.75, 0.5, 1.25, 1.5, 2.0 / 3.0, 0.8, 1.2];
pub const MAX_IMAGE_PIXELS: usize = 50_000_000;
#[derive(Clone, Debug)]
pub struct WatermarkPacket {
    pub protocol_version: u8,
    pub algorithm_version: u8,
    pub flags: u8,
    pub mod_id_hash: [u8; 16],
    pub checksum: u32,
    pub checksum_valid: bool,
}
impl WatermarkPacket {
    pub fn mod_id_hash_hex(&self) -> String {
        hex(&self.mod_id_hash)
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|x| format!("{x:02X}")).collect()
}
pub fn mod_id_hash(id: &str) -> Result<[u8; 16]> {
    ensure!(
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == b'_' || x == b'-'),
        "水印 mod_id 必须匹配 [a-z0-9_-]{{1,64}}"
    );
    let mut hash = Sha256::new();
    hash.update(b"lom_modkit:watermark:mod-id:v1\0");
    hash.update(id.as_bytes());
    let digest = hash.finalize();
    Ok(digest[..16].try_into().unwrap())
}
pub fn encode_packet(id: &str, algorithm: u8) -> Result<[u8; PAYLOAD_SIZE]> {
    ensure!(algorithm > 0, "水印 algorithm_version 必须是 1~255 的整数");
    let mut out = [0; PAYLOAD_SIZE];
    out[..4].copy_from_slice(b"LOMW");
    out[4] = PROTOCOL_VERSION;
    out[5] = algorithm;
    out[8..24].copy_from_slice(&mod_id_hash(id)?);
    let checksum = crc32fast::hash(&out[..24]);
    out[24..].copy_from_slice(&checksum.to_be_bytes());
    Ok(out)
}
pub fn parse_packet(payload: &[u8]) -> Result<WatermarkPacket> {
    ensure!(
        payload.len() == PAYLOAD_SIZE,
        "水印 payload 必须恰好是 {PAYLOAD_SIZE} 字节"
    );
    ensure!(&payload[..4] == b"LOMW", "水印 magic 不匹配");
    ensure!(
        payload[4] == PROTOCOL_VERSION,
        "不支持的水印协议版本：{}",
        payload[4]
    );
    ensure!(payload[5] > 0, "水印 algorithm_version 不能为 0");
    ensure!(
        payload[6] == 0 && payload[7] == 0,
        "水印协议 v1 的 flags/reserved 必须为 0"
    );
    let checksum = u32::from_be_bytes(payload[24..].try_into()?);
    Ok(WatermarkPacket {
        protocol_version: payload[4],
        algorithm_version: payload[5],
        flags: payload[6],
        mod_id_hash: payload[8..24].try_into()?,
        checksum,
        checksum_valid: checksum == crc32fast::hash(&payload[..24]),
    })
}
pub fn decode_packet(payload: &[u8]) -> Result<WatermarkPacket> {
    let packet = parse_packet(payload)?;
    ensure!(packet.checksum_valid, "水印 payload CRC-32 校验失败");
    Ok(packet)
}
pub fn packet_to_bits(payload: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        payload.len() == PAYLOAD_SIZE,
        "水印 payload 必须恰好是 {PAYLOAD_SIZE} 字节"
    );
    Ok(payload
        .iter()
        .flat_map(|b| (0..8).rev().map(move |shift| (b >> shift) & 1))
        .collect())
}
pub fn bits_to_packet(bits: &[u8]) -> Result<[u8; PAYLOAD_SIZE]> {
    ensure!(
        bits.len() == PAYLOAD_SIZE * 8 && bits.iter().all(|b| *b <= 1),
        "水印 bit 序列必须恰好是 224 个 0/1"
    );
    let mut out = [0; PAYLOAD_SIZE];
    for (i, b) in bits.iter().enumerate() {
        out[i / 8] |= b << (7 - i % 8);
    }
    Ok(out)
}
pub fn hamming_encode(payload: &[u8]) -> Result<Vec<u8>> {
    let bits = packet_to_bits(payload)?;
    Ok(bits
        .chunks_exact(4)
        .flat_map(|b| {
            [
                b[0] ^ b[1] ^ b[3],
                b[0] ^ b[2] ^ b[3],
                b[0],
                b[1] ^ b[2] ^ b[3],
                b[1],
                b[2],
                b[3],
            ]
        })
        .collect())
}
pub fn hamming_decode(bits: &[u8]) -> Result<([u8; PAYLOAD_SIZE], usize)> {
    ensure!(
        bits.len() == ECC_BITS && bits.iter().all(|b| *b <= 1),
        "ECC 序列必须是 {ECC_BITS} 个 0/1"
    );
    let mut decoded = Vec::with_capacity(PAYLOAD_SIZE * 8);
    let mut corrections = 0;
    for word in bits.chunks_exact(7) {
        let mut w: [u8; 7] = word.try_into()?;
        let syndrome = (w[0] ^ w[2] ^ w[4] ^ w[6])
            | ((w[1] ^ w[2] ^ w[5] ^ w[6]) << 1)
            | ((w[3] ^ w[4] ^ w[5] ^ w[6]) << 2);
        if syndrome > 0 {
            w[syndrome as usize - 1] ^= 1;
            corrections += 1;
        }
        decoded.extend([w[2], w[4], w[5], w[6]]);
    }
    Ok((bits_to_packet(&decoded)?, corrections))
}
struct XorShift(u32);
impl XorShift {
    fn next(&mut self) -> u32 {
        let mut v = self.0;
        v ^= v << 13;
        v ^= v >> 17;
        v ^= v << 5;
        self.0 = v;
        v
    }
}
pub fn carrier_layout() -> &'static (Vec<usize>, Vec<i8>) {
    static LAYOUT: OnceLock<(Vec<usize>, Vec<i8>)> = OnceLock::new();
    LAYOUT.get_or_init(|| {
        let digest = Sha256::digest(b"lom_modkit:watermark:carrier-prng:algorithm:1");
        let seed = u32::from_be_bytes(digest[..4].try_into().unwrap());
        let mut rng = XorShift(if seed == 0 { 0x6D2B79F5 } else { seed });
        let mut cells = (0..ECC_BITS).collect::<Vec<_>>();
        for index in (1..ECC_BITS).rev() {
            let other = rng.next() as usize % (index + 1);
            cells.swap(index, other);
        }
        let polarity = (0..ECC_BITS)
            .map(|_| if rng.next() & 1 == 1 { 1 } else { -1 })
            .collect();
        (cells, polarity)
    })
}
pub fn carrier_signs(payload: &[u8]) -> Result<Vec<i8>> {
    let encoded = hamming_encode(payload)?;
    let (cells, polarity) = carrier_layout();
    let mut signs = vec![0; ECC_BITS];
    for (i, bit) in encoded.iter().enumerate() {
        signs[cells[i]] = if *bit == 1 { polarity[i] } else { -polarity[i] };
    }
    Ok(signs)
}
pub fn recover_ecc_bits(signs: &[i8]) -> Result<Vec<u8>> {
    ensure!(
        signs.len() == ECC_BITS && signs.iter().all(|s| [-1, 1].contains(s)),
        "载波判决必须是 {ECC_BITS} 个 -1/+1"
    );
    let (cells, polarity) = carrier_layout();
    Ok((0..ECC_BITS)
        .map(|i| u8::from(signs[cells[i]] * polarity[i] > 0))
        .collect())
}
pub fn tile_rgba(payload: &[u8], alpha: u8) -> Result<Vec<u8>> {
    ensure!((1..=16).contains(&alpha), "水印 overlay alpha 必须是 1~16");
    let signs = carrier_signs(payload)?;
    let mut out = vec![0; TILE_WIDTH * TILE_HEIGHT * 4];
    for y in 0..TILE_HEIGHT {
        for x in 0..TILE_WIDTH {
            let sign = signs[y / CELL_SIZE * GRID_COLUMNS + x / CELL_SIZE];
            let checker = if ((x % CELL_SIZE / 2 + y % CELL_SIZE / 2) & 1) == 0 {
                1
            } else {
                -1
            };
            let v = if sign * checker > 0 { 255 } else { 0 };
            out[(y * TILE_WIDTH + x) * 4..(y * TILE_WIDTH + x + 1) * 4]
                .copy_from_slice(&[v, v, v, alpha]);
        }
    }
    Ok(out)
}

#[derive(Clone)]
struct Candidate {
    sync: f64,
    origin: usize,
    shift_x: usize,
    shift_y: usize,
    observed: Vec<f64>,
    scale: f64,
}
fn header_template() -> Vec<(usize, f64)> {
    let mut p = [0; PAYLOAD_SIZE];
    p[..8].copy_from_slice(b"LOMW\x01\x01\0\0");
    let bits = hamming_encode(&p).unwrap();
    let (cells, polarity) = carrier_layout();
    let mut template = (0..112)
        .map(|i| {
            (
                cells[i],
                (if bits[i] == 1 { 1. } else { -1. }) * polarity[i] as f64,
            )
        })
        .collect::<Vec<_>>();
    template.sort_by_key(|x| x.0);
    template
}
fn canonical_index(cell: usize, sx: usize, sy: usize) -> usize {
    ((cell / GRID_COLUMNS + GRID_ROWS - sy) % GRID_ROWS) * GRID_COLUMNS
        + (cell % GRID_COLUMNS + GRID_COLUMNS - sx) % GRID_COLUMNS
}
// The four unique checker phases are processed separately so even large images
// require only one integral image in memory, rather than sixteen copies.
fn sync_candidates(values: &[f32], width: usize, height: usize, scale: f64) -> Vec<Candidate> {
    if width < TILE_WIDTH || height < TILE_HEIGHT {
        return Vec::new();
    }
    let template = header_template();
    let mut candidates = Vec::new();
    let stride = width + 1;
    for py in 0..2 {
        for px in 0..2 {
            let mut integral = vec![0f64; (height + 1) * stride];
            // Match NumPy cumsum(0).cumsum(1) summation order.
            for y in 0..height {
                let ys = if ((y as isize - py as isize).div_euclid(2) & 1) == 0 {
                    1.
                } else {
                    -1.
                };
                for x in 0..width {
                    let xs = if ((x as isize - px as isize).div_euclid(2) & 1) == 0 {
                        1.
                    } else {
                        -1.
                    };
                    integral[(y + 1) * stride + x + 1] =
                        values[y * width + x] as f64 * xs * ys + integral[y * stride + x + 1];
                }
            }
            for y in 1..=height {
                for x in 1..=width {
                    integral[y * stride + x] += integral[y * stride + x - 1];
                }
            }
            for oy in (py..CELL_SIZE).step_by(2) {
                for ox in (px..CELL_SIZE).step_by(2) {
                    let cols = (width - ox) / CELL_SIZE;
                    let rows = (height - oy) / CELL_SIZE;
                    if cols < GRID_COLUMNS || rows < GRID_ROWS {
                        continue;
                    }
                    let sign = if ((ox / 2 + oy / 2) & 1) == 0 {
                        1.
                    } else {
                        -1.
                    };
                    let mut observed = vec![0f64; ECC_BITS];
                    let mut counts = [0usize; ECC_BITS];
                    for row in 0..rows {
                        let y = oy + row * CELL_SIZE;
                        for col in 0..cols {
                            let x = ox + col * CELL_SIZE;
                            let score = integral[(y + CELL_SIZE) * stride + x + CELL_SIZE]
                                - integral[y * stride + x + CELL_SIZE]
                                - integral[(y + CELL_SIZE) * stride + x]
                                + integral[y * stride + x];
                            let cell = row % GRID_ROWS * GRID_COLUMNS + col % GRID_COLUMNS;
                            observed[cell] += score * sign;
                            counts[cell] += 1;
                        }
                    }
                    for i in 0..ECC_BITS {
                        observed[i] /= counts[i].max(1) as f64;
                    }
                    let mut local = Vec::new();
                    for sy in 0..GRID_ROWS {
                        for sx in 0..GRID_COLUMNS {
                            let mut numerator = 0.;
                            let mut norm = 0.;
                            for (cell, expected) in &template {
                                let v = observed[canonical_index(*cell, sx, sy)];
                                numerator += v * expected;
                                norm += v * v;
                            }
                            let denominator = norm.sqrt();
                            if denominator <= 1e-9 {
                                continue;
                            }
                            local.push((
                                numerator / (denominator * (template.len() as f64).sqrt()),
                                sx,
                                sy,
                            ));
                        }
                    }
                    local.sort_by(|a, b| b.0.total_cmp(&a.0));
                    for (score, sx, sy) in local.into_iter().take(2) {
                        candidates.push(Candidate {
                            sync: score,
                            origin: oy * CELL_SIZE + ox,
                            shift_x: sx,
                            shift_y: sy,
                            observed: observed.clone(),
                            scale,
                        });
                    }
                }
            }
        }
    }
    candidates.sort_by(|a, b| {
        b.sync
            .total_cmp(&a.sync)
            .then_with(|| a.origin.cmp(&b.origin))
            .then_with(|| a.shift_y.cmp(&b.shift_y))
            .then_with(|| a.shift_x.cmp(&b.shift_x))
    });
    candidates.truncate(12);
    candidates
}
struct Decoded {
    valid: bool,
    confidence: f64,
    sync: f64,
    corrections: usize,
    packet: Option<WatermarkPacket>,
    scale: f64,
}
fn decode_candidate(candidate: &Candidate) -> Result<Decoded> {
    let values = (0..ECC_BITS)
        .map(|cell| candidate.observed[canonical_index(cell, candidate.shift_x, candidate.shift_y)])
        .collect::<Vec<_>>();
    let signs = values
        .iter()
        .map(|v| if *v >= 0. { 1 } else { -1 })
        .collect::<Vec<_>>();
    let (payload, corrections) = hamming_decode(&recover_ecc_bits(&signs)?)?;
    let packet = parse_packet(&payload).ok();
    let expected = carrier_signs(&payload)?;
    let energy: f64 = values.iter().map(|v| v.abs()).sum();
    let agreement = if energy <= 1e-9 {
        0.5
    } else {
        values
            .iter()
            .zip(expected)
            .filter(|(v, s)| v.signum() == *s as f64 && **v != 0.)
            .map(|(v, _)| v.abs())
            .sum::<f64>()
            / energy
    };
    let carrier = ((agreement - 0.5) * 2.).clamp(0., 1.);
    let confidence = (0.35 * candidate.sync.max(0.) + 0.65 * carrier).clamp(0., 1.);
    let valid = packet.as_ref().is_some_and(|p| {
        p.checksum_valid
            && p.protocol_version == PROTOCOL_VERSION
            && p.algorithm_version == ALGORITHM_VERSION
    });
    Ok(Decoded {
        valid,
        confidence,
        sync: candidate.sync,
        corrections,
        packet,
        scale: candidate.scale,
    })
}
fn round6(v: f64) -> f64 {
    (v * 1e6).round_ties_even() / 1e6
}
fn image_bounds(width: usize, height: usize) -> Result<()> {
    ensure!(
        width.min(height) >= CELL_SIZE * 4,
        "截图尺寸过小，无法容纳水印载体"
    );
    ensure!(
        width
            .checked_mul(height)
            .is_some_and(|v| v <= MAX_IMAGE_PIXELS),
        "截图像素数超过 5000 万上限"
    );
    Ok(())
}
/// Native 2-D luminance detector. Array dimensions are explicit and length checked.
pub fn detect_luminance(
    values: &[f32],
    width: usize,
    height: usize,
    scales: &[f64],
) -> Result<Value> {
    image_bounds(width, height)?;
    ensure!(values.len() == width * height, "截图亮度数据尺寸不一致");
    ensure!(
        values.iter().all(|v| v.is_finite()),
        "截图亮度数据必须是有限值"
    );
    let source = values
        .iter()
        .map(|v| v.clamp(0., 255.) as u8)
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    let mut used = Vec::new();
    for factor in scales {
        ensure!(
            factor.is_finite() && *factor > 0.,
            "检测 scale factor 必须是正数"
        );
        let key = round6(*factor);
        if used.contains(&key) {
            continue;
        }
        used.push(key);
        if *factor == 1. {
            candidates.extend(sync_candidates(values, width, height, *factor));
        } else {
            let tw = ((width as f64 / factor).round_ties_even().max(1.)) as usize;
            let th = ((height as f64 / factor).round_ties_even().max(1.)) as usize;
            if tw.checked_mul(th).is_none_or(|v| v > MAX_IMAGE_PIXELS) {
                continue;
            }
            let resized = resize_bicubic(&source, width, height, tw, th);
            let normalized = resized.into_iter().map(|v| v as f32).collect::<Vec<_>>();
            candidates.extend(sync_candidates(&normalized, tw, th, *factor));
        }
    }
    candidates.sort_by(|a, b| b.sync.total_cmp(&a.sync));
    let decoded = candidates
        .iter()
        .take(64)
        .map(decode_candidate)
        .collect::<Result<Vec<_>>>()?;
    let best = decoded.iter().filter(|d| d.valid).max_by(|a, b| {
        a.confidence
            .total_cmp(&b.confidence)
            .then_with(|| a.sync.total_cmp(&b.sync))
    });
    if let Some(best) = best {
        let packet = best.packet.as_ref().unwrap();
        return Ok(
            json!({"detected":true,"confidence":round6(best.confidence),"protocol_version":packet.protocol_version,"algorithm_version":packet.algorithm_version,"mod_hash":packet.mod_id_hash_hex(),"checksum_status":"valid","ecc_status":if best.corrections==0{"clean"}else{"corrected"},"ecc_corrections":best.corrections,"scale_factor":round6(best.scale),"sync_score":round6(best.sync),"message":"检测到 lom_modkit 来源水印；它不是作者或官方认证"}),
        );
    }
    let best = decoded.iter().max_by(|a, b| {
        a.sync
            .total_cmp(&b.sync)
            .then_with(|| a.confidence.total_cmp(&b.confidence))
    });
    Ok(
        json!({"detected":false,"confidence":best.map_or(0.,|b|round6(b.confidence)),"protocol_version":null,"algorithm_version":null,"mod_hash":null,"checksum_status":if best.is_some_and(|b|b.packet.is_some()){"invalid"}else{"unavailable"},"ecc_status":"uncorrectable","ecc_corrections":best.map(|b|b.corrections),"scale_factor":best.map(|b|round6(b.scale)),"sync_score":best.map_or(0.,|b|round6(b.sync)),"message":"未检测到可通过协议与 CRC 校验的 lom_modkit 来源水印"}),
    )
}
// Pillow-compatible separable bicubic resampling for the detector's 8-bit L plane.
fn cubic(mut x: f64) -> f64 {
    x = x.abs();
    if x < 1. {
        ((1.5 * x - 2.5) * x) * x + 1.
    } else if x < 2. {
        (((-0.5 * x + 2.5) * x - 4.) * x) + 2.
    } else {
        0.
    }
}
fn coefficients(input: usize, output: usize) -> Vec<(usize, Vec<i64>)> {
    let scale = input as f64 / output as f64;
    let filter_scale = scale.max(1.);
    let support = 2. * filter_scale;
    (0..output)
        .map(|i| {
            let center = (i as f64 + 0.5) * scale;
            let first = ((center - support + 0.5) as isize).max(0) as usize;
            let end = ((center + support + 0.5) as usize).min(input);
            let weights = (first..end)
                .map(|x| cubic((x as f64 - center + 0.5) / filter_scale))
                .collect::<Vec<_>>();
            let sum: f64 = weights.iter().sum();
            (
                first,
                weights
                    .into_iter()
                    .map(|w| {
                        let value = if sum == 0. { w } else { w / sum };
                        (value * ((1u64 << 22) as f64) + if value < 0. { -0.5 } else { 0.5 }) as i64
                    })
                    .collect(),
            )
        })
        .collect()
}
fn resize_bicubic(
    source: &[u8],
    width: usize,
    height: usize,
    target_w: usize,
    target_h: usize,
) -> Vec<u8> {
    let horizontal = coefficients(width, target_w);
    let mut middle = vec![0u8; target_w * height];
    if width == target_w {
        middle.copy_from_slice(source);
    } else {
        for y in 0..height {
            for (x, (first, weights)) in horizontal.iter().enumerate() {
                let v = (1i64 << 21)
                    + weights
                        .iter()
                        .enumerate()
                        .map(|(i, w)| source[y * width + first + i] as i64 * w)
                        .sum::<i64>();
                middle[y * target_w + x] = (v >> 22).clamp(0, 255) as u8;
            }
        }
    }
    if height == target_h {
        return middle;
    }
    let vertical = coefficients(height, target_h);
    let mut out = vec![0; target_w * target_h];
    for (y, (first, weights)) in vertical.iter().enumerate() {
        for x in 0..target_w {
            let v = (1i64 << 21)
                + weights
                    .iter()
                    .enumerate()
                    .map(|(i, w)| middle[(first + i) * target_w + x] as i64 * w)
                    .sum::<i64>();
            out[y * target_w + x] = (v >> 22).clamp(0, 255) as u8;
        }
    }
    out
}
/// Decode bounded PNG/JPEG, applying the recorded EXIF orientation before detection.
pub fn load_image_luminance(path: &Path) -> Result<(Vec<f32>, usize, usize)> {
    ensure!(path.is_file(), "截图不存在：{}", path.display());
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    ensure!(
        matches!(
            reader.format(),
            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
        ),
        "截图检测器只接受 PNG 或 JPG"
    );
    let mut decoder = reader
        .into_decoder()
        .with_context(|| format!("无法读取截图 {}", path.display()))?;
    let (width, height) = decoder.dimensions();
    ensure!(
        width as u64 * height as u64 <= MAX_IMAGE_PIXELS as u64,
        "截图像素数超过 5000 万上限"
    );
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let rgb = image.to_rgb8();
    let (width, height) = (rgb.width() as usize, rgb.height() as usize);
    let values = rgb
        .pixels()
        .map(|p| p[0] as f32 * 0.2126 + p[1] as f32 * 0.7152 + p[2] as f32 * 0.0722)
        .collect();
    Ok((values, width, height))
}
pub fn detect_image(path: &Path, scales: &[f64]) -> Result<Value> {
    let (values, width, height) = load_image_luminance(path)?;
    detect_luminance(&values, width, height, scales)
}
pub fn detect_video_frames(paths: &[PathBuf], interval: f64, scales: &[f64]) -> Result<Value> {
    ensure!(!paths.is_empty(), "视频检测至少需要一帧");
    let mut accumulated = Vec::<f64>::new();
    let mut shape = (0, 0);
    for path in paths {
        let (values, width, height) = load_image_luminance(path)?;
        if accumulated.is_empty() {
            image_bounds(width, height)?;
            shape = (width, height);
            accumulated.resize(values.len(), 0.);
        } else {
            ensure!(
                shape == (width, height),
                "FFmpeg 提取帧尺寸不一致，无法做空间相关累积"
            );
        }
        let mean = values.iter().map(|v| *v as f64).sum::<f64>() / values.len() as f64;
        let deviation = (values
            .iter()
            .map(|v| (*v as f64 - mean).powi(2))
            .sum::<f64>()
            / values.len() as f64)
            .sqrt();
        let scale = if deviation > 1e-6 {
            32. / deviation
        } else {
            1.
        };
        for (i, v) in values.iter().enumerate() {
            accumulated[i] += ((*v - mean as f32) * scale as f32) as f64;
        }
    }
    let averaged = accumulated
        .into_iter()
        .map(|v| (v / paths.len() as f64 + 128.) as f32)
        .collect::<Vec<_>>();
    let mut result = detect_luminance(&averaged, shape.0, shape.1, scales)?;
    if result["detected"] == true {
        let confidence = result["confidence"].as_f64().unwrap_or(0.);
        result["confidence"] = json!(round6(
            1. - (1. - confidence).max(0.).powf((paths.len() as f64).sqrt())
        ));
    }
    result["frames_sampled"] = json!(paths.len());
    result["sample_interval_seconds"] = json!(interval);
    result["method"] = json!("ffmpeg-frame-extraction+normalized-luminance-correlation");
    result["message"] = json!(if result["detected"] == true {
        "多帧累积检测到 lom_modkit 来源水印；它不是作者或官方认证"
    } else {
        "多帧累积后未恢复出协议与 CRC 均有效的来源水印"
    });
    Ok(result)
}
pub fn detect_video(
    path: &Path,
    ffmpeg: Option<&Path>,
    interval: f64,
    max_frames: usize,
    scales: &[f64],
) -> Result<Value> {
    ensure!(path.is_file(), "视频不存在：{}", path.display());
    let suffix = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ensure!(
        ["mp4", "mkv", "mov", "webm", "avi", "m4v"].contains(&suffix.as_str()),
        "视频检测器只接受 MP4/MKV/MOV/WebM/AVI/M4V"
    );
    ensure!(
        path.metadata()?.len() <= 16 * 1024 * 1024 * 1024,
        "视频超过 16 GiB 离线检测上限"
    );
    ensure!(
        interval.is_finite() && (0.25..=60.).contains(&interval),
        "抽帧间隔必须是 0.25~60 秒"
    );
    ensure!((1..=120).contains(&max_frames), "最大抽帧数必须是 1~120");
    let directory = tempfile::Builder::new()
        .prefix("lom-watermark-video-")
        .tempdir()?;
    let executable = ffmpeg.unwrap_or_else(|| Path::new("ffmpeg"));
    let stderr = tempfile::tempfile()?;
    let mut child = Command::new(executable)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args([
            "-vf",
            &format!("fps=1/{interval}"),
            "-frames:v",
            &max_frames.to_string(),
            "-fps_mode",
            "vfr",
        ])
        .arg(directory.path().join("frame-%05d.png"))
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr.try_clone()?))
        .spawn()
        .with_context(|| {
            format!(
                "找不到或无法启动 FFmpeg：{}；请安装 FFmpeg 或用 --ffmpeg 指定可执行文件",
                executable.display()
            )
        })?;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(600) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("FFmpeg 抽帧超时（600秒）");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if !status.success() {
        use std::io::{Read, Seek, SeekFrom};
        let mut stderr = stderr;
        stderr.seek(SeekFrom::Start(0))?;
        let mut message = String::new();
        stderr.take(4800).read_to_string(&mut message)?;
        bail!(
            "FFmpeg 抽帧失败：{}",
            message.chars().take(1200).collect::<String>().trim()
        );
    }
    let mut frames = fs::read_dir(directory.path())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with("frame-") && s.ends_with(".png"))
        })
        .collect::<Vec<_>>();
    frames.sort();
    ensure!(!frames.is_empty(), "FFmpeg 未提取到可检测的视频帧");
    detect_video_frames(&frames, interval, scales)
}
