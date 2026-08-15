# -*- coding: utf-8 -*-
"""开发环境 User Content Registry。

全局仓库只服务编辑器：导入、列出、解析、删除。剧情 JSON 只保存 ``user:`` 引用，
不保存本机绝对路径。导出 .lommod 时由 package_io 把实际引用复制进包；
运行时只读包内 assets/user/，绝不回读本仓库。

图片仍走 ``asset_store``（story 保存 ``assets/<文件名>``，兼容已有 Mod）。
本 Registry 按类型扩展：audio 与 character（自定义立绘角色）。
"""

from __future__ import annotations

import json
import os
import re
import shutil
from dataclasses import dataclass
from pathlib import Path
import sys

_COMPILER = Path(__file__).resolve().parent.parent / "compiler"
if _COMPILER.is_dir() and str(_COMPILER) not in sys.path:
    sys.path.insert(0, str(_COMPILER))

from lomc.content import (
    AUDIO_EXTENSIONS,
    AUDIO_KINDS,
    CONTENT_SCHEMA,
    IMAGE_EXTENSIONS,
    MAX_AUDIO_BYTES,
    MAX_IMAGE_BYTES,
    USER_PREFIX,
    check_character_portrait,
    check_content_matches_kind,
    collect_stories_content_refs,
    default_repository_root,
    listed_content_files,
    load_content_metadata,
    make_content_ref,
    package_content_dir,
    resolve_content,
    resolve_content_dir,
    safe_audio_filename,
    safe_image_filename,
    scan_repository,
    validate_content_id,
    validate_portrait_id,
    write_content_metadata,
)
from lomc.errors import LomcError


class ContentRegistryError(ValueError):
    """面向编辑器用户的内容库错误。"""


@dataclass(frozen=True)
class ContentRecord:
    content_id: str
    type: str
    name: str
    audio_kind: str | None
    main_file: str
    folder: Path
    ref: str
    portraits: dict[str, str] | None = None

    @property
    def display(self) -> str:
        return "%s（%s）" % (self.name, self.ref)

    def portrait_ids(self) -> list[str]:
        portraits = self.portraits or {}
        ids = list(portraits.keys())
        if "normal" in ids:
            ids.remove("normal")
            return ["normal"] + sorted(ids)
        return sorted(ids) or ["normal"]


def repository_root() -> Path:
    return Path(default_repository_root())


def _registry_path() -> Path:
    return repository_root() / "registry.json"


def _sanitize_namespace(raw: str) -> str:
    text = re.sub(r"[^a-z0-9_]+", "", (raw or "").strip().lower())
    if not text:
        text = "custom"
    if not text[0].isalpha():
        text = "u" + text
    return text[:32]


def default_namespace() -> str:
    data = _read_index()
    stored = data.get("default_namespace")
    if isinstance(stored, str) and stored:
        try:
            validate_content_id(stored + ".x")
            return stored
        except LomcError:
            pass
    return _sanitize_namespace(os.environ.get("USERNAME") or os.environ.get("USER") or "custom")


def set_default_namespace(namespace: str) -> str:
    ns = _sanitize_namespace(namespace)
    try:
        validate_content_id(ns + ".placeholder")
    except LomcError as exc:
        raise ContentRegistryError(str(exc)) from exc
    data = _read_index()
    data["default_namespace"] = ns
    _write_index(data)
    return ns


def suggest_content_id(filename: str, namespace: str | None = None) -> str:
    ns = _sanitize_namespace(namespace or default_namespace())
    stem = Path(filename).stem.strip().lower()
    stem = re.sub(r"[^a-z0-9_]+", "_", stem).strip("_") or "audio"
    if stem[0].isdigit():
        stem = "a" + stem
    return "%s.%s" % (ns, stem[:48])


def _read_index() -> dict:
    path = _registry_path()
    if not path.is_file():
        return {"schema": CONTENT_SCHEMA, "default_namespace": "", "contents": []}
    try:
        data = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        return {"schema": CONTENT_SCHEMA, "default_namespace": "", "contents": []}
    if not isinstance(data, dict):
        return {"schema": CONTENT_SCHEMA, "default_namespace": "", "contents": []}
    data.setdefault("schema", CONTENT_SCHEMA)
    data.setdefault("contents", [])
    return data


