# -*- coding: utf-8 -*-
"""pack：mod 目录 -> .lommod（zip）。

按契约 §1 打包：
    manifest.json          包元信息（先校验 §2）
    story/<id>.json        剧情源文件（原样拷贝，供编辑器回读）
    lua/<id>.lua           编译产物（导出时重新编译）
    texts.json             已读文本表 {MOD_<modid>_<scriptid>_<nodeid>: 文本}
                           （仅 say 节点文本；death 文本由 mod_set_death_text
                           直接进 Lua，不进 texts.json / 已读系统）
    assets/                预留目录，存在则原样打进包

默认输出：<mod目录> 同级、以目录名命名的 <目录名>.lommod。
"""

import json
import os
import zipfile

from .compiler import compile_story, load_json_file
from .errors import LomcError
from .validate import validate_manifest

# 汗青书左页插图上限（字节，与运行时插件 §6 一致）：超过则打包报错
_MAX_ENDING_IMAGE_BYTES = 8 * 1024 * 1024

# 汗青书左页插图允许的扩展名（契约 §3.1）
_IMAGE_EXTS = (".png", ".jpg", ".jpeg")


def pack_mod(mod_dir, output=None):
    """校验并打包 mod 目录，返回生成的 .lommod 路径。"""
    mod_dir = os.path.normpath(mod_dir)
    if not os.path.isdir(mod_dir):
        raise LomcError("mod 目录不存在: %s" % mod_dir)

    manifest_path = os.path.join(mod_dir, "manifest.json")
    if not os.path.isfile(manifest_path):
        raise LomcError("mod 目录缺少 manifest.json: %s" % mod_dir)
    manifest = load_json_file(manifest_path)
    validate_manifest(manifest)

    story_dir = os.path.join(mod_dir, "story")
    if not os.path.isdir(story_dir):
        raise LomcError("mod 目录缺少 story/ 子目录: %s" % mod_dir)
    try:
        story_files = sorted(
            f
            for f in os.listdir(story_dir)
            if f.endswith(".json") and os.path.isfile(os.path.join(story_dir, f))
        )
    except OSError as e:
        raise LomcError("story/ 目录读取失败: %s" % e)
    if not story_files:
        raise LomcError("story/ 目录下没有任何 .json 剧情脚本（至少 1 个）")

    # 逐个校验 + 编译；同时收集脚本 id 集与 texts.json 的已读文本表
    compiled = {}  # 脚本 id -> lua 源码
    texts = {}  # 已读 key -> 文本（契约 §1：say/death 节点文本）
    mod_id = manifest["id"]  # 打包时必有（validate_manifest 已保证）
    referenced_assets = set()
    for fname in story_files:
        stem = fname[: -len(".json")]
        story = load_json_file(os.path.join(story_dir, fname))
        inner_id = story.get("id") if isinstance(story, dict) else None
        if inner_id != stem:
            raise LomcError(
                'story/%s: 文件名与内部 id 不一致（文件 "%s" vs id %r），'
                "二者必须相同" % (fname, stem, inner_id)
            )
        for node in story.get("nodes", []):
            # 已读文本表只收 say：death 文本不再走已读 key，由 codegen 发射
            # mod_set_death_text 字面量（官方死亡画面中央显示，见 mod_format §3.1/§6）
            if node.get("type") == "say":
                key = "MOD_%s_%s_%s" % (mod_id, stem, node["id"])
                texts[key] = node["text"]
        lua = compile_story(story, mod_info=manifest, source="story/%s" % fname)
        compiled[stem] = lua

    entry = manifest["entry"]
    if entry not in compiled:
        raise LomcError(
            'manifest.json: entry 指向的入口脚本 "%s" 不存在于 story/ 目录' % entry
        )
    # end 节点 next_script 必须指向包内已有脚本
    for fname in story_files:
        stem = fname[: -len(".json")]
        story = load_json_file(os.path.join(story_dir, fname))
        for node in story["nodes"]:
            if node.get("type") == "end" and node.get("next_script"):
                target = node["next_script"]
                if target not in compiled:
                    raise LomcError(
                        'story/%s 节点 "%s"(end): next_script 指向包内不存在的脚本 "%s"'
                        % (fname, node["id"], target)
                    )
            # 汗青书左页插图（契约 §3.1）：引用必须指向 mod 目录下存在的
            # assets/ 图片，且 ≤8MB（运行时插件读取上限）
            if (
                node.get("type") == "goto_scene"
                and node.get("scene") == "End"
                and node.get("image")
            ):
                image = node["image"]
                # 防御：image 虽经 validate 限定为包内 assets/ 路径，pack 仍再兜一层
                # 路径解析（normpath + 前缀检测，防任何目录逃逸）
                full = os.path.normpath(
                    os.path.join(mod_dir, image.replace("/", os.sep))
                )
                if full != mod_dir and not full.startswith(mod_dir + os.sep):
                    raise LomcError(
                        'story/%s 节点 "%s"(goto_scene): image 不得指向包外：%s'
                        % (fname, node["id"], image)
                    )
                if not os.path.isfile(full):
                    raise LomcError(
                        'story/%s 节点 "%s"(goto_scene): image 指向的文件不存在：'
                        "%s（请放到 mod 目录的 assets/ 下）"
                        % (fname, node["id"], image)
                    )
                if not image.lower().endswith(_IMAGE_EXTS):
                    raise LomcError(
                        'story/%s 节点 "%s"(goto_scene): image 必须是 '
                        ".png/.jpg/.jpeg 图片，实际为 %s" % (fname, node["id"], image)
                    )
                if os.path.getsize(full) > _MAX_ENDING_IMAGE_BYTES:
                    raise LomcError(
                        'story/%s 节点 "%s"(goto_scene): image 超过 8MB '
                        "（运行时插件读取上限），请压缩图片：%s"
                        % (fname, node["id"], image)
                    )
                referenced_assets.add(image.replace("\\", "/"))

    if output is None:
        output = os.path.join(
            os.path.dirname(mod_dir) or ".", os.path.basename(mod_dir) + ".lommod"
        )

    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.write(manifest_path, "manifest.json")
        for fname in story_files:
            zf.write(os.path.join(story_dir, fname), "story/%s" % fname)
        for stem, lua in compiled.items():
            zf.writestr("lua/%s.lua" % stem, lua)
        # 已读文本表（契约 §1/§4）：key 与 lua 里 GetStoryText 的 key 一一对应
        zf.writestr(
            "texts.json",
            json.dumps(texts, ensure_ascii=False, indent=2) + "\n",
        )
        # 目前 assets 的受控用途只有 End.image。只收剧情明确引用且已通过
        # 路径/大小校验的文件，避免把本机 assets/ 中未使用的原版素材意外分发。
        for rel in sorted(referenced_assets):
            full = os.path.join(mod_dir, rel.replace("/", os.sep))
            zf.write(full, rel)

    return output
