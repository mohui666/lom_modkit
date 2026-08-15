# -*- coding: utf-8 -*-
"""pack：mod 目录 -> .lommod（zip）。

按契约 §1 打包：
    manifest.json          包元信息（先校验 §2）
    story/<id>.json        剧情源文件（原样拷贝，供编辑器回读）
    lua/<id>.lua           编译产物（导出时重新编译）
    texts.json             已读文本表 {MOD_<modid>_<scriptid>_<nodeid>: 文本}
                           （仅 say 节点文本；death 文本由 mod_set_death_text
                           直接进 Lua，不进 texts.json / 已读系统）
    assets/                仅收剧情明确引用的资源：
                           - End.image / 自定义 intro.image（包内 assets/ 图片，≤8MB）
                           - user: 用户音频 / 自定义角色（assets/user/<type>/<id>/，仅实际引用）
                           未引用的本机素材不会打进包，避免意外分发

默认输出：<mod目录> 同级、以目录名命名的 <目录名>.lommod。
"""

import os

from .compiler import compile_story, load_json_file
from .content import (
    check_character_portrait,
    check_content_matches_kind,
    collect_story_content_refs,
    content_metadata_payload,
    listed_content_files,
    package_content_dir,
    resolve_content,
)
from .errors import LomcError
from .deterministic_zip import DeterministicPackageBuilder
from .localization import SUPPORTED_LOCALES, apply_story_locale, localization_config
from .validate import validate_manifest, validate_story
from .schema_versions import STORY_SCHEMA, version_declarations

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
    manifest = dict(manifest)
    manifest.update(version_declarations())
    manifest["format"] = manifest["package_format"]  # legacy v1 readers

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
    loaded_stories = {}
    localized_compiled = {locale: {} for locale in SUPPORTED_LOCALES}
    localized_texts = {locale: {} for locale in SUPPORTED_LOCALES}
    package_localization = None
    mod_id = manifest["id"]  # 打包时必有（validate_manifest 已保证）
    referenced_assets = set()
    referenced_user_content = {}  # (type, content_id) -> (meta, folder_abs)
    referenced_id_types = {}  # content_id -> type（user: 引用本身不携带类型）
    for fname in story_files:
        stem = fname[: -len(".json")]
        story = load_json_file(os.path.join(story_dir, fname))
        validate_story(story, source="story/%s" % fname)
        story = dict(story)
        story["story_schema"] = STORY_SCHEMA
        inner_id = story.get("id") if isinstance(story, dict) else None
        if inner_id != stem:
            raise LomcError(
                'story/%s: 文件名与内部 id 不一致（文件 "%s" vs id %r），'
                "二者必须相同" % (fname, stem, inner_id)
            )
        loaded_stories[stem] = story
        story_localization = localization_config(story)
        if story_localization is not None:
            current = (
                story_localization["default_locale"],
                story_localization.get("fallback_locale", story_localization["default_locale"]),
            )
            if package_localization is None:
                package_localization = current
            elif package_localization != current:
                raise LomcError(
                    "包内所有本地化 Story 必须使用相同 default_locale/fallback_locale；"
                    "story/%s 为 %s/%s，之前为 %s/%s"
                    % ((fname,) + current + package_localization)
                )
        for node in story.get("nodes", []):
            # 已读文本表只收 say：death 文本不再走已读 key，由 codegen 发射
            # mod_set_death_text 字面量（官方死亡画面中央显示，见 mod_format §3.1/§6）
            if node.get("type") == "say":
                key = "MOD_%s_%s_%s" % (mod_id, stem, node["id"])
                texts[key] = node["text"]
        lua = compile_story(
            story,
            mod_info=manifest,
            source="story/%s" % fname,
            content_root=mod_dir,
        )
        compiled[stem] = lua

    # 本地化包为四种受支持语言各编译完整脚本；未本地化章节原样复用。
    # 因此语言切换只选择脚本，不需要在 Lua 内引入任何新 API。
    if package_localization is not None:
        for locale in SUPPORTED_LOCALES:
            for stem, story in loaded_stories.items():
                localized_story = (
                    apply_story_locale(story, locale)
                    if localization_config(story) is not None else story
                )
                localized_compiled[locale][stem] = compile_story(
                    localized_story,
                    mod_info=manifest,
                    source="story/%s.json" % stem,
                    content_root=mod_dir,
                )
                for node in localized_story.get("nodes", []):
                    if node.get("type") == "say":
                        key = "MOD_%s_%s_%s" % (mod_id, stem, node["id"])
                        localized_texts[locale][key] = node["text"]

    entry = manifest["entry"]
    if entry not in compiled:
        raise LomcError(
            'manifest.json: entry 指向的入口脚本 "%s" 不存在于 story/ 目录' % entry
        )
    triggers = (manifest.get("campaign") or {}).get("triggers", [])
    for index, trigger in enumerate(triggers):
        target = trigger["script"]
        if target not in compiled:
            raise LomcError(
                'manifest.json: campaign.triggers 第 %d 项的 script '
                '指向包内不存在的脚本 "%s"' % (index + 1, target)
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
            # 汗青书插图 / 自定义人物介绍图：必须位于包内 assets/，且 ≤8MB。
            is_ending_image = (
                node.get("type") == "goto_scene"
                and node.get("scene") == "End"
                and node.get("image")
            )
            is_intro_image = (
                node.get("type") == "intro"
                and node.get("intro_source", "official") == "custom"
                and node.get("image")
            )
            if is_ending_image or is_intro_image:
                image = node["image"]
                # 防御：image 虽经 validate 限定为包内 assets/ 路径，pack 仍再兜一层
                # 路径解析（normpath + 前缀检测，防任何目录逃逸）
                full = os.path.normpath(
                    os.path.join(mod_dir, image.replace("/", os.sep))
                )
                if full != mod_dir and not full.startswith(mod_dir + os.sep):
                    raise LomcError(
                        'story/%s 节点 "%s"(%s): image 不得指向包外：%s'
                        % (fname, node["id"], node.get("type"), image)
                    )
                if not os.path.isfile(full):
                    raise LomcError(
                        'story/%s 节点 "%s"(%s): image 指向的文件不存在：'
                        "%s（请放到 mod 目录的 assets/ 下）"
                        % (fname, node["id"], node.get("type"), image)
                    )
                if not image.lower().endswith(_IMAGE_EXTS):
                    raise LomcError(
                        'story/%s 节点 "%s"(%s): image 必须是 '
                        ".png/.jpg/.jpeg 图片，实际为 %s"
                        % (fname, node["id"], node.get("type"), image)
                    )
                if os.path.getsize(full) > _MAX_ENDING_IMAGE_BYTES:
                    raise LomcError(
                        'story/%s 节点 "%s"(%s): image 超过 8MB '
                        "（运行时插件读取上限），请压缩图片：%s"
                        % (fname, node["id"], node.get("type"), image)
                    )
                referenced_assets.add(image.replace("\\", "/"))
        for item in collect_story_content_refs(story):
            ref = item["ref"]
            ctype = item.get("expected_type") or "audio"
            previous_type = referenced_id_types.get(ref.content_id)
            if previous_type is not None and previous_type != ctype:
                raise LomcError(
                    'story/%s 节点 "%s"(%s): 用户内容 %s 同时被当作 %s / %s；'
                    "稳定 user: ID 在一个包内只能属于一种类型"
                    % (
                        fname,
                        item["node_id"],
                        item["node_type"],
                        ref.raw,
                        previous_type,
                        ctype,
                    )
                )
            referenced_id_types[ref.content_id] = ctype
            try:
                meta, main_path = resolve_content(mod_dir, ctype, ref.content_id)
                if ctype == "character":
                    check_character_portrait(meta, item.get("portrait"), ref.raw)
                elif ctype == "audio":
                    check_content_matches_kind(meta, item["expected_kind"], ref.raw)
            except LomcError as exc:
                raise LomcError(
                    'story/%s 节点 "%s"(%s): %s'
                    % (fname, item["node_id"], item["node_type"], exc)
                )
            referenced_user_content[(ctype, ref.content_id)] = (
                meta,
                os.path.dirname(main_path),
            )

    if output is None:
        output = os.path.join(
            os.path.dirname(mod_dir) or ".", os.path.basename(mod_dir) + ".lommod"
        )

    package = DeterministicPackageBuilder()
    package.add_json("manifest.json", manifest)
    for fname in story_files:
        stem = fname[: -len(".json")]
        package.add_json("story/%s" % fname, loaded_stories[stem])
    for stem, lua in compiled.items():
        package.add_bytes("lua/%s.lua" % stem, lua)
    package.add_json("texts.json", texts)
    if package_localization is not None:
        default_locale, fallback_locale = package_localization
        package.add_json(
            "localization.json",
            {
                "schema": 1,
                "default_locale": default_locale,
                "fallback_locale": fallback_locale,
                "locales": list(SUPPORTED_LOCALES),
            },
        )
        for locale in SUPPORTED_LOCALES:
            for stem, lua in localized_compiled[locale].items():
                package.add_bytes("lua/%s/%s.lua" % (locale, stem), lua)
            package.add_json("texts/%s.json" % locale, localized_texts[locale])
    # assets 只收剧情明确引用的图片与用户内容，避免把本机未使用素材意外分发。
    for rel in sorted(referenced_assets):
        full = os.path.join(mod_dir, rel.replace("/", os.sep))
        package.add_file(rel, full)
    for ctype, content_id in sorted(referenced_user_content):
        meta, folder = referenced_user_content[(ctype, content_id)]
        rel_dir = package_content_dir(ctype, content_id)
        package.add_json(rel_dir + "/content.json", content_metadata_payload(meta))
        for fname in listed_content_files(meta):
            package.add_file(rel_dir + "/" + fname, os.path.join(folder, fname))

    final_path, _content_hash = package.write(output)
    return final_path