def _write_index(data: dict) -> None:
    root = repository_root()
    root.mkdir(parents=True, exist_ok=True)
    payload = {
        "schema": CONTENT_SCHEMA,
        "default_namespace": data.get("default_namespace") or default_namespace(),
        "contents": data.get("contents") or [],
    }
    _registry_path().write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )


def _record_from_meta(meta: dict, folder: Path) -> ContentRecord:
    return ContentRecord(
        content_id=meta["id"],
        type=meta["type"],
        name=meta["name"],
        audio_kind=meta.get("audio_kind"),
        main_file=meta["files"]["main"],
        folder=folder,
        ref=USER_PREFIX + meta["id"],
        portraits=dict(meta["portraits"]) if meta.get("portraits") else None,
    )


def _infer_type(content_id: str) -> str:
    root = str(repository_root())
    for ctype in ("audio", "character"):
        if resolve_content_dir(root, ctype, content_id) is not None:
            return ctype
    return "audio"


def _rebuild_index(records: list[ContentRecord]) -> None:
    data = _read_index()
    data["contents"] = [
        {"id": rec.content_id, "type": rec.type} for rec in records
    ]
    _write_index(data)


def list_contents(
    content_type: str | None = None, audio_kind: str | None = None
) -> list[ContentRecord]:
    """列出仓库中的内容。优先扫盘，registry.json 只作索引缓存。"""
    records = []
    for meta, folder in scan_repository(str(repository_root()), content_type):
        rec = _record_from_meta(meta, Path(folder))
        if audio_kind and rec.audio_kind != audio_kind:
            continue
        records.append(rec)
    return records


def get(content_id: str) -> ContentRecord:
    try:
        validate_content_id(content_id)
        ctype = _infer_type(content_id)
        meta, main_path = resolve_content(str(repository_root()), ctype, content_id)
    except LomcError as exc:
        raise ContentRegistryError(str(exc)) from exc
    return _record_from_meta(meta, Path(main_path).parent)


def resolve(
    ref_or_id: str,
    expected_kind: str | None = None,
    expected_type: str | None = None,
    portrait: str | None = None,
) -> tuple[ContentRecord, Path]:
    raw = ref_or_id[len(USER_PREFIX) :] if ref_or_id.startswith(USER_PREFIX) else ref_or_id
    try:
        ctype = expected_type or _infer_type(raw)
        meta, main_path = resolve_content(str(repository_root()), ctype, raw)
        if ctype == "character":
            check_character_portrait(meta, portrait, USER_PREFIX + raw)
        elif expected_kind:
            check_content_matches_kind(meta, expected_kind, USER_PREFIX + raw)
    except LomcError as exc:
        raise ContentRegistryError(str(exc)) from exc
    return _record_from_meta(meta, Path(main_path).parent), Path(main_path)


