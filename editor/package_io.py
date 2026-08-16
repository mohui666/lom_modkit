# -*- coding: utf-8 -*-
"""`.lommod` 包的导入/导出（包结构契约见 docs/chs/mod_format.md §1/§2）。

导出时必须重新编译：story/*.json → lua/*.lua，二者同名。
编译优先调 lomc.pack_mod（编译器官方打包入口），否则编辑器自行
逐个调 lomc.compile_story 并写 zip。
"""

from __future__ import annotations

import json
import os
import shutil
import zipfile
from pathlib import Path

from asset_store import AssetStoreError, resolve_image_asset, store_image_bytes
import content_registry
from lua_preview import compile_story, get_lomc
from migration import MigrationError, migrate_manifest, migrate_story
from schema_versions import (
    CONTENT_SCHEMA,
    PACKAGE_FORMAT,
    STORY_SCHEMA,
    assert_supported_version,
    manifest_versions,
)
from lomc.package_validation import (
    ArchiveValidationError,
    MAX_ARCHIVE_ENTRIES,
    MAX_ARCHIVE_UNCOMPRESSED,
    MAX_ENTRY_BYTES,
    MAX_PACKAGE_FILE_BYTES,
    MAX_TEXT_BYTES,
    canonical_archive_name,
    validate_archive_entries,
)
from lomc.story_lua_integrity import (
    STORY_LUA_INTEGRITY_ENTRY,
    build_story_lua_integrity,
    verify_story_lua_integrity,
)
from lomc.deterministic_zip import stable_json_bytes
from lomc.content import COMBAT_ANIMATION_FIELDS


class PackError(Exception):
    """导入/导出失败，message 面向用户可读。"""


MAX_JSON_BYTES = MAX_TEXT_BYTES
MAX_IMAGE_BYTES = 8 * 1024 * 1024
MAX_USER_FILE_BYTES = 32 * 1024 * 1024


def _safe_archive_name(name: str) -> str:
    """Return a canonical ZIP path, rejecting paths unsafe on POSIX or Windows."""
    try:
        return canonical_archive_name(name)
    except ArchiveValidationError as exc:
        raise PackError(str(exc)) from exc


def _validated_entries(zf: zipfile.ZipFile) -> dict[str, zipfile.ZipInfo]:
    try:
        return validate_archive_entries(zf.infolist())
    except ArchiveValidationError as exc:
        raise PackError(str(exc)) from exc


def _read_entry(
    zf: zipfile.ZipFile, info: zipfile.ZipInfo, limit: int, description: str
) -> bytes:
    if info.file_size > limit:
        raise PackError(f"包内 {description} 过大（最多 {limit // (1024 * 1024)} MiB）")
    try:
        with zf.open(info) as stream:
            data = stream.read(limit + 1)
    except (OSError, RuntimeError, zipfile.BadZipFile) as exc:
        raise PackError(f"无法读取包内 {description}：{exc}") from exc
    if len(data) > limit:
        raise PackError(f"包内 {description} 过大（最多 {limit // (1024 * 1024)} MiB）")
    return data


def _referenced_images(stories: dict[str, dict]) -> set[str]:
    refs: set[str] = set()
    for story in stories.values():
        for node in story.get("nodes") or []:
            image = node.get("image")
            if not image:
                continue
            if (
                node.get("type") == "intro"
                and node.get("intro_source", "official") == "custom"
            ) or (node.get("type") == "goto_scene" and node.get("scene") == "End"):
                refs.add(str(image).replace("\\", "/"))
    return refs


