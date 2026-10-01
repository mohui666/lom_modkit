"""Read the owner's extracted game previews without embedding them in releases."""
from __future__ import annotations

import json
from pathlib import Path, PurePosixPath


def read_preview_library(directory: Path) -> tuple[dict, Path]:
    root = Path(directory).expanduser().resolve()
    if not (root / "preview_map.json").is_file() and (root / "data" / "preview_map.json").is_file():
        root = root / "data"
    source = root / "preview_map.json"
    if source.stat().st_size > 4 * 1024 * 1024:
        raise ValueError("预览映射超过 4MB")
    mapping = json.loads(source.read_text(encoding="utf-8"))
    if not isinstance(mapping, dict) or not isinstance(mapping.get("characters"), dict):
        raise ValueError("预览目录需要 preview_map.json 和 assets/portraits/")
    views = mapping.get("views") or {}
    if not isinstance(views, dict):
        raise ValueError("场景预览映射格式错误")
    paths = list(views.values())
    for character in mapping["characters"].values():
        if not isinstance(character, dict) or not isinstance(character.get("portraits"), dict):
            raise ValueError("人物预览映射格式错误")
        paths.extend(character["portraits"].values())
    for value in paths:
        if not isinstance(value, str):
            raise ValueError("预览图片路径必须为字符串")
        path = PurePosixPath(value.replace("\\", "/"))
        if path.is_absolute() or ".." in path.parts or ":" in value:
            raise ValueError("预览图片路径必须留在素材目录内")
        if not (root / str(path)).resolve().is_relative_to(root):
            raise ValueError("预览图片链接越出了素材目录")
    return mapping, root