def register_audio(
    source: Path,
    content_id: str,
    name: str,
    audio_kind: str,
) -> ContentRecord:
    """导入本地音频到仓库，返回稳定记录。同一 ID 已存在则报错。"""
    if audio_kind not in AUDIO_KINDS:
        raise ContentRegistryError(
            "音频用途必须是 music（音乐）、sound（音效）或 env（环境音）。"
        )
    try:
        validate_content_id(content_id)
    except LomcError as exc:
        raise ContentRegistryError(str(exc)) from exc
    display = (name or "").strip() or Path(source).stem
    src = Path(source)
    try:
        data = src.read_bytes()
    except OSError as exc:
        raise ContentRegistryError("无法读取音频文件：%s" % exc) from exc
    if not data:
        raise ContentRegistryError("音频文件是空的。")
    if len(data) > MAX_AUDIO_BYTES:
        raise ContentRegistryError("音频超过 20MB，请压缩后再导入。")
    try:
        filename = safe_audio_filename(src.name)
    except LomcError as exc:
        raise ContentRegistryError(str(exc)) from exc
    suffix = Path(filename).suffix.lower()
    if suffix not in AUDIO_EXTENSIONS:
        raise ContentRegistryError("目前只支持 OGG 和 WAV 音频。")

    folder = repository_root() / Path(package_content_dir("audio", content_id))
    if folder.exists():
        raise ContentRegistryError(
            "内容 ID %s 已存在。请换一个 ID，或先删除旧内容。"
            % (USER_PREFIX + content_id)
        )
    folder.mkdir(parents=True, exist_ok=False)
    try:
        (folder / filename).write_bytes(data)
        write_content_metadata(
            str(folder / "content.json"),
            {
                "schema": CONTENT_SCHEMA,
                "id": content_id,
                "type": "audio",
                "name": display,
                "audio_kind": audio_kind,
                "files": {"main": filename},
            },
        )
    except Exception:
        shutil.rmtree(folder, ignore_errors=True)
        raise
    _rebuild_index(list_contents())
    return get(content_id)


def register_character(
    portraits: dict[str, Path],
    content_id: str,
    name: str,
) -> ContentRecord:
    """导入自定义角色。portraits 至少包含 normal，值为本机图片路径。"""
    try:
        validate_content_id(content_id)
    except LomcError as exc:
        raise ContentRegistryError(str(exc)) from exc
    if not isinstance(portraits, dict) or not portraits:
        raise ContentRegistryError("至少导入一张默认立绘（normal）。")
    display = (name or "").strip() or content_id
    normalized: dict[str, tuple[str, bytes]] = {}
    for raw_key, source in portraits.items():
        try:
            key = validate_portrait_id(str(raw_key))
        except LomcError as exc:
            raise ContentRegistryError(str(exc)) from exc
        src = Path(source)
        try:
            data = src.read_bytes()
        except OSError as exc:
            raise ContentRegistryError("无法读取立绘 %s：%s" % (key, exc)) from exc
        if not data:
            raise ContentRegistryError("立绘 %s 是空文件。" % key)
        if len(data) > MAX_IMAGE_BYTES:
            raise ContentRegistryError("立绘 %s 超过 8MB，请压缩后再导入。" % key)
        try:
            filename = safe_image_filename(src.name)
        except LomcError as exc:
            raise ContentRegistryError(str(exc)) from exc
        if Path(filename).suffix.lower() not in IMAGE_EXTENSIONS:
            raise ContentRegistryError("立绘只支持 PNG / JPG。")
        if filename in {item[0] for item in normalized.values()} and key != "normal":
            stem, ext = Path(filename).stem, Path(filename).suffix
            filename = "%s_%s%s" % (stem[:48], key, ext)
        normalized[key] = (filename, data)
    if "normal" not in normalized:
        raise ContentRegistryError("必须提供 normal 默认立绘。")

    folder = repository_root() / Path(package_content_dir("character", content_id))
    if folder.exists():
        raise ContentRegistryError(
            "内容 ID %s 已存在。请换一个 ID，或先删除旧内容。"
            % (USER_PREFIX + content_id)
        )
    folder.mkdir(parents=True, exist_ok=False)
    try:
        for key, (filename, data) in normalized.items():
            (folder / filename).write_bytes(data)
        write_content_metadata(
            str(folder / "content.json"),
            {
                "schema": CONTENT_SCHEMA,
                "id": content_id,
                "type": "character",
                "name": display,
                "files": {"main": normalized["normal"][0]},
                "portraits": {key: filename for key, (filename, _data) in normalized.items()},
            },
        )
    except Exception:
        shutil.rmtree(folder, ignore_errors=True)
        raise
    _rebuild_index(list_contents())
    return get(content_id)


