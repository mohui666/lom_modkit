# -*- coding: utf-8 -*-
"""Offline FFmpeg + multi-frame provenance detector. No ML is used."""

from __future__ import annotations

from dataclasses import asdict, dataclass
import json
import math
from pathlib import Path
import shutil
import subprocess
import tempfile

from .errors import LomcError
from .watermark_detector import (
    DEFAULT_SCALE_FACTORS,
    DetectionResult,
    detect_luminance,
    load_image_luminance,
)


MAX_VIDEO_BYTES = 16 * 1024 * 1024 * 1024
SUPPORTED_SUFFIXES = {".mp4", ".mkv", ".mov", ".webm", ".avi", ".m4v"}


@dataclass(frozen=True)
class VideoDetectionResult:
    detected: bool
    confidence: float
    protocol_version: int | None
    algorithm_version: int | None
    mod_hash: str | None
    checksum_status: str
    ecc_status: str
    ecc_corrections: int | None
    frames_sampled: int
    sample_interval_seconds: float
    scale_factor: float | None
    sync_score: float
    method: str
    message: str

    def to_dict(self) -> dict:
        return asdict(self)

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), ensure_ascii=False, sort_keys=True)


def _resolve_ffmpeg(executable: str | None) -> str:
    candidate = executable or "ffmpeg"
    resolved = shutil.which(candidate)
    if resolved:
        return resolved
    path = Path(candidate)
    if path.is_file():
        return str(path.resolve())
    raise LomcError("找不到 FFmpeg；请安装 FFmpeg 或用 --ffmpeg 指定可执行文件")


def _extract_frames(video: Path, output_dir: Path, ffmpeg: str,
                    interval: float, max_frames: int) -> list[Path]:
    pattern = output_dir / "frame-%05d.png"
    command = [
        ffmpeg,
        "-hide_banner",
        "-loglevel", "error",
        "-i", str(video),
        "-vf", "fps=1/%s" % format(interval, ".6g"),
        "-frames:v", str(max_frames),
        "-vsync", "vfr",
        str(pattern),
    ]
    try:
        completed = subprocess.run(
            command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, check=False, timeout=600,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise LomcError(f"FFmpeg 抽帧失败：{exc}") from exc
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout or "未知错误").strip()
        raise LomcError("FFmpeg 抽帧失败：" + detail[:1200])
    frames = sorted(output_dir.glob("frame-*.png"))
    if not frames:
        raise LomcError("FFmpeg 未提取到可检测的视频帧")
    return frames


def detect_video_frames(frame_paths, interval=2.0,
                        scale_factors=DEFAULT_SCALE_FACTORS) -> VideoDetectionResult:
    """Accumulate aligned frame luminance before running carrier correlation."""
    try:
        import numpy as np
    except ImportError as exc:
        raise LomcError(
            "视频检测器需要 Pillow 与 NumPy；请安装 "
            "compiler/requirements-detector.txt"
        ) from exc

    paths = [Path(path) for path in frame_paths]
    if not paths:
        raise LomcError("视频检测至少需要一帧")
    accumulated = None
    shape = None
    for path in paths:
        luminance = load_image_luminance(path)
        if shape is None:
            shape = luminance.shape
        elif luminance.shape != shape:
            raise LomcError("FFmpeg 提取帧尺寸不一致，无法做空间相关累积")
        mean = float(luminance.mean())
        deviation = float(luminance.std())
        normalized = luminance - mean
        if deviation > 1e-6:
            normalized *= 32.0 / deviation
        if accumulated is None:
            accumulated = normalized.astype(np.float64)
        else:
            accumulated += normalized
    accumulated = (accumulated / len(paths) + 128.0).astype(np.float32)
    frame_result: DetectionResult = detect_luminance(
        accumulated, scale_factors=scale_factors
    )
    # Repeated aligned evidence improves confidence, but never turns an invalid
    # protocol/CRC result into a detection.
    accumulated_confidence = 1.0 - math.pow(
        max(0.0, 1.0 - frame_result.confidence), math.sqrt(len(paths))
    )
    return VideoDetectionResult(
        frame_result.detected,
        round(accumulated_confidence if frame_result.detected else frame_result.confidence, 6),
        frame_result.protocol_version,
        frame_result.algorithm_version,
        frame_result.mod_hash,
        frame_result.checksum_status,
        frame_result.ecc_status,
        frame_result.ecc_corrections,
        len(paths),
        float(interval),
        frame_result.scale_factor,
        frame_result.sync_score,
        "ffmpeg-frame-extraction+normalized-luminance-correlation",
        (
            "多帧累积检测到 lom_modkit 来源水印；它不是作者或官方认证"
            if frame_result.detected
            else "多帧累积后未恢复出协议与 CRC 均有效的来源水印"
        ),
    )


def detect_video(path, ffmpeg=None, interval=2.0, max_frames=12,
                 scale_factors=DEFAULT_SCALE_FACTORS) -> VideoDetectionResult:
    source = Path(path)
    if not source.is_file():
        raise LomcError(f"视频不存在：{source}")
    if source.suffix.lower() not in SUPPORTED_SUFFIXES:
        raise LomcError("视频检测器只接受 MP4/MKV/MOV/WebM/AVI/M4V")
    if source.stat().st_size > MAX_VIDEO_BYTES:
        raise LomcError("视频超过 16 GiB 离线检测上限")
    if isinstance(interval, bool) or not isinstance(interval, (int, float)) \
            or not 0.25 <= float(interval) <= 60.0:
        raise LomcError("抽帧间隔必须是 0.25~60 秒")
    if isinstance(max_frames, bool) or not isinstance(max_frames, int) \
            or not 1 <= max_frames <= 120:
        raise LomcError("最大抽帧数必须是 1~120")
    executable = _resolve_ffmpeg(ffmpeg)
    with tempfile.TemporaryDirectory(prefix="lom-watermark-video-") as temporary:
        frames = _extract_frames(
            source, Path(temporary), executable, float(interval), max_frames
        )
        return detect_video_frames(frames, interval=float(interval),
                                   scale_factors=scale_factors)
