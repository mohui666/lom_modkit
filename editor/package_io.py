# -*- coding: utf-8 -*-
"""`.lommod` 包的导入/导出（包结构契约见 docs/zh_CN/mod_format.md §1/§2）。

导出时必须重新编译：story/*.json → lua/*.lua，二者同名。
编译优先调 lomc.pack_mod（编译器官方打包入口），否则编辑器自行
逐个调 lomc.compile_story 并写 zip。
"""

from __future__ import annotations

import json
import os
import shutil
import zipfile
from pathlib import Path, PurePosixPath, PureWindowsPath

from asset_store import AssetStoreError, resolve_image_asset, store_image_bytes
import content_registry
from lua_preview import compile_story, get_lomc


class PackError(Exception):
    """导入/导出失败，message 面向用户可读。"""


MAX_ARCHIVE_ENTRIES = 2048
MAX_ARCHIVE_UNCOMPRESSED = 128 * 1024 * 1024
MAX_JSON_BYTES = 4 * 1024 * 1024
MAX_IMAGE_BYTES = 24 * 1024 * 1024
MAX_USER_FILE_BYTES = 32 * 1024 * 1024


def _safe_archive_name(name: str) -> str:
    """Return a canonical ZIP path, rejecting paths unsafe on POSIX or Windows."""
    normalized = name.replace("\\", "/")
    posix = PurePosixPath(normalized)
    windows = PureWindowsPath(name)
    if (
        not normalized
        or normalized.startswith("/")
        or posix.is_absolute()
        or windows.is_absolute()
        or windows.drive
        or any(part == ".." for part in posix.parts)
    ):
        raise PackError(f"包内包含不安全路径：{name!r}")
    canonical = str(posix)
    if normalized.endswith("/") and canonical != ".":
        canonical += "/"
    return canonical


def _validated_entries(zf: zipfile.ZipFile) -> dict[str, zipfile.ZipInfo]:
    infos = zf.infolist()
    if len(infos) > MAX_ARCHIVE_ENTRIES:
        raise PackError(f"包内条目过多（最多 {MAX_ARCHIVE_ENTRIES} 个）")
    total = 0
    entries: dict[str, zipfile.ZipInfo] = {}
    for info in infos:
        name = _safe_archive_name(info.filename)
        total += info.file_size
        if info.file_size < 0 or total > MAX_ARCHIVE_UNCOMPRESSED:
            raise PackError("包解压后总大小超过 128 MiB")
        if name in entries:
            raise PackError(f"包内存在重复路径：{name}")
        entries[name] = info
    return entries


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


def _import_lommod(path: str | Path) -> tuple[dict, dict[str, dict]]:
    """解 zip 读 manifest + story/*.json。

    返回 (manifest, {story_id: story_dict})。story_id 取文件名去后缀。
    """
    path = Path(path)
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
        if manifest.get("format") != 1:
            raise PackError(f"不支持的 format：{manifest.get('format')!r}（仅支持 1）")
        stories: dict[str, dict] = {}
        for name in entries:
            if name.startswith("story/") and name.endswith(".json"):
                story = _read_json_from_zip(zf, entries, name)
                story_id = Path(name).stem
                story.setdefault("id", story_id)
                stories[story_id] = story
        if not stories:
            raise PackError("包内没有 story/*.json（契约要求 ≥1）")
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
    manifest.setdefault("format", 1)
    asset_sources: dict[str, Path] = {}
    for rel in sorted(_referenced_images(stories)):
        source = resolve_image_asset(rel)
        if source is None:
            raise PackError(
                f"找不到图片 {rel}。请在对应步骤中点击“选择图片…”重新选择。"
            )
        asset_sources[rel] = source
    user_audio_ids: list[str] = []
    seen_audio = set()
    lomc_mod, lomc_err = get_lomc()
    if lomc_mod is None:
        raise PackError("编译器不可用，无法收集用户音频：%s" % lomc_err)
    from lomc.content import collect_stories_content_refs

    for item in collect_stories_content_refs(stories):
        cid = item["ref"].content_id
        if cid in seen_audio:
            continue
        seen_audio.add(cid)
        try:
            content_registry.resolve(cid, expected_kind=item["expected_kind"])
        except content_registry.ContentRegistryError as exc:
            raise PackError(str(exc)) from exc
        user_audio_ids.append(cid)

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
            for content_id in user_audio_ids:
                try:
                    content_registry.copy_into_mod(tmp_path, content_id)
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

    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr(
            "manifest.json", json.dumps(manifest, ensure_ascii=False, indent=2) + "\n"
        )
        for sid, story in stories.items():
            zf.writestr(
                f"story/{sid}.json",
                json.dumps(story, ensure_ascii=False, indent=2) + "\n",
            )
        for sid, lua in compiled.items():
            zf.writestr(f"lua/{sid}.lua", lua)
        for rel, source in sorted(asset_sources.items()):
            zf.write(source, rel)
        for content_id in user_audio_ids:
            rec, _main_path = content_registry.resolve(content_id)
            rel_dir = "assets/user/%s/%s" % (rec.type, content_id)
            zf.write(rec.folder / "content.json", rel_dir + "/content.json")
            from lomc.content import listed_content_files

            for fname in listed_content_files(
                {
                    "files": {"main": rec.main_file},
                    "portraits": rec.portraits or {},
                    "intro": rec.intro or {},
                }
            ):
                src = rec.folder / fname
                if src.is_file():
                    zf.write(src, rel_dir + "/" + fname)
    return [f"{sid}.json → lua/{sid}.lua 编译成功" for sid in stories]
