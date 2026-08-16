# -*- coding: utf-8 -*-
"""Versioned, lossless JSON migrations with recoverable atomic file updates."""

from __future__ import annotations

import copy
from dataclasses import dataclass
import json
import os
from pathlib import Path
import tempfile
from typing import Callable

from schema_versions import CONTENT_SCHEMA, PACKAGE_FORMAT, STORY_SCHEMA


MAX_MIGRATION_JSON_BYTES = 4 * 1024 * 1024
DocumentValidator = Callable[[dict], None]


class MigrationError(ValueError):
    pass


@dataclass(frozen=True)
class MigrationResult:
    kind: str
    document: dict
    changed: bool
    from_version: int
    to_version: int
    steps: tuple[str, ...]


def _integer_version(value: object, field: str, current: int) -> int:
    if value != current or isinstance(value, bool):
        raise MigrationError(
            "无法迁移 %s=%r：当前工具仅支持版本 %d" % (field, value, current)
        )
    return current


def migrate_manifest(document: dict) -> MigrationResult:
    """Migrate a v1 manifest to explicit package/story/content declarations."""
    if not isinstance(document, dict):
        raise MigrationError("manifest 顶层必须是 JSON 对象")
    result = copy.deepcopy(document)
    has_explicit = "package_format" in result
    has_legacy = "format" in result
    if not has_explicit and not has_legacy:
        raise MigrationError("manifest 缺少 package_format/format，无法判断来源版本")
    package_version = _integer_version(
        result.get("package_format") if has_explicit else result.get("format"),
        "package_format",
        PACKAGE_FORMAT,
    )
    if has_explicit and has_legacy:
        legacy = _integer_version(result.get("format"), "format", PACKAGE_FORMAT)
        if legacy != package_version:
            raise MigrationError("package_format 与旧字段 format 不一致")

    steps: list[str] = []
    declarations = (
        ("package_format", PACKAGE_FORMAT),
        ("story_schema", STORY_SCHEMA),
        ("content_schema", CONTENT_SCHEMA),
    )
    for field, current in declarations:
        if field in result:
            _integer_version(result.get(field), field, current)
        else:
            result[field] = current
            steps.append("add " + field)
    if "format" not in result:
        result["format"] = PACKAGE_FORMAT
        steps.append("add legacy format")
    return MigrationResult(
        kind="manifest",
        document=result,
        changed=bool(steps),
        from_version=package_version,
        to_version=PACKAGE_FORMAT,
        steps=tuple(steps),
    )


def migrate_story(document: dict) -> MigrationResult:
    """Add the explicit v1 Story declaration without rewriting unknown fields."""
    if not isinstance(document, dict):
        raise MigrationError("story 顶层必须是 JSON 对象")
    result = copy.deepcopy(document)
    steps: list[str] = []
    if "story_schema" in result:
        version = _integer_version(
            result.get("story_schema"), "story_schema", STORY_SCHEMA
        )
    else:
        version = STORY_SCHEMA  # pre-declaration Story files are legacy v1
        result["story_schema"] = STORY_SCHEMA
        steps.append("add story_schema")
    return MigrationResult(
        kind="story",
        document=result,
        changed=bool(steps),
        from_version=version,
        to_version=STORY_SCHEMA,
        steps=tuple(steps),
    )


def migrate_content(document: dict) -> MigrationResult:
    """Migrate legacy ``schema: 1`` content metadata to ``content_schema``."""
    if not isinstance(document, dict):
        raise MigrationError("content 顶层必须是 JSON 对象")
    result = copy.deepcopy(document)
    has_explicit = "content_schema" in result
    has_legacy = "schema" in result
    if not has_explicit and not has_legacy:
        raise MigrationError("content.json 缺少 content_schema/schema")
    version = _integer_version(
        result.get("content_schema") if has_explicit else result.get("schema"),
        "content_schema",
        CONTENT_SCHEMA,
    )
    if has_explicit and has_legacy:
        legacy = _integer_version(result.get("schema"), "schema", CONTENT_SCHEMA)
        if legacy != version:
            raise MigrationError("content_schema 与旧字段 schema 不一致")
    steps: list[str] = []
    if not has_explicit:
        result["content_schema"] = CONTENT_SCHEMA
        steps.append("add content_schema")
    if not has_legacy:
        result["schema"] = CONTENT_SCHEMA
        steps.append("add legacy schema")
    return MigrationResult(
        kind="content",
        document=result,
        changed=bool(steps),
        from_version=version,
        to_version=CONTENT_SCHEMA,
        steps=tuple(steps),
    )