def _read_json_from_zip(
    zf: zipfile.ZipFile, entries: dict[str, zipfile.ZipInfo], name: str
) -> dict:
    try:
        info = entries[name]
    except KeyError:
        raise PackError(f"包内缺少文件：{name}")
    try:
        result = json.loads(_read_entry(zf, info, MAX_JSON_BYTES, name).decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PackError(f"包内 {name} 不是合法 JSON：{exc}")
    if not isinstance(result, dict):
        raise PackError(f"包内 {name} 的 JSON 顶层必须是对象")
    return result


def _verify_story_lua_pairs(
    zf: zipfile.ZipFile,
    entries: dict[str, zipfile.ZipInfo],
    manifest: dict,
    stories: dict[str, dict],
    *,
    require_integrity: bool = False,
) -> None:
    """Verify metadata hashes and independently recompile every Lua variant."""
    lua_names = sorted(
        name for name, info in entries.items()
        if name.startswith("lua/") and name.endswith(".lua") and not info.is_dir()
    )
    expected_default = {f"lua/{story_id}.lua" for story_id in stories}
    if not expected_default.issubset(lua_names):
        missing = sorted(expected_default - set(lua_names))
        raise PackError("包内 Story 缺少对应 Lua：" + "、".join(missing))
    relevant_names = set(lua_names)
    relevant_names.update(f"story/{story_id}.json" for story_id in stories)
    entry_bytes = {
        name: _read_entry(zf, entries[name], MAX_TEXT_BYTES, name)
        for name in relevant_names
    }
    integrity_info = entries.get(STORY_LUA_INTEGRITY_ENTRY)
    if integrity_info is None and require_integrity:
        raise PackError(
            "package_format=%d 的包必须包含 story-lua.sha256" % PACKAGE_FORMAT
        )
    if integrity_info is not None:
        record = _read_entry(
            zf, integrity_info, MAX_TEXT_BYTES, STORY_LUA_INTEGRITY_ENTRY
        )
        try:
            verify_story_lua_integrity(record, entry_bytes)
        except Exception as exc:
            raise PackError(str(exc)) from exc

    lomc, lomc_error = get_lomc()
    if lomc is None:
        raise PackError("编译器不可用，无法验证 Story/Lua 一致性：%s" % lomc_error)
    try:
        lomc.validate_manifest(manifest, source="包内 manifest.json")
    except Exception as exc:
        raise PackError("包内 manifest.json 不符合 v3 契约：%s" % exc) from exc
    for story_id, story in stories.items():
        story_path = f"story/{story_id}.json"
        variants = []
        for lua_path in lua_names:
            parts = lua_path.split("/")
            if len(parts) == 2:
                lua_story_id = parts[1][:-4]
                locale = None
            elif len(parts) == 3:
                locale = parts[1]
                locale = {
                    "zh_CN": "chs", "zh-CN": "chs", "zh_Hans": "chs", "zh-Hans": "chs",
                    "zh_TW": "cht", "zh-TW": "cht", "zh_Hant": "cht", "zh-Hant": "cht",
                }.get(locale, locale)
                if locale not in ("chs", "cht", "ja", "ko"):
                    raise PackError("包内包含不支持的 locale Lua 路径：%s" % lua_path)
                lua_story_id = parts[2][:-4]
            else:
                raise PackError("包内 Lua 路径层级无效：%s" % lua_path)
            if lua_story_id not in stories:
                raise PackError("包内 Lua 没有对应 Story：%s" % lua_path)
            if lua_story_id == story_id:
                variants.append((lua_path, locale))
        for lua_path, locale in variants:
            try:
                if locale is None:
                    generated = lomc.compile_story(
                        story, mod_info=manifest, source=story_path
                    )
                else:
                    localized = lomc.apply_story_locale(story, locale)
                    generated = lomc.compile_story(
                        localized, mod_info=manifest, source=story_path
                    )
            except Exception as exc:
                raise PackError(f"无法复编译 {story_path} 以验证 {lua_path}：{exc}") from exc
            packaged = entry_bytes[lua_path]
            if generated.encode("utf-8") != packaged:
                raise PackError(
                    f"包内 {lua_path} 不是由对应 {story_path} 编译生成，拒绝导入"
                )


def _import_lommod(path: str | Path) -> tuple[dict, dict[str, dict]]:
    """解 zip 读 manifest + story/*.json。

    返回 (manifest, {story_id: story_dict})。story_id 取文件名去后缀。
    """
    path = Path(path)
    try:
        if path.stat().st_size > MAX_PACKAGE_FILE_BYTES:
            raise PackError("Mod 包文件超过 160 MiB 上限")
    except OSError as exc:
        raise PackError(f"无法读取 {path.name}：{exc}") from exc
    try:
        is_zip = zipfile.is_zipfile(path)
    except OSError as exc:
        raise PackError(f"无法读取 {path.name}：{exc}") from exc
    if not is_zip:
        raise PackError(f"{path.name} 不是合法的 .lommod（zip）文件")
    try:
        zf = zipfile.ZipFile(path)
    except (OSError, zipfile.BadZipFile) as exc:
        raise PackError(f"无法打开 {path.name}：{exc}") from exc
    with zf:
        try:
            entries = _validated_entries(zf)
        except (ValueError, OSError, zipfile.BadZipFile) as exc:
            raise PackError(f"无法检查包内容：{exc}") from exc
        manifest = _read_json_from_zip(zf, entries, "manifest.json")
        source_package_format = manifest.get("package_format", manifest.get("format"))
        try:
            manifest = migrate_manifest(manifest).document
            assert_supported_version(
                manifest, "package_format", PACKAGE_FORMAT, legacy="format"
            )
            assert_supported_version(
                manifest, "story_schema", STORY_SCHEMA
            )
            assert_supported_version(
                manifest, "content_schema", CONTENT_SCHEMA
            )
        except (MigrationError, ValueError) as exc:
            raise PackError(str(exc)) from exc
        stories: dict[str, dict] = {}
        for name in entries:
            if name.startswith("story/") and name.endswith(".json"):
                story = _read_json_from_zip(zf, entries, name)
                try:
                    story = migrate_story(story).document
                    assert_supported_version(
                        story, "story_schema", STORY_SCHEMA
                    )
                except (MigrationError, ValueError) as exc:
                    raise PackError(f"包内 {name}：{exc}") from exc
                story_id = Path(name).stem
                story.setdefault("id", story_id)
                if story.get("id") != story_id:
                    raise PackError(
                        f"包内 {name} 的 id={story.get('id')!r} 与文件名不一致"
                    )
                stories[story_id] = story
        if not stories:
            raise PackError("包内没有 story/*.json（契约要求 ≥1）")
        _verify_story_lua_pairs(
            zf, entries, manifest, stories,
            require_integrity=source_package_format == PACKAGE_FORMAT,
        )
        asset_map: dict[str, str] = {}
        for normalized, info in entries.items():
            if not normalized.startswith("assets/") or normalized.endswith("/"):
                continue
            if Path(normalized).suffix.lower() not in (".png", ".jpg", ".jpeg"):
                continue
            try:
                replacement, _stored = store_image_bytes(
                    Path(normalized).name,
                    _read_entry(zf, info, MAX_IMAGE_BYTES, normalized),
                )
            except (AssetStoreError, OSError, KeyError, ValueError) as exc:
                raise PackError(f"无法导入包内图片 {normalized}：{exc}") from exc
            asset_map[normalized] = replacement
        if asset_map:
            for story in stories.values():
                for node in story.get("nodes") or []:
                    image = str(node.get("image") or "").replace("\\", "/")
                    if image in asset_map:
                        node["image"] = asset_map[image]
        _import_user_audio_from_zip(zf, entries)
    return manifest, stories


def import_lommod(path: str | Path) -> tuple[dict, dict[str, dict]]:
    """安全导入包；所有畸形包和本地读取错误统一转换为 ``PackError``。"""
    try:
        return _import_lommod(path)
    except PackError:
        raise
    except Exception as exc:
        raise PackError(f"无法导入包：{type(exc).__name__}: {exc}") from exc


def _import_user_audio_from_zip(
    zf: zipfile.ZipFile, entries: dict[str, zipfile.ZipInfo]
) -> None:
    """把包内 assets/user/audio/ 登记进本地仓库，便于再编辑。同 ID 已存在则跳过。"""
    import tempfile

    names = [
        name
        for name, info in entries.items()
        if name.startswith("assets/user/") and not info.is_dir()
    ]
    if not names:
        return
    with tempfile.TemporaryDirectory(prefix="lom_user_import_") as tmp:
        tmp_path = Path(tmp)
        for name in names:
            target = (tmp_path / name).resolve()
            if os.path.commonpath((str(tmp_path.resolve()), str(target))) != str(
                tmp_path.resolve()
            ):
                raise PackError(f"包内包含逃逸路径：{name!r}")
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(
                _read_entry(zf, entries[name], MAX_USER_FILE_BYTES, name)
            )
        try:
            content_registry.import_package_audio(tmp_path)
        except Exception as exc:
            raise PackError("无法导入包内用户音频：%s" % exc) from exc


def export_lommod(
    path: str | Path, manifest: dict, stories: dict[str, dict]
) -> list[str]:
    """编译并打包为 .lommod。返回报告行（每个 story 一行编译结果）。

    优先调 lomc.pack_mod（编译器官方打包入口，内部自动编译 lua）；
    lomc 不可用/缺接口时回退为编辑器逐个 compile_story 后自行写 zip。
    任何 story 编译失败都抛 PackError（打包中止），信息里带全部错误。
    """
    path = Path(path)
    manifest = dict(manifest)
    try:
        assert_supported_version(
            manifest,
            "package_format",
            PACKAGE_FORMAT,
            legacy="format",
            allow_missing=True,
        )
        assert_supported_version(
            manifest, "story_schema", STORY_SCHEMA
        )
        assert_supported_version(
            manifest, "content_schema", CONTENT_SCHEMA
        )
        for sid, story in stories.items():
            assert_supported_version(
                story, "story_schema", STORY_SCHEMA
            )
    except ValueError as exc:
        raise PackError("无法导出未知格式：%s" % exc) from exc
    manifest.update(manifest_versions())
    stories = {
        sid: {**story, "story_schema": STORY_SCHEMA}
        for sid, story in stories.items()
    }
    asset_sources: dict[str, Path] = {}
    for rel in sorted(_referenced_images(stories)):
        source = resolve_image_asset(rel)
        if source is None:
            raise PackError(
                f"找不到图片 {rel}。请在对应步骤中点击“选择图片…”重新选择。"
            )
        asset_sources[rel] = source
    user_content: list[tuple[str, str]] = []
    seen_content: set[tuple[str, str]] = set()
    lomc_mod, lomc_err = get_lomc()
    if lomc_mod is None:
        raise PackError("编译器不可用，无法收集用户内容：%s" % lomc_err)
    try:
        lomc_mod.validate_manifest(manifest, source="manifest.json")
    except Exception as exc:
        raise PackError("manifest.json 不符合 v3 契约：%s" % exc) from exc
    from lomc.content import collect_stories_content_refs

    for item in collect_stories_content_refs(stories):
        cid = item["ref"].content_id
        expected_type = item.get("expected_type") or "audio"
        identity = (expected_type, cid)
        if identity in seen_content:
            continue
        seen_content.add(identity)
        try:
            content_registry.resolve(
                cid,
                expected_kind=item["expected_kind"],
                expected_type=expected_type,
                portrait=item.get("portrait"),
            )
        except content_registry.ContentRegistryError as exc:
            raise PackError(str(exc)) from exc
        user_content.append(identity)

    lomc, _err = get_lomc()
    if lomc is not None and hasattr(lomc, "pack_mod"):
        # 官方打包：先把 manifest + story 落到临时目录，再交给 lomc 编译打包
        import tempfile

        with tempfile.TemporaryDirectory(prefix="lom_export_") as tmp:
            tmp_path = Path(tmp)
            (tmp_path / "story").mkdir()
            (tmp_path / "manifest.json").write_text(
                json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
                encoding="utf-8",
            )
            for sid, story in stories.items():
                (tmp_path / "story" / f"{sid}.json").write_text(
                    json.dumps(story, ensure_ascii=False, indent=2) + "\n",
                    encoding="utf-8",
                )
            for rel, source in sorted(asset_sources.items()):
                target = tmp_path / Path(rel)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
            for content_type, content_id in user_content:
                try:
                    content_registry.copy_into_mod(
                        tmp_path, content_id, expected_type=content_type
                    )
                except content_registry.ContentRegistryError as exc:
                    raise PackError(str(exc)) from exc
            try:
                lomc.pack_mod(str(tmp_path), output=str(path))
            except Exception as exc:
                raise PackError(f"lomc 打包失败：{exc}") from exc
        return [f"{sid}.json → lua/{sid}.lua（lomc pack）" for sid in stories]

    # 回退路径：编辑器逐个编译后自行写 zip
    compiled: dict[str, str] = {}
    errors: list[str] = []
    for sid, story in stories.items():
        lua, err = compile_story(story)
        if err is not None or lua is None:
            errors.append(f"[{sid}] {err.splitlines()[-1] if err else '未知错误'}")
        else:
            compiled[sid] = lua
    if errors:
        raise PackError("编译失败，未生成 .lommod：\n" + "\n".join(errors))
    if manifest.get("entry") not in compiled:
        raise PackError(f"入口脚本 {manifest.get('entry')!r} 不在 story 列表中")

    from lomc.content import (
        content_metadata_payload,
        listed_content_files,
        load_content_metadata,
    )
    from lomc.deterministic_zip import DeterministicPackageBuilder

    package = DeterministicPackageBuilder()
    package.add_json("manifest.json", manifest)
    integrity_pairs = []
    for sid, story in stories.items():
        package.add_json(f"story/{sid}.json", story)
        integrity_pairs.append(
            (
                f"story/{sid}.json",
                stable_json_bytes(story),
                f"lua/{sid}.lua",
                compiled[sid].encode("utf-8"),
            )
        )
    for sid, lua in compiled.items():
        package.add_bytes(f"lua/{sid}.lua", lua)
    package.add_bytes(
        STORY_LUA_INTEGRITY_ENTRY,
        build_story_lua_integrity(integrity_pairs),
    )
    for rel, source in sorted(asset_sources.items()):
        package.add_file(rel, source)
    for content_type, content_id in user_content:
        rec, _main_path = content_registry.resolve(
            content_id, expected_type=content_type
        )
        rel_dir = "assets/user/%s/%s" % (rec.type, content_id)
        metadata = load_content_metadata(str(rec.folder / "content.json"))
        package.add_json(
            rel_dir + "/content.json", content_metadata_payload(metadata)
        )
        for fname in listed_content_files(
            {
                "files": {"main": rec.main_file},
                "portraits": rec.portraits or {},
                "intro": rec.intro or {},
                **{
                    field: getattr(rec, field)
                    for field in COMBAT_ANIMATION_FIELDS
                },
            }
        ):
            src = rec.folder / fname
            if src.is_file():
                package.add_file(rel_dir + "/" + fname, src)
    package.write(path)
    return [f"{sid}.json → lua/{sid}.lua 编译成功" for sid in stories]
