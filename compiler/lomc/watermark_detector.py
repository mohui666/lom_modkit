# -*- coding: utf-8 -*-
"""Offline screenshot detector for provenance watermark algorithm v1.

Image decoding and vector operations are optional detector-only dependencies.
The story compiler/packer stays standard-library-only.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
import json
import math
from pathlib import Path

from .errors import LomcError
from .watermark_codec import (
    ALGORITHM_VERSION,
    CELL_SIZE,
    ECC_BITS,
    GRID_COLUMNS,
    GRID_ROWS,
    carrier_layout,
    carrier_signs,
    hamming_decode,
)
from .watermark_protocol import MAGIC, PROTOCOL_VERSION, parse_packet


DEFAULT_SCALE_FACTORS = (1.0, 0.75, 0.5, 1.25, 1.5, 2.0 / 3.0, 0.8, 1.2)
MAX_IMAGE_PIXELS = 50_000_000
MIN_DIMENSION = CELL_SIZE * 4


@dataclass(frozen=True)
class DetectionResult:
    detected: bool
    confidence: float
    protocol_version: int | None
    algorithm_version: int | None
    mod_hash: str | None
    checksum_status: str
    ecc_status: str
    ecc_corrections: int | None
    scale_factor: float | None
    sync_score: float
    message: str

    def to_dict(self) -> dict:
        return asdict(self)

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), ensure_ascii=False, sort_keys=True)


def _dependencies():
    try:
        import numpy as np
        from PIL import Image, ImageOps
    except ImportError as exc:
        raise LomcError(
            "截图检测器需要 Pillow 与 NumPy；请安装 "
            "compiler/requirements-detector.txt"
        ) from exc
    return np, Image, ImageOps


def _known_header_template(np):
    # The first eight protocol bytes are fixed. Hamming(7,4) keeps the first
    # 112 encoded bits independent of the unknown Mod hash and CRC.
    header = MAGIC + bytes((PROTOCOL_VERSION, ALGORITHM_VERSION, 0, 0))
    bits = tuple((byte >> shift) & 1 for byte in header for shift in range(7, -1, -1))
    encoded = []
    for offset in range(0, len(bits), 4):
        d1, d2, d3, d4 = bits[offset : offset + 4]
        encoded.extend((d1 ^ d2 ^ d4, d1 ^ d3 ^ d4, d1,
                        d2 ^ d3 ^ d4, d2, d3, d4))
    cells, polarity = carrier_layout()
    template = np.zeros((GRID_ROWS, GRID_COLUMNS), dtype=np.float32)
    mask = np.zeros((GRID_ROWS, GRID_COLUMNS), dtype=np.bool_)
    for index, bit in enumerate(encoded):
        cell = cells[index]
        template[cell // GRID_COLUMNS, cell % GRID_COLUMNS] = (
            (1.0 if bit else -1.0) * polarity[index]
        )
        mask[cell // GRID_COLUMNS, cell % GRID_COLUMNS] = True
    return template, mask


def _integral_cell_scores(np, luminance, origin_x, origin_y, integrals):
    phase = (origin_x % 4, origin_y % 4)
    integral = integrals.get(phase)
    if integral is None:
        x = np.arange(luminance.shape[1], dtype=np.int32)
        y = np.arange(luminance.shape[0], dtype=np.int32)
        px = np.where((((x - origin_x) // 2) & 1) == 0, 1.0, -1.0)
        py = np.where((((y - origin_y) // 2) & 1) == 0, 1.0, -1.0)
        signed = luminance * py[:, None] * px[None, :]
        integral = np.pad(signed, ((1, 0), (1, 0))).cumsum(0).cumsum(1)
        integrals[phase] = integral

    xs = np.arange(origin_x, luminance.shape[1] - CELL_SIZE + 1, CELL_SIZE)
    ys = np.arange(origin_y, luminance.shape[0] - CELL_SIZE + 1, CELL_SIZE)
    if len(xs) < GRID_COLUMNS or len(ys) < GRID_ROWS:
        return None
    x0, x1 = xs, xs + CELL_SIZE
    y0, y1 = ys, ys + CELL_SIZE
    scores = (
        integral[np.ix_(y1, x1)]
        - integral[np.ix_(y0, x1)]
        - integral[np.ix_(y1, x0)]
        + integral[np.ix_(y0, x0)]
    )

    rows = np.arange(scores.shape[0])[:, None] % GRID_ROWS
    cols = np.arange(scores.shape[1])[None, :] % GRID_COLUMNS
    cell_ids = (rows * GRID_COLUMNS + cols).ravel()
    sums = np.bincount(cell_ids, weights=scores.ravel(), minlength=ECC_BITS)
    counts = np.bincount(cell_ids, minlength=ECC_BITS)
    return (sums / np.maximum(counts, 1)).reshape(GRID_ROWS, GRID_COLUMNS)


def _sync_candidates(np, luminance, scale_factor, limit=12):
    template, mask = _known_header_template(np)
    candidates = []
    integrals = {}
    for origin_y in range(CELL_SIZE):
        for origin_x in range(CELL_SIZE):
            observed = _integral_cell_scores(
                np, luminance, origin_x, origin_y, integrals
            )
            if observed is None:
                continue
            local = []
            for shift_y in range(GRID_ROWS):
                shifted_y = np.roll(observed, shift_y, axis=0)
                for shift_x in range(GRID_COLUMNS):
                    canonical = np.roll(shifted_y, shift_x, axis=1)
                    selected = canonical[mask]
                    denominator = math.sqrt(float(np.dot(selected, selected)))
                    if denominator <= 1e-9:
                        continue
                    score = float(np.dot(selected, template[mask])) / (
                        denominator * math.sqrt(float(mask.sum()))
                    )
                    local.append((score, origin_x, origin_y, shift_x, shift_y, observed))
            local.sort(key=lambda item: item[0], reverse=True)
            candidates.extend(local[:2])
    candidates.sort(key=lambda item: item[0], reverse=True)
    return [item + (scale_factor,) for item in candidates[:limit]]


def _decode_candidate(np, candidate):
    sync, origin_x, origin_y, shift_x, shift_y, observed, scale = candidate
    canonical = np.roll(observed, (shift_y, shift_x), axis=(0, 1))
    decisions = np.where(canonical.ravel() >= 0, 1, -1).tolist()
    from .watermark_codec import recover_ecc_bits

    payload, corrections = hamming_decode(recover_ecc_bits(decisions))
    packet = None
    parse_error = None
    try:
        packet = parse_packet(payload)
    except LomcError as exc:
        parse_error = str(exc)

    expected = np.asarray(carrier_signs(payload), dtype=np.float32)
    values = canonical.ravel()
    energy = float(np.abs(values).sum())
    weighted_agreement = 0.5 if energy <= 1e-9 else float(
        np.abs(values)[np.sign(values) == expected].sum() / energy
    )
    carrier_confidence = max(0.0, min(1.0, (weighted_agreement - 0.5) * 2.0))
    confidence = max(0.0, min(1.0, 0.35 * max(sync, 0.0) + 0.65 * carrier_confidence))
    valid = (
        packet is not None
        and packet.checksum_valid
        and packet.protocol_version == PROTOCOL_VERSION
        and packet.algorithm_version == ALGORITHM_VERSION
    )
    return {
        "valid": valid,
        "confidence": confidence,
        "sync": sync,
        "corrections": corrections,
        "packet": packet,
        "parse_error": parse_error,
        "scale": scale,
        "origin": (origin_x, origin_y),
        "shift": (shift_x, shift_y),
    }


def detect_luminance(luminance, scale_factors=DEFAULT_SCALE_FACTORS) -> DetectionResult:
    """Detect algorithm v1 from a 2-D NumPy luminance array."""
    np, Image, _ = _dependencies()
    values = np.asarray(luminance, dtype=np.float32)
    if values.ndim != 2:
        raise LomcError("截图亮度数据必须是二维数组")
    height, width = values.shape
    if min(width, height) < MIN_DIMENSION:
        raise LomcError("截图尺寸过小，无法容纳水印载体")
    if width * height > MAX_IMAGE_PIXELS:
        raise LomcError("截图像素数超过 5000 万上限")

    source = Image.fromarray(np.clip(values, 0, 255).astype(np.uint8), mode="L")
    all_candidates = []
    used = set()
    for factor in scale_factors:
        if isinstance(factor, bool) or not isinstance(factor, (int, float)) or factor <= 0:
            raise LomcError("检测 scale factor 必须是正数")
        factor = float(factor)
        key = round(factor, 6)
        if key in used:
            continue
        used.add(key)
        if factor == 1.0:
            normalized = values
        else:
            target = (max(1, round(width / factor)), max(1, round(height / factor)))
            if target[0] * target[1] > MAX_IMAGE_PIXELS:
                continue
            normalized = np.asarray(
                source.resize(target, Image.Resampling.BICUBIC), dtype=np.float32
            )
        all_candidates.extend(_sync_candidates(np, normalized, factor))

    all_candidates.sort(key=lambda item: item[0], reverse=True)
    decoded = [_decode_candidate(np, item) for item in all_candidates[:64]]
    valid = [item for item in decoded if item["valid"]]
    if valid:
        best = max(valid, key=lambda item: (item["confidence"], item["sync"]))
        packet = best["packet"]
        return DetectionResult(
            True,
            round(best["confidence"], 6),
            packet.protocol_version,
            packet.algorithm_version,
            packet.mod_id_hash_hex,
            "valid",
            "clean" if best["corrections"] == 0 else "corrected",
            best["corrections"],
            round(best["scale"], 6),
            round(best["sync"], 6),
            "检测到 lom_modkit 来源水印；它不是作者或官方认证",
        )

    best = max(decoded, key=lambda item: (item["sync"], item["confidence"]), default=None)
    return DetectionResult(
        False,
        round(best["confidence"], 6) if best else 0.0,
        None,
        None,
        None,
        "invalid" if best and best["packet"] is not None else "unavailable",
        "uncorrectable",
        best["corrections"] if best else None,
        round(best["scale"], 6) if best else None,
        round(best["sync"], 6) if best else 0.0,
        "未检测到可通过协议与 CRC 校验的 lom_modkit 来源水印",
    )


def detect_image(path, scale_factors=DEFAULT_SCALE_FACTORS) -> DetectionResult:
    np, Image, ImageOps = _dependencies()
    source = Path(path)
    if not source.is_file():
        raise LomcError(f"截图不存在：{source}")
    try:
        with Image.open(source) as opened:
            if opened.format not in ("PNG", "JPEG"):
                raise LomcError("截图检测器只接受 PNG 或 JPG")
            image = ImageOps.exif_transpose(opened).convert("RGB")
    except LomcError:
        raise
    except Exception as exc:
        raise LomcError(f"无法读取截图 {source}：{exc}") from exc
    width, height = image.size
    if width * height > MAX_IMAGE_PIXELS:
        raise LomcError("截图像素数超过 5000 万上限")
    rgb = np.asarray(image, dtype=np.float32)
    luminance = rgb[:, :, 0] * 0.2126 + rgb[:, :, 1] * 0.7152 + rgb[:, :, 2] * 0.0722
    return detect_luminance(luminance, scale_factors=scale_factors)
