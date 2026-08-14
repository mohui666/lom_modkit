# -*- coding: utf-8 -*-
"""编辑器托管的用户图片素材仓库（结局卡 / 人物介绍图）。

剧情 JSON 始终只保存 ``assets/<文件名>``，绝不保存本机绝对路径；实际文件
缓存在用户 AppData，导出 .lommod 时再复制进包。这样新手用“选择图片”即可，
也不会把游戏原版素材或其它目录意外整包带走。

带稳定 ``user:`` ID 的新类型内容（自定义音频等）走 ``content_registry``，
不要在这里平行再造一套 AudioStore。图片继续用本模块以保持已有 Mod 兼容。
"""

from __future__ import annotations

import hashlib
import os
import re
from pathlib import Path


IMAGE_EXTENSIONS = {".png", ".jpg", ".jpeg"}
MAX_IMAGE_BYTES = 8 * 1024 * 1024


class AssetStoreError(ValueError):
    pass


def store_root() -> Path:
    appdata = os.environ.get("APPDATA")
    base = Path(appdata) if appdata else Path.home() / "AppData" / "Roaming"
    return base / "lom_modkit" / "assets"


def _safe_name(name: str, suffix: str) -> str:
    stem = Path(name).stem.strip() or "image"
    stem = re.sub(r"[^a-zA-Z0-9_\-\u4e00-\u9fff]+", "_", stem).strip("_")
    return (stem[:80] or "image") + suffix.lower()


def store_image_bytes(name: str, data: bytes) -> tuple[str, Path]:
    suffix = Path(name).suffix.lower()
    if suffix not in IMAGE_EXTENSIONS:
        raise AssetStoreError("人物图片只支持 PNG、JPG 或 JPEG。")
    if not data:
        raise AssetStoreError("图片文件为空。")
    if len(data) > MAX_IMAGE_BYTES:
        raise AssetStoreError("图片超过 8MB，请压缩后重试。")
    digest = hashlib.sha256(data).hexdigest()
    root = store_root()
    root.mkdir(parents=True, exist_ok=True)
    base_name = _safe_name(name, suffix)
    target = root / base_name
    if target.exists() and hashlib.sha256(target.read_bytes()).hexdigest() != digest:
        target = root / f"{Path(base_name).stem}-{digest[:8]}{suffix}"
    if not target.exists():
        target.write_bytes(data)
    return f"assets/{target.name}", target


def import_image_file(path: Path) -> tuple[str, Path]:
    source = Path(path)
    try:
        data = source.read_bytes()
    except OSError as exc:
        raise AssetStoreError(f"无法读取图片：{exc}") from exc
    return store_image_bytes(source.name, data)


def resolve_image_asset(relative: str) -> Path | None:
    value = str(relative or "").replace("\\", "/")
    if not value.startswith("assets/") or "/" in value[len("assets/") :]:
        return None
    target = store_root() / value[len("assets/") :]
    return target if target.is_file() else None