_MIGRATORS = {
    "manifest": migrate_manifest,
    "story": migrate_story,
    "content": migrate_content,
}


def migrate_document(document: dict, kind: str) -> MigrationResult:
    try:
        migrator = _MIGRATORS[kind]
    except KeyError as exc:
        raise MigrationError("未知迁移类型：%s" % kind) from exc
    return migrator(document)


def _read_json_bytes(path: Path) -> tuple[bytes, dict]:
    try:
        raw = path.read_bytes()
    except OSError as exc:
        raise MigrationError("无法读取 %s：%s" % (path, exc)) from exc
    if len(raw) > MAX_MIGRATION_JSON_BYTES:
        raise MigrationError("%s 超过 4 MiB，拒绝自动迁移" % path.name)
    try:
        document = json.loads(raw.decode("utf-8-sig"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise MigrationError("%s 不是合法 UTF-8 JSON：%s" % (path.name, exc)) from exc
    if not isinstance(document, dict):
        raise MigrationError("%s 顶层必须是 JSON 对象" % path.name)
    return raw, document


def _backup_path(source: Path, original: bytes, label: str) -> Path:
    base = source.with_name(source.name + ".pre-migration-%s.bak" % label)
    for index in range(1000):
        candidate = base if index == 0 else base.with_name(base.name + ".%d" % index)
        try:
            with candidate.open("xb") as stream:
                stream.write(original)
                stream.flush()
                os.fsync(stream.fileno())
            return candidate
        except FileExistsError:
            try:
                if candidate.read_bytes() == original:
                    return candidate
            except OSError:
                pass
            continue
        except OSError as exc:
            raise MigrationError("无法创建迁移备份 %s：%s" % (candidate, exc)) from exc
    raise MigrationError("迁移备份文件过多，请先整理 %s" % source.parent)


def _atomic_replace_bytes(target: Path, payload: bytes) -> None:
    temp_path: Path | None = None
    try:
        fd, temp_name = tempfile.mkstemp(
            prefix=target.name + ".", suffix=".migration.tmp", dir=str(target.parent)
        )
        temp_path = Path(temp_name)
        with os.fdopen(fd, "wb") as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp_path, target)
        temp_path = None
    except OSError as exc:
        raise MigrationError("无法原子更新 %s：%s" % (target, exc)) from exc
    finally:
        if temp_path is not None:
            try:
                temp_path.unlink()
            except OSError:
                pass


def migrate_json_file(
    path: Path,
    kind: str,
    *,
    validator: DocumentValidator | None = None,
) -> tuple[MigrationResult, Path | None]:
    """Migrate one JSON file, backing up exact original bytes before replacement."""
    source = Path(path)
    original, document = _read_json_bytes(source)
    result = migrate_document(document, kind)
    if validator is not None:
        validator(copy.deepcopy(result.document))
    if not result.changed:
        return result, None
    payload = (
        json.dumps(result.document, ensure_ascii=False, indent=2) + "\n"
    ).encode("utf-8")
    backup = _backup_path(source, original, "v%d" % result.from_version)
    _atomic_replace_bytes(source, payload)
    return result, backup


def restore_migration_backup(source: Path, backup: Path) -> Path:
    """Restore an explicit migration backup and retain the replaced file as recovery."""
    target = Path(source)
    backup_path = Path(backup)
    original, document = _read_json_bytes(backup_path)
    if not isinstance(document, dict):  # defensive; _read_json_bytes already checks
        raise MigrationError("备份不是 JSON 对象")
    current = target.read_bytes() if target.exists() else b""
    recovery = _backup_path(target, current, "before-recovery")
    _atomic_replace_bytes(target, original)
    return recovery
