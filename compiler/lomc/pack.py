# -*- coding: utf-8 -*-
"""pack：mod 目录 -> .lommod（zip）。

按契约 §1 打包：
    manifest.json          包元信息（先校验 §2）
    story/<id>.json        剧情源文件（原样拷贝，供编辑器回读）
    lua/<id>.lua           编译产物（导出时重新编译）
    assets/                预留目录，存在则原样打进包

默认输出：<mod目录> 同级、以目录名命名的 <目录名>.lommod。
"""

import os
import zipfile

from .compiler import compile_story, load_json_file
from .errors import LomcError
from .validate import validate_manifest


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
    story_files = sorted(
        f for f in os.listdir(story_dir)
        if f.endswith(".json") and os.path.isfile(os.path.join(story_dir, f))
    )
    if not story_files:
        raise LomcError("story/ 目录下没有任何 .json 剧情脚本（至少 1 个）")

    # 逐个校验 + 编译；同时收集脚本 id 集用于 entry / next_script 交叉校验
    compiled = {}  # 脚本 id -> lua 源码
    for fname in story_files:
        stem = fname[: -len(".json")]
        story = load_json_file(os.path.join(story_dir, fname))
        inner_id = story.get("id") if isinstance(story, dict) else None
        if inner_id != stem:
            raise LomcError(
                'story/%s: 文件名与内部 id 不一致（文件 "%s" vs id %r），'
                "二者必须相同" % (fname, stem, inner_id)
            )
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
        assets_dir = os.path.join(mod_dir, "assets")
        if os.path.isdir(assets_dir):
            for root, _dirs, files in os.walk(assets_dir):
                for f in sorted(files):
                    full = os.path.join(root, f)
                    rel = os.path.relpath(full, mod_dir).replace(os.sep, "/")
                    zf.write(full, rel)

    return output