def update_character(
    content_id: str,
    name: str | None = None,
    portraits: dict[str, Path] | None = None,
    remove_portraits: list[str] | None = None,
) -> ContentRecord:
    """改已有角色的显示名 / 增改表情 / 删表情。编号不变。"""
    rec = get(content_id)
    if rec.type != "character":
        raise ContentRegistryError("%s 不是自定义角色。" % rec.ref)
    display = (name or rec.name).strip() or rec.name
    current = dict(rec.portraits or {"normal": rec.main_file})
    for key in remove_portraits or []:
        if key == "normal":
            raise ContentRegistryError("不能删除默认立绘 normal，只能换图。")
        current.pop(key, None)
    written: dict[str, str] = dict(current)
    for raw_key, source in (portraits or {}).items():
        try:
            key = validate_portrait_id(str(raw_key))
        except LomcError as exc:
            raise ContentRegistryError(str(exc)) from exc
        src = Path(source)
        try:
            data = src.read_bytes()
        except OSError as exc:
            raise ContentRegistryError("无法读取立绘 %s：%s" % (key, exc)) from exc
        if not data:
            raise ContentRegistryError("立绘 %s 是空文件。" % key)
        if len(data) > MAX_IMAGE_BYTES:
            raise ContentRegistryError("立绘 %s 超过 8MB，请压缩后再导入。" % key)
        try:
            filename = safe_image_filename(src.name)
        except LomcError as exc:
            raise ContentRegistryError(str(exc)) from exc
        if filename in set(written.values()) and written.get(key) != filename:
            stem, ext = Path(filename).stem, Path(filename).suffix
            filename = "%s_%s%s" % (stem[:48], key, ext)
        (rec.folder / filename).write_bytes(data)
        written[key] = filename
    if "normal" not in written:
        raise ContentRegistryError("必须保留 normal 默认立绘。")
    keep = set(written.values())
    write_content_metadata(
        str(rec.folder / "content.json"),
        {
            "schema": CONTENT_SCHEMA,
            "id": content_id,
            "type": "character",
            "name": display,
            "files": {"main": written["normal"]},
            "portraits": written,
        },
    )
    for leftover in rec.folder.iterdir():
        if leftover.name in ("content.json",) or leftover.name in keep:
            continue
        if leftover.is_file():
            leftover.unlink()
    _rebuild_index(list_contents())
    return get(content_id)


def remove(content_id: str, stories: dict | None = None) -> None:
    """删除未使用的用户内容。仍被当前项目引用时拒绝。"""
    rec = get(content_id)
    if stories:
        refs = [
            item
            for item in collect_stories_content_refs(stories)
            if item["ref"].content_id == content_id
        ]
        if refs:
            places = "、".join(
                "章节 %s 步骤 %s" % (item.get("story_id", "?"), item["node_id"])
                for item in refs[:6]
            )
            raise ContentRegistryError(
                "无法删除 %s：仍被 %s 引用。请先改掉这些步骤。"
                % (rec.ref, places)
            )
    shutil.rmtree(rec.folder, ignore_errors=False)
    _rebuild_index(list_contents())


def copy_into_mod(mod_dir: Path, content_id: str) -> Path:
    """把仓库中的一条内容复制到 mod 目录的 assets/user/，供 pack 收集。"""
    rec, _main_path = resolve(content_id)
    dest_dir = Path(mod_dir) / package_content_dir(rec.type, content_id)
    dest_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(rec.folder / "content.json", dest_dir / "content.json")
    for fname in listed_content_files(
        {
            "files": {"main": rec.main_file},
            "portraits": rec.portraits or {},
        }
    ):
        src = rec.folder / fname
        if src.is_file():
            shutil.copy2(src, dest_dir / fname)
    return dest_dir


def import_package_audio(mod_or_zip_root: Path) -> list[ContentRecord]:
    """从已解包的 .lommod 目录把用户内容登记进仓库（同 ID 已存在则跳过）。"""
    imported: list[ContentRecord] = []
    for meta, folder in scan_repository(str(mod_or_zip_root)):
        dest = repository_root() / Path(package_content_dir(meta["type"], meta["id"]))
        if dest.exists():
            continue
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(folder, dest)
        imported.append(_record_from_meta(meta, dest))
    if imported:
        _rebuild_index(list_contents())
    return imported
