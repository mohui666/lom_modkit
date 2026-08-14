# -*- coding: utf-8 -*-
"""开发环境 User Content Registry。

全局仓库只服务编辑器：导入、列出、解析、删除。剧情 JSON 只保存 ``user:`` 引用，
不保存本机绝对路径。导出 .lommod 时由 package_io 把实际引用复制进包；
运行时只读包内 assets/user/，绝不回读本仓库。

图片仍走 ``asset_store``（story 保存 ``assets/<文件名>``，兼容已有 Mod）。
本 Registry 按类型扩展：v1 实现 audio，预留 character 目录与 type 字段。
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
    MAX_AUDIO_BYTES,
    USER_PREFIX,
    check_content_matches_kind,
    collect_stories_content_refs,
    default_repository_root,
    load_content_metadata,
    make_content_ref,
    package_content_dir,
    resolve_content,
    safe_audio_filename,
    scan_repository,
    validate_content_id,
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

    @property
    def display(self) -> str:
        return "%s（%s）" % (self.name, self.ref)


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
    )


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
        meta, main_path = resolve_content(str(repository_root()), "audio", content_id)
    except LomcError as exc:
        raise ContentRegistryError(str(exc)) from exc
    return _record_from_meta(meta, Path(main_path).parent)


def resolve(ref_or_id: str, expected_kind: str | None = None) -> tuple[ContentRecord, Path]:
    raw = ref_or_id[len(USER_PREFIX) :] if ref_or_id.startswith(USER_PREFIX) else ref_or_id
    try:
        meta, main_path = resolve_content(str(repository_root()), "audio", raw)
        if expected_kind:
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
                "无法删除 %s：仍被 %s 引用。请先改掉这些音乐/音效步骤。"
                % (rec.ref, places)
            )
    shutil.rmtree(rec.folder, ignore_errors=False)
    _rebuild_index(list_contents())


def copy_into_mod(mod_dir: Path, content_id: str) -> Path:
    """把仓库中的一条音频复制到 mod 目录的 assets/user/，供 pack 收集。"""
    rec, main_path = resolve(content_id)
    dest_dir = Path(mod_dir) / package_content_dir("audio", content_id)
    dest_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(rec.folder / "content.json", dest_dir / "content.json")
    shutil.copy2(main_path, dest_dir / rec.main_file)
    return dest_dir


def import_package_audio(mod_or_zip_root: Path) -> list[ContentRecord]:
    """从已解包的 .lommod 目录把用户音频登记进仓库（同 ID 已存在则跳过）。"""
    imported: list[ContentRecord] = []
    for meta, folder in scan_repository(str(mod_or_zip_root), "audio"):
        dest = repository_root() / Path(package_content_dir("audio", meta["id"]))
        if dest.exists():
            continue
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(folder, dest)
        imported.append(_record_from_meta(meta, dest))
    if imported:
        _rebuild_index(list_contents())
    return imported
