# -*- coding: utf-8 -*-
"""Versioned, lossless JSON migrations with recoverable atomic file updates."""

from __future__ import annotations

import copy
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import sys
import tempfile
from typing import Callable

from schema_versions import CONTENT_SCHEMA, PACKAGE_FORMAT, STORY_SCHEMA


MAX_MIGRATION_JSON_BYTES = 4 * 1024 * 1024
DocumentValidator = Callable[[dict], None]


class MigrationError(ValueError):
    pass


def _dice_metadata() -> dict:
    root = (
        Path(getattr(sys, "_MEIPASS"))
        if getattr(sys, "frozen", False) and getattr(sys, "_MEIPASS", None)
        else Path(__file__).resolve().parent.parent
    )
    try:
        data = json.loads((root / "data" / "editor_data.json").read_text(encoding="utf-8"))
        meta = data.get("dice_meta")
        return meta if isinstance(meta, dict) else {}
    except (OSError, ValueError, TypeError):
        return {}


def _inline_legacy_dice(node: dict, metadata: dict) -> dict[int, int]:
    check = node.get("check")
    meta = metadata.get(check)
    if not isinstance(check, str) or not isinstance(meta, dict):
        raise MigrationError(
            "节点 %r 的旧骰子检查点 %r 缺少元数据，无法安全展开" % (
                node.get("id"), check,
            )
        )
    source_bands = meta.get("bands")
    if not isinstance(source_bands, list) or not 2 <= len(source_bands) <= 4:
        raise MigrationError("节点 %r 的旧骰子结果带无效" % node.get("id"))
    option = (node.get("options") or [{}])[0]
    if not isinstance(option, dict):
        raise MigrationError("节点 %r 的旧骰子选项无效" % node.get("id"))
    custom_texts = option.get("band_texts")
    parsed = []
    for index, band in enumerate(source_bands):
        if not isinstance(band, dict):
            raise MigrationError("节点 %r 的旧骰子结果带无效" % node.get("id"))
        match = re.search(r"(<=|>=|<|>|=)\s*(-?\d+)", str(band.get("cond", "")))
        if match is None:
            raise MigrationError(
                "节点 %r 的旧骰子条件 %r 无法安全展开" % (
                    node.get("id"), band.get("cond"),
                )
            )
        op, raw = match.group(1), int(match.group(2))
        low = -10**9 if op in ("<", "<=") else raw + (1 if op == ">" else 0)
        high = 10**9 if op in (">", ">=") else raw - (1 if op == "<" else 0)
        text = (
            custom_texts[index]
            if isinstance(custom_texts, list) and index < len(custom_texts)
            else band.get("text")
        )
        parsed.append((low, high, str(text or "结果"), index))
    parsed.sort(key=lambda item: (item[0], item[1]))
    direct_bands = []
    old_to_new = {}
    for index, (_low, high, text, old_index) in enumerate(parsed):
        old_to_new[old_index] = index
        if index == 0:
            target = option.get("goto_失败")
        elif index == len(parsed) - 1 and len(parsed) >= 3:
            target = option.get("goto_大成功")
        else:
            target = option.get("goto_成功")
        row = {"text": text, "goto": target}
        if index < len(parsed) - 1:
            if high >= 10**9:
                raise MigrationError("节点 %r 的旧骰子分段顺序无法安全展开" % node.get("id"))
            row["upper"] = high
        direct_bands.append(row)
    node.pop("check", None)
    node.pop("options", None)
    node.update({
        "max": int(meta.get("max", 99)),
        "header": "命运检定",
        "bonus": 0,
        "bands": direct_bands,
    })
    return old_to_new


def _migrate_legacy_dice_localization(result: dict, index_maps: dict[str, dict[int, int]]) -> None:
    localization = result.get("localization")
    if not isinstance(localization, dict):
        return
    translations = localization.get("translations")
    if not isinstance(translations, dict):
        return
    for catalog in translations.values():
        if not isinstance(catalog, dict):
            continue
        replacements = {}
        removals = []
        for path, value in list(catalog.items()):
            match = re.fullmatch(r"([A-Za-z0-9_-]+)\.options\.0\.band_texts\.(\d+)", str(path))
            if match is None or match.group(1) not in index_maps:
                continue
            old_index = int(match.group(2))
            if old_index not in index_maps[match.group(1)]:
                raise MigrationError("旧骰子翻译路径 %r 超出结果带范围" % path)
            new_index = index_maps[match.group(1)][old_index]
            new_path = "%s.bands.%d.text" % (match.group(1), new_index)
            if new_path in catalog or new_path in replacements:
                raise MigrationError("旧骰子翻译迁移后路径冲突：%s" % new_path)
            replacements[new_path] = value
            removals.append(path)
        for path in removals:
            catalog.pop(path, None)
        catalog.update(replacements)


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
    """Add the Story declaration and inline obsolete Combat/Battle presets."""
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

    if "battle_presets" in result:
        presets = result.get("battle_presets")
        if not isinstance(presets, dict):
            raise MigrationError("旧字段 battle_presets 必须是对象，无法自动展开")
        nodes = result.get("nodes")
        if not isinstance(nodes, list):
            raise MigrationError("story 缺少 nodes 数组，无法展开旧战斗预设")
        for node in nodes:
            if not isinstance(node, dict) or "preset" not in node:
                continue
            preset_id = node.get("preset")
            preset = presets.get(preset_id)
            if not isinstance(preset_id, str) or not isinstance(preset, dict):
                raise MigrationError("节点 %r 引用了不存在的旧战斗预设 %r" % (
                    node.get("id"), preset_id,
                ))
            node_kind = node.get("type")
            if node_kind not in ("combat", "battle") or preset.get("kind") != node_kind:
                raise MigrationError(
                    "节点 %r 与旧战斗预设 %r 的类型不一致" % (node.get("id"), preset_id)
                )
            expanded = {
                key: copy.deepcopy(value)
                for key, value in preset.items()
                if key not in ("name", "kind")
            }
            expanded.update({
                key: copy.deepcopy(value)
                for key, value in node.items()
                if key != "preset"
            })
            node.clear()
            node.update(expanded)
        result.pop("battle_presets", None)
        steps.append("inline battle_presets into combat/battle nodes")

    legacy_dice = [
        node for node in result.get("nodes", [])
        if isinstance(node, dict) and node.get("type") == "dice" and "check" in node
    ]
    if legacy_dice:
        metadata = _dice_metadata()
        index_maps = {}
        for node in legacy_dice:
            node_id = str(node.get("id") or "")
            index_maps[node_id] = _inline_legacy_dice(node, metadata)
        _migrate_legacy_dice_localization(result, index_maps)
        steps.append("inline original dice checkpoints into direct dice parameters")
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
