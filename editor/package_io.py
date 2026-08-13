# -*- coding: utf-8 -*-
"""`.lommod` 包的导入/导出（包结构契约见 docs/mod_format.md §1/§2）。

导出时必须重新编译：story/*.json → lua/*.lua，二者同名。
编译优先调 lomc.pack_mod（编译器官方打包入口），否则编辑器自行
逐个调 lomc.compile_story 并写 zip。
"""

from __future__ import annotations

import json
import zipfile
from pathlib import Path

from lua_preview import compile_story, get_lomc


class PackError(Exception):
    """导入/导出失败，message 面向用户可读。"""


def _read_json_from_zip(zf: zipfile.ZipFile, name: str) -> dict:
    try:
        with zf.open(name) as f:
            return json.loads(f.read().decode("utf-8"))
    except KeyError:
        raise PackError(f"包内缺少文件：{name}")
    except json.JSONDecodeError as exc:
        raise PackError(f"包内 {name} 不是合法 JSON：{exc}")


def import_lommod(path: str | Path) -> tuple[dict, dict[str, dict]]:
    """解 zip 读 manifest + story/*.json。

    返回 (manifest, {story_id: story_dict})。story_id 取文件名去后缀。
    """
    path = Path(path)
    if not zipfile.is_zipfile(path):
        raise PackError(f"{path.name} 不是合法的 .lommod（zip）文件")
    with zipfile.ZipFile(path) as zf:
        manifest = _read_json_from_zip(zf, "manifest.json")
        if manifest.get("format") != 1:
            raise PackError(f"不支持的 format：{manifest.get('format')!r}（仅支持 1）")
        stories: dict[str, dict] = {}
        for name in zf.namelist():
            if name.startswith("story/") and name.endswith(".json"):
                story = _read_json_from_zip(zf, name)
                story_id = Path(name).stem
                story.setdefault("id", story_id)
                stories[story_id] = story
        if not stories:
            raise PackError("包内没有 story/*.json（契约要求 ≥1）")
    return manifest, stories


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
    return [f"{sid}.json → lua/{sid}.lua 编译成功" for sid in stories]
