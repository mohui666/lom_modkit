# -*- coding: utf-8 -*-
"""story.json / manifest.json 校验。

严格遵循 docs/zh_CN/mod_format.md（v3 契约）：
- §2 manifest.json 字段
- §3 story/*.json 结构与 §3.1 全量节点类型表（字段除标注"可选"外均为必填）
- §4 补充规则（末节点收尾、禁止显式 goto 的类型、分支兜底等）

所有错误抛出 LomcError，消息带节点 id / 字段名。
"""

import math
import re
import unicodedata

from .content import (
    UNSUPPORTED_USER_CHAR_TYPES,
    is_user_ref,
    parse_content_ref,
    validate_portrait_id,
)
from .dice_data import (
    check_portrait,
    get_dice_meta,
    get_official_scene_ids,
    load_portrait_table,
)
from .errors import LomcError
from .schema_versions import CONTENT_SCHEMA, PACKAGE_FORMAT, STORY_SCHEMA

# §1：剧情脚本 id 规则
SCRIPT_ID_RE = re.compile(r"^[a-zA-Z0-9_\-]{1,64}$")
# §2：mod id 规则
MOD_ID_RE = re.compile(r"^[a-z0-9_\-]{1,64}$")
# 节点 id 会拼进 Lua 函数名 node_<id>，必须落在 Lua 标识符安全字符集内
NODE_ID_RE = re.compile(r"^[a-zA-Z0-9_]+$")
# 编辑器显示标签是“名称（内部ID）”，游戏接口只能接收括号内的内部 ID。
CHARACTER_DISPLAY_RE = re.compile(r"^.+（[a-zA-Z0-9_\-]+）$")

_MANIFEST_TEXT_LIMITS = {
    "name": 80,
    "version": 32,
    "author": 80,
    "description": 500,
}

_SEMVER_RE = re.compile(
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)
_GAME_VERSION_RE = re.compile(r"^[0-9A-Za-z][0-9A-Za-z._+\-]{0,63}$")


def _parse_host_version(field, value):
    if not isinstance(value, str) or _SEMVER_RE.fullmatch(value) is None:
        raise LomcError(
            '字段 "%s" 必须是 SemVer（如 0.6.0 或 0.7.0-beta.1），实际为 %r'
            % (field, value)
        )
    match = _SEMVER_RE.fullmatch(value)
    core = tuple(int(match.group(i)) for i in (1, 2, 3))
    prerelease = match.group(4)
    if any(part > 2147483647 for part in core):
        raise LomcError('字段 "%s" 的版本数字不能超过 2147483647' % field)
    for identifier in (prerelease or "").split("."):
        if identifier.isdigit() and (
            (len(identifier) > 1 and identifier.startswith("0"))
            or int(identifier) > 2147483647
        ):
            raise LomcError(
                '字段 "%s" 的预发布数字必须无前导零且不超过 2147483647'
                % field
            )
    return core, prerelease


def _host_version_greater(left, right):
    if left[0] != right[0]:
        return left[0] > right[0]
    if left[1] is None:
        return right[1] is not None
    if right[1] is None:
        return False
    left_parts = left[1].split(".")
    right_parts = right[1].split(".")
    for lpart, rpart in zip(left_parts, right_parts):
        if lpart == rpart:
            continue
        lnum = lpart.isdigit()
        rnum = rpart.isdigit()
        if lnum and rnum:
            return int(lpart) > int(rpart)
        if lnum != rnum:
            return not lnum  # numeric prerelease identifiers have lower precedence
        return lpart > rpart
    return len(left_parts) > len(right_parts)


def _validate_compatibility_metadata(manifest):
    parsed = {}
    for field in ("min_host_version", "tested_host_version"):
        if field in manifest:
            parsed[field] = _parse_host_version(field, manifest.get(field))
    if (
        "min_host_version" in parsed
        and "tested_host_version" in parsed
        and _host_version_greater(
            parsed["min_host_version"], parsed["tested_host_version"]
        )
    ):
        raise LomcError("min_host_version 不能高于 tested_host_version")
    for field in ("game_version", "tested_game_version"):
        if field in manifest:
            value = manifest.get(field)
            if not isinstance(value, str) or _GAME_VERSION_RE.fullmatch(value) is None:
                raise LomcError(
                    '字段 "%s" 必须是 1~64 位版本标识（字母、数字、.-_+），实际为 %r'
                    % (field, value)
                )
    if (
        "game_version" in manifest
        and "tested_game_version" in manifest
        and manifest["game_version"].casefold()
        != manifest["tested_game_version"].casefold()
    ):
        raise LomcError("game_version 与 tested_game_version 不能互相矛盾")


def _validate_manifest_display_text(field, value):
    limit = _MANIFEST_TEXT_LIMITS[field]
    if not isinstance(value, str) or not value:
        raise LomcError('缺少必填字段 "%s"（非空字符串）' % field)
    if len(value) > limit:
        raise LomcError('字段 "%s" 不能超过 %d 个字符' % (field, limit))
    for char in value:
        category = unicodedata.category(char)
        if char in "\r\n" or category in ("Cc", "Cf", "Zl", "Zp"):
            raise LomcError(
                '字段 "%s" 必须是单行可见文本，不能含控制、零宽或双向格式字符'
                % field
            )

SAY_MODES = ("character", "think", "narrative", "center")
FACINGS = ("left", "right")
BRANCH_SOURCES = ("mod", "game", "stat", "flag_value", "condition")
# stat/flag_value 来源的数值比较运算符（缺省 ">="）
BRANCH_OPS = (">=", ">", "<=", "<", "==")
CAMPAIGN_POSITIONS = (
    "Mall",
    "Center",
    "Alchemy",
    "Forge",
    "BackMountain",
    "Room1",
    "Door",
    "Study",
    "Kitchen",
    "Room2",
    "Secret",
)

# 枚举字段的合法值（§3.1）
_ENUMS = {
    "music_op": ("play", "stop", "fadeout"),
    "sound_kind": ("sound", "env"),
    "sound_op": ("play", "fadeout"),
    "background_action": ("set", "show", "replace", "fadein", "fadeout", "clear"),
    "overlay_action": ("show", "hide"),
    "overlay_position": (
        "center", "top", "bottom", "left", "right",
        "top_left", "top_right", "bottom_left", "bottom_right",
    ),
    "overlay_layer": ("back", "front"),
    "transition_phase": ("in", "out"),
    "transition_dir": ("lr", "rl", "tb", "bt"),
    "block_fc": ("view", "common"),
    "cg_action": ("show", "hide"),
    "cg_kind": ("picture", "item", "big", "map", "family", "title"),
    "item_kind": ("book", "misc", "special"),
    "gameflag_op": ("set", "add"),
    "enemy_op": ("team", "level", "people", "id"),
    "bskill_op": ("set", "active", "reset"),
    "gameplay_kind": ("any", "combat", "battle"),
    "reward_kind": ("stat", "affinity", "talent", "item", "flag"),
    "shop_item_kind": ("book", "misc", "special"),
    "shop_condition_source": ("mod", "condition"),
    "time_op": ("set", "round", "month", "mission"),
    "autosave_kind": ("story", "free", "prologue"),
    "goto_scene": (
        "Free",
        "Title",
        "Combat",
        "Battle",
        "GameOver",
        "End",
        "Story",
        "DemoEnd",
    ),
    "panel_kind": (
        "martial",
        "weapon",
        "poison",
        "cg",
        "cgvideo",
        "shop",
        "newshop",
        "credit",
        "endgame",
    ),
    "branch_source": BRANCH_SOURCES,
    "mode": SAY_MODES,
    "facing": FACINGS,
    "death_next": ("Free", "Title"),
    "intro_source": ("official", "custom", "character"),
}

# 字段类型标签 -> 中文类型名（用于报错）
_TYPE_NAMES = {
    "str": "字符串",
    "idstr": "非空字符串（官方/资源 id）",
    "num": "数值",
    "bool": "布尔值",
    "list": "数组",
    "talent_level": "1 或 -1",
    "save_button": "0 或 1",
    "script_id": "脚本 id 字符串",
}
for _tag, _vals in _ENUMS.items():
    _TYPE_NAMES[_tag] = "/".join('"%s"' % v for v in _vals)


def _check_type(tag, value):
    if tag == "str":
        return isinstance(value, str)
    if tag == "idstr":
        # “必须是官方 id”的字段：清单由编辑器侧管（契约 §4 任务分工），
        # 编译器只做非空校验，不查表
        return isinstance(value, str) and value != ""
    if tag == "num":
        # bool 是 int 的子类，数值字段不接受 true/false
        return (
            (isinstance(value, int) and not isinstance(value, bool))
            or (isinstance(value, float) and math.isfinite(value))
        )
    if tag == "bool":
        return isinstance(value, bool)
    if tag == "list":
        return isinstance(value, list)
    if tag == "talent_level":
        return not isinstance(value, bool) and value in (1, -1)
    if tag == "save_button":
        return value in (0, 1) and not isinstance(value, bool)
    if tag == "script_id":
        return isinstance(value, str) and SCRIPT_ID_RE.fullmatch(value) is not None
    if tag in _ENUMS:
        return value in _ENUMS[tag]
    raise AssertionError("未知字段类型标签: %r" % tag)


# §3.1 节点字段表：type -> (必填字段 {名: 类型}, 可选字段 {名: 类型})
# 表只描述字段存在性与类型；跨字段规则（联动、组合、选项结构等）
# 在 _check_node_extra 里单独校验。
_NODE_FIELDS = {
    # ---- 演出类 ----
    "music": ({"name": "str"}, {"op": "music_op", "seconds": "num"}),
    "sound": (
        {"name": "str"},
        {"kind": "sound_kind", "op": "sound_op", "seconds": "num"},
    ),
    "scene": ({"view": "str"}, {}),
    "background": (
        {"action": "background_action"},
        {"image": "str", "fade": "num"},
    ),
    "custom_cg": (
        {"action": "cg_action"},
        {"image": "str", "fade": "num", "scale": "num", "x": "num", "y": "num"},
    ),
    "overlay": (
        {"action": "overlay_action", "slot": "str"},
        {
            "image": "str", "position": "overlay_position", "scale": "num",
            "opacity": "num", "layer": "overlay_layer", "fade": "num",
        },
    ),
    "show": (
        {"character": "str", "position": "str"},
        {
            "portrait": "str",
            "facing": "facing",
            "fadeDuration": "num",
            "moveDuration": "num",
        },
    ),
    "move": (
        {"character": "str", "from": "str", "to": "str"},
        {"duration": "num"},
    ),
    "face": ({"character": "str", "facing": "facing"}, {}),
    "hide": ({"character": "str"}, {"fadeDuration": "num"}),
    "focus": ({"character": "str"}, {}),
    "offset": (
        {"character": "str", "x": "num", "y": "num", "duration": "num"},
        {},
    ),
    "say": (
        {"text": "str"},
        {"character": "str", "portrait": "str", "mode": "mode", "voice": "str"},
    ),
    "choice": ({"options": "list"}, {"dialog": "idstr"}),
    "shock": ({"character": "str"}, {"duration": "num"}),
    "mask": ({"show": "bool"}, {}),
    "intro": (
        {},
        {
            "intro_source": "intro_source",
            "character": "str",
            "title": "str",
            "name": "str",
            "text": "str",
            "image": "str",
            "image_scale": "num",
            "image_x": "num",
            "image_y": "num",
        },
    ),
    "effect": (
        {"name": "str"},
        {
            "x": "num",
            "y": "num",
            "a": "num",
            "b": "num",
            "c": "num",
            "d": "num",
            "play": "bool",
        },
    ),
    "transition": ({"phase": "transition_phase"}, {"dir": "transition_dir"}),
    "camera": ({"name": "str", "active": "bool"}, {}),
    "block": (
        {"flowchart": "block_fc", "name": "str"},
        {"vars": "list"},
    ),
    "cg": (
        {"action": "cg_action", "kind": "cg_kind"},
        {"key": "str", "key2": "str", "n1": "num", "n2": "num"},
    ),
    "dim": ({"character": "str", "dimmed": "bool"}, {}),
    "message": ({"text": "str"}, {}),
    "rotate": ({"character": "str", "angle": "num", "duration": "num"}, {}),
    # dayenv 环境字段名 day_type：字段名 "type" 与节点通用键 "type"（节点类型）
    # 冲突（dict 无法同名共存），故契约命名 day_type
    "dayenv": ({"day_type": "num"}, {}),
    # ---- 数值/状态类 ----
    "stat": (
        {"key": "str", "delta": "num"},
        {"waitDisplay": "bool", "display": "num", "mode": "str"},
    ),
    "stat_set": ({"key": "str", "value": "num"}, {"update": "bool"}),
    "affinity": ({"character": "str", "delta": "num"}, {}),
    "talent": ({"talent": "idstr", "level": "talent_level"}, {}),
    "item": (
        {"kind": "item_kind", "item": "idstr"},
        {"count": "num", "remove": "bool"},
    ),
    "flag": ({"flag": "str"}, {}),
    "game_flag": ({"flag": "idstr", "value": "num"}, {"op": "gameflag_op"}),
    "enemy": (
        {"op": "enemy_op", "enemy": "idstr"},
        {"value": "num", "display": "num"},
    ),
    "battle_skill": (
        {"op": "bskill_op"},
        {"key": "idstr", "index": "num", "active": "num"},
    ),
    "battle_setup": (
        {},
        {
            "enemy": "idstr", "team": "num", "level": "num",
            "people": "num", "display": "num", "reset_skills": "bool",
            "skills": "list",
        },
    ),
    "combat": (
        {"win": "idstr", "lose": "idstr"},
        {
            "preset": "idstr", "key": "idstr",
            "enemy": "idstr", "team": "num", "level": "num",
            "people": "num", "display": "num",
        },
    ),
    "battle": (
        {"win": "idstr", "lose": "idstr"},
        {"preset": "idstr", "key": "idstr"},
    ),
    "battle_result": (
        {"win": "idstr", "lose": "idstr"},
        {"kind": "gameplay_kind"},
    ),
    "reward": ({"entries": "list"}, {}),
    "custom_shop": ({"items": "list"}, {"discount": "num"}),
    "mission": ({"name": "idstr", "key": "idstr"}, {}),
    "time": (
        {"op": "time_op"},
        {"year": "num", "month": "num", "stage": "num", "name": "idstr"},
    ),
    "autosave": ({}, {"kind": "autosave_kind", "save_button": "save_button"}),
    # ---- 流程类 ----
    "branch": (
        {"cases": "list"},
        {"source": "branch_source", "flag": "str", "stat": "str"},
    ),
    "dice": ({"check": "idstr", "options": "list"}, {}),
    "goto_scene": (
        {"scene": "goto_scene"},
        {"key": "idstr", "next": "str", "title": "str", "desc": "str", "image": "str"},
    ),
    "panel": (
        {"panel": "panel_kind"},
        {"key": "idstr", "discount": "num", "mode": "num"},
    ),
    "wait": ({"seconds": "num"}, {}),
    "end": ({}, {"next_script": "script_id"}),
    "death": (
        {"text": "str", "death_id": "idstr"},
        {"title": "str", "next": "death_next"},
    ),
    "raw": ({"code": "str"}, {}),
}

# 任何节点都允许的通用字段
_COMMON_FIELDS = ("id", "type", "goto")

# 不允许显式 goto 的节点类型（契约 §4：流转由自身结构/场景跳转决定）
_NO_GOTO_TYPES = ("choice", "branch", "dice", "end", "goto_scene", "death", "combat", "battle", "battle_result")

# 可以作最后一个节点收尾的类型（其余类型在末位且无 goto → 校验错误）
_TERMINAL_TYPES = ("end", "choice", "branch", "dice", "goto_scene", "raw", "death", "combat", "battle", "battle_result")

# dice 选项的字段(§3.1)：三向 goto 载体（text/threshold 已废弃，以官方结果带元数据为准）
_DICE_OPTION_GOTOS = ("goto_大成功", "goto_成功", "goto_失败")


def _node_label(node, index):
    """生成用于报错的节点描述，尽量带 id。"""
    nid = node.get("id") if isinstance(node, dict) else None
    if isinstance(nid, str):
        return '节点 "%s"' % nid
    return "第 %d 个节点" % (index + 1)


def _check_node_fields(node, label):
    """按 §3.1 校验单个节点的字段存在性与类型，返回节点 type。"""
    ntype = node.get("type")
    if not isinstance(ntype, str):
        raise LomcError('%s: 缺少必填字段 "type"（字符串）' % label)
    if ntype not in _NODE_FIELDS:
        known = "、".join(sorted(_NODE_FIELDS))
        raise LomcError('%s: 未知节点类型 "%s"（支持：%s）' % (label, ntype, known))

    required, optional = _NODE_FIELDS[ntype]
    allowed = set(_COMMON_FIELDS) | set(required) | set(optional)
    for key in node:
        if key not in allowed:
            raise LomcError(
                '%s(%s): 未知字段 "%s"（允许：%s）'
                % (label, ntype, key, "、".join(sorted(allowed)))
            )

    for name, tag in required.items():
        if name not in node:
            raise LomcError('%s(%s): 缺少必填字段 "%s"' % (label, ntype, name))
        if not _check_type(tag, node[name]):
            raise LomcError(
                '%s(%s): 字段 "%s" 必须是%s，实际为 %r'
                % (label, ntype, name, _TYPE_NAMES[tag], node[name])
            )
    for name, tag in optional.items():
        if name in node and not _check_type(tag, node[name]):
            raise LomcError(
                '%s(%s): 可选字段 "%s" 必须是%s，实际为 %r'
                % (label, ntype, name, _TYPE_NAMES[tag], node[name])
            )
    return ntype


def _check_goto(node, label, id_set):
    """校验节点上所有 goto 引用（显式 goto / choice 选项 / branch case / dice 三向）。"""
    targets = []
    if "goto" in node:
        if not isinstance(node["goto"], str):
            raise LomcError('%s: 字段 "goto" 必须是节点 id 字符串' % label)
        targets.append(node["goto"])
    for opt in node.get("options", []):
        if isinstance(opt, dict):
            for key in ("goto",) + _DICE_OPTION_GOTOS:
                val = opt.get(key)
                if not isinstance(val, str):
                    continue
                if val == "" and key in _DICE_OPTION_GOTOS:
                    continue  # dice 的 goto_大成功 允许空（2 带检查点回退 goto_成功）
                targets.append(val)
    for case in node.get("cases", []):
        if isinstance(case, dict) and isinstance(case.get("goto"), str):
            targets.append(case["goto"])
    if node.get("type") in ("combat", "battle", "battle_result"):
        targets.extend((node["win"], node["lose"]))
    for t in targets:
        if t not in id_set:
            raise LomcError('%s: goto 指向不存在的节点 "%s"' % (label, t))


def _check_options(node, label, min_n, max_n, fields, type_map, optional_fields=()):
    """choice / dice 的 options 数组结构校验（共用）。"""
    options = node["options"]
    ntype = node["type"]
    if not min_n <= len(options) <= max_n:
        raise LomcError(
            "%s(%s): 选项数必须在 %d~%d 之间，实际为 %d"
            % (label, ntype, min_n, max_n, len(options))
        )
    for i, opt in enumerate(options):
        opt_label = "%s(%s) 第 %d 个选项" % (label, ntype, i + 1)
        if not isinstance(opt, dict):
            raise LomcError("%s: 必须是对象" % opt_label)
        allowed = set(fields) | set(optional_fields)
        for key in opt:
            if key not in allowed:
                raise LomcError(
                    '%s: 未知字段 "%s"（允许：%s）'
                    % (opt_label, key, "、".join(sorted(allowed)))
                )
        for name in fields:
            if name not in opt:
                raise LomcError('%s: 缺少必填字段 "%s"' % (opt_label, name))
            tag = type_map[name]
            if not _check_type(tag, opt[name]):
                raise LomcError(
                    '%s: 字段 "%s" 必须是%s，实际为 %r'
                    % (opt_label, name, _TYPE_NAMES[tag], opt[name])
                )


def _check_audio_name(name, label, ntype):
    """官方音频名保持原样；user: 引用必须是合法内容 ID（存在性由 pack/preflight 查）。"""
    if is_user_ref(name):
        parse_content_ref(name, label='%s(%s) 的 name' % (label, ntype))


def _check_asset_image_path(image, label, ntype):
    if not isinstance(image, str) or not image.strip():
        raise LomcError(
            '%s(%s): 字段 "image" 必须是非空字符串（包内图片路径），实际为 %r'
            % (label, ntype, image)
        )
    normalized = image.replace("\\", "/")
    if not image.lower().endswith((".png", ".jpg", ".jpeg")):
        raise LomcError(
            '%s(%s): 字段 "image" 必须是包内 assets/ 下的 '
            ".png/.jpg/.jpeg 图片，实际为 %r" % (label, ntype, image)
        )
    if (
        not normalized.startswith("assets/")
        or normalized == "assets/"
        or ".." in normalized.split("/")
    ):
        raise LomcError(
            '%s(%s): 字段 "image" 必须是包内 assets/ 相对路径'
            "（不得指向包外），实际为 %r" % (label, ntype, image)
        )


def _check_node_extra(node, ntype, label, battle_presets=None):
    """各节点类型的跨字段 / 结构性规则。"""
    character = node.get("character")
    character_is_used = not (
        ntype == "intro" and node.get("intro_source", "official") == "custom"
    )
    if (
        character_is_used
        and isinstance(character, str)
        and CHARACTER_DISPLAY_RE.fullmatch(character.strip())
    ):
        raise LomcError(
            "%s(%s): 人物必须保存内部 ID，不能使用下拉显示文字 %r；"
            "请重新选择该人物（例如 chicken1）" % (label, ntype, character)
        )
    # 角色表情校验（练武场卡死修复）：show/say 的 (character, portrait) 组合必须
    # 落在 editor_data 角色表情表内（表不可用/角色不在表 → 放行）。
    # 自定义 user: 角色不进官方表：只校验引用格式与表情 id；文件/表情是否存在
    # 由打包与预检对照 content.json 检查。
    if ntype in ("show", "say"):
        character = node.get("character")
        portrait = node.get("portrait")
        if (
            isinstance(character, str)
            and character
            and isinstance(portrait, str)
            and portrait
        ):
            if is_user_ref(character):
                parse_content_ref(
                    character, label='%s(%s) 的 character' % (label, ntype)
                )
                try:
                    validate_portrait_id(portrait, label='%s(%s) 的 portrait' % (label, ntype))
                except LomcError:
                    raise
            else:
                msg = check_portrait(load_portrait_table(), character, portrait)
                if msg:
                    raise LomcError("%s(%s): %s" % (label, ntype, msg))
    if ntype in UNSUPPORTED_USER_CHAR_TYPES and is_user_ref(node.get("character")):
        raise LomcError(
            "%s(%s): 自定义角色暂不支持该步骤，请改用官方角色，"
            "或用纯演出节点。" % (label, ntype)
        )
    if (
        isinstance(node.get("character"), str)
        and is_user_ref(node.get("character"))
        and ntype
        in (
            "show",
            "say",
            "hide",
            "move",
            "face",
            "focus",
            "offset",
            "shock",
            "dim",
            "rotate",
        )
    ):
        parse_content_ref(
            node.get("character"), label='%s(%s) 的 character' % (label, ntype)
        )
    if ntype == "background":
        action = node["action"]
        image = node.get("image")
        fade = node.get("fade", 0)
        if fade < 0:
            raise LomcError('%s(background): 字段 "fade" 不能小于 0' % label)
        if action in ("set", "show", "replace", "fadein"):
            if not isinstance(image, str) or not image.strip():
                raise LomcError(
                    '%s(background): action="%s" 时必填字段 "image"' % (label, action)
                )
            if not is_user_ref(image):
                raise LomcError(
                    '%s(background): 字段 "image" 必须是 user: 图片引用，实际为 %r'
                    % (label, image)
                )
            parse_content_ref(image, label='%s(background) 的 image' % label)
        elif image not in (None, ""):
            raise LomcError(
                '%s(background): action="%s" 时不能填写 "image"' % (label, action)
            )
    elif ntype == "custom_cg":
        action = node["action"]
        image = node.get("image")
        fade = node.get("fade", 0.5)
        scale = node.get("scale", 100)
        x = node.get("x", 0)
        y = node.get("y", 0)
        if fade < 0:
            raise LomcError('%s(custom_cg): 字段 "fade" 不能小于 0' % label)
        if scale < 10 or scale > 300:
            raise LomcError('%s(custom_cg): 字段 "scale" 必须在 10~300 之间' % label)
        if x < -100 or x > 100 or y < -100 or y > 100:
            raise LomcError('%s(custom_cg): 字段 "x/y" 必须在 -100~100 之间' % label)
        if action == "show":
            if not isinstance(image, str) or not image.strip():
                raise LomcError('%s(custom_cg): action="show" 时必填字段 "image"' % label)
            if not is_user_ref(image):
                raise LomcError(
                    '%s(custom_cg): 字段 "image" 必须是 user: 图片引用，实际为 %r'
                    % (label, image)
                )
            parse_content_ref(image, label='%s(custom_cg) 的 image' % label)
    elif ntype == "overlay":
        action = node["action"]
        slot = node["slot"]
        image = node.get("image")
        fade = node.get("fade", 0.25)
        scale = node.get("scale", 100)
        opacity = node.get("opacity", 100)
        if not slot.strip() or SCRIPT_ID_RE.fullmatch(slot) is None:
            raise LomcError(
                '%s(overlay): 字段 "slot" 必须是 [A-Za-z0-9_-]+ 的非空槽位 id' % label
            )
        if fade < 0:
            raise LomcError('%s(overlay): 字段 "fade" 不能小于 0' % label)
        if scale < 10 or scale > 300:
            raise LomcError('%s(overlay): 字段 "scale" 必须在 10~300 之间' % label)
        if opacity < 0 or opacity > 100:
            raise LomcError('%s(overlay): 字段 "opacity" 必须在 0~100 之间' % label)
        if action == "show":
            if not isinstance(image, str) or not image.strip():
                raise LomcError('%s(overlay): action="show" 时必填字段 "image"' % label)
            if not is_user_ref(image):
                raise LomcError(
                    '%s(overlay): 字段 "image" 必须是 user: 图片引用，实际为 %r'
                    % (label, image)
                )
            parse_content_ref(image, label='%s(overlay) 的 image' % label)
    elif ntype == "say":
        mode = node.get("mode", "character")
        if mode in ("character", "think") and "character" not in node:
            raise LomcError(
                '%s(say): mode="%s" 时必填字段 "character"（narrative/center 可省略）'
                % (label, mode)
            )
        if "voice" in node:
            voice = node["voice"]
            if not isinstance(voice, str) or not voice.strip():
                raise LomcError(
                    '%s(say): 字段 "voice" 必须是非空的用户音频引用（如 user:mohui.line_01），'
                    "不要语音请删除该字段" % label
                )
            if not is_user_ref(voice):
                raise LomcError(
                    '%s(say): 字段 "voice" 必须是用户内容引用（以 user: 开头），'
                    "不能用手填官方音效名。实际为 %r" % (label, voice)
                )
            parse_content_ref(voice, label='%s(say) 的 voice' % label)
    elif ntype == "intro":
        source = node.get("intro_source", "official")
        if source == "official":
            if not isinstance(node.get("character"), str) or not node["character"].strip():
                raise LomcError(
                    '%s(intro): 使用原版人物资料时必填字段 "character"' % label
                )
        elif source == "character":
            raw = node.get("character")
            if not isinstance(raw, str) or not raw.strip():
                raise LomcError(
                    '%s(intro): 使用自定义角色介绍卡时必填字段 "character"' % label
                )
            if not is_user_ref(raw):
                raise LomcError(
                    '%s(intro): 自定义角色介绍卡必须是 user: 引用，实际为 %r' % (label, raw)
                )
            parse_content_ref(raw, label='%s(intro) 的 character' % label)
        else:
            if not isinstance(node.get("name"), str) or not node["name"].strip():
                raise LomcError(
                    '%s(intro): 使用自定义人物资料时“人物姓名”不能为空' % label
                )
            if not isinstance(node.get("text"), str) or not node["text"].strip():
                raise LomcError(
                    '%s(intro): 使用自定义人物资料时“人物介绍”不能为空' % label
                )
            if node.get("image"):
                _check_asset_image_path(node["image"], label, ntype)
            image_scale = node.get("image_scale", 100)
            if image_scale < 40 or image_scale > 160:
                raise LomcError(
                    '%s(intro): 字段 "image_scale" 必须在 40~160 之间' % label
                )
            for field in ("image_x", "image_y"):
                value = node.get(field, 0)
                if value < -30 or value > 30:
                    raise LomcError(
                        '%s(intro): 字段 "%s" 必须在 -30~30 之间'
                        % (label, field)
                    )
    elif ntype == "choice":
        _check_options(
            node, label, 2, 4, ("text", "goto"), {"text": "str", "goto": "str"}
        )
        dialog = node.get("dialog", "Options")
        if dialog != "Options":
            raise LomcError(
                '%s(choice): dialog 只支持 "Options"。"%s" 是自由场景的 break '
                "格式菜单（选项文本为 类型+key+行动点+贡献 四段 + 分隔），"
                "纯文本选项会触发 BreakOptionButton 解析崩溃（IndexOutOfRange，"
                "菜单冻结无法点击）。" % (label, dialog)
            )
    elif ntype == "dice":
        _check_options(
            node,
            label,
            1,
            1,
            _DICE_OPTION_GOTOS,
            {key: "str" for key in _DICE_OPTION_GOTOS},
            optional_fields=("band_texts",),
        )
        check = node["check"]
        meta = get_dice_meta(check)
        if meta is None:
            raise LomcError(
                '%s(dice): 骰子检查点 "%s" 缺少官方元数据（骰子范围与结果带未知，'
                "游戏内骰子菜单会因此崩溃：选项条数不足时 UpdateSelection 索引越界 NRE，"
                "继续按钮永不出现）。请改用编辑器清单里带元数据的检查点，"
                "或运行 tools/extract_editor_data.py 重新提取数据。" % (label, check)
            )
        n_bands = len(meta.get("bands") or [])
        if not n_bands:
            raise LomcError(
                '%s(dice): 骰子检查点 "%s" 的官方元数据没有结果带（提取数据异常）'
                % (label, check)
            )
        opt = node["options"][0]
        band_texts = opt.get("band_texts")
        if band_texts is not None:
            if not isinstance(band_texts, list):
                raise LomcError(
                    '%s(dice): 可选字段 "band_texts" 必须是数组（逐带覆写选项文本，'
                    "条数等于结果带数），实际为 %r" % (label, band_texts)
                )
            if len(band_texts) != n_bands:
                raise LomcError(
                    '%s(dice): "band_texts" 条数必须等于检查点 "%s" 的结果带数'
                    "（%d 条），实际为 %d 条" % (label, check, n_bands, len(band_texts))
                )
            for bi, bt in enumerate(band_texts, 1):
                if not isinstance(bt, str) or not bt:
                    raise LomcError(
                        '%s(dice): "band_texts" 第 %d 条必须是非空字符串，实际为 %r'
                        % (label, bi, bt)
                    )
        if n_bands < 3 and not opt.get("goto_成功"):
            raise LomcError(
                '%s(dice): 检查点 "%s" 只有 %d 个结果带，必填字段 "goto_成功"（最优带）'
                % (label, check, n_bands)
            )
        if not opt.get("goto_失败"):
            raise LomcError(
                '%s(dice): 必填字段 "goto_失败"（结果带 1，最差带）' % label
            )
        if n_bands >= 3 and not opt.get("goto_大成功"):
            raise LomcError(
                '%s(dice): 检查点 "%s" 有 %d 个结果带，必填字段 "goto_大成功"（最优带）'
                % (label, check, n_bands)
            )
    elif ntype == "music":
        _check_audio_name(node.get("name"), label, ntype)
    elif ntype == "sound":
        _check_audio_name(node.get("name"), label, ntype)
        if node.get("op", "play") == "fadeout" and node.get("kind", "sound") != "env":
            raise LomcError(
                '%s(sound): op="fadeout" 仅支持 kind="env"（契约 §3.1）' % label
            )
    elif ntype == "message":
        # 系统提示文本必须非空（mainui.DisplayMessageText 显示原文）
        if not node["text"].strip():
            raise LomcError('%s(message): 字段 "text" 不能为空' % label)
    elif ntype == "rotate":
        # angle 契约要求整数（官方调用点均整数角度）；duration 可为小数（默认 1）
        angle = node["angle"]
        if isinstance(angle, bool) or not isinstance(angle, int):
            raise LomcError(
                '%s(rotate): 字段 "angle" 必须是整数（官方调用点均整数角度），实际为 %r'
                % (label, angle)
            )
        duration = node["duration"]
        if not _check_type("num", duration) or duration <= 0:
            raise LomcError(
                '%s(rotate): 字段 "duration" 必须是正数（秒），实际为 %r'
                % (label, duration)
            )
    elif ntype == "dayenv":
        # DayEnvironmentType 枚举实证（raw_scripts 调用点）：白天=1，晚上=2
        dtype = node["day_type"]
        if isinstance(dtype, bool) or not isinstance(dtype, int) or dtype not in (1, 2):
            raise LomcError(
                '%s(dayenv): 字段 "day_type" 必须是 1（白天）或 2（晚上）——官方 '
                "DayEnvironmentType 枚举仅两值（raw_scripts 的 "
                "SetGameDayEnvironment 调用点实证），实际为 %r" % (label, dtype)
            )
    elif ntype == "cg":
        action, kind = node["action"], node["kind"]
        if action == "show":
            # 各 kind 的必填参数（§3.1 + 官方脚本实证）
            need = {
                "picture": ("key",),
                "item": ("key",),
                "big": ("key",),
                "title": ("key",),
                "map": ("key", "key2"),
                "family": ("key", "key2", "n1", "n2"),
            }[kind]
            for name in need:
                if name not in node:
                    raise LomcError(
                        '%s(cg): action="show" kind="%s" 时必填字段 "%s"'
                        % (label, kind, name)
                    )
        elif kind == "title":
            raise LomcError(
                '%s(cg): action="hide" 不支持 kind="title"（官方无对应 API）' % label
            )
    elif ntype == "item":
        if node.get("remove", False) and node["kind"] == "special":
            raise LomcError(
                '%s(item): remove 仅支持 kind="book"/"misc"（special 无 Remove API）'
                % label
            )
    elif ntype == "enemy":
        if node["op"] != "id" and "value" not in node:
            raise LomcError(
                '%s(enemy): op="%s" 时必填字段 "value"（仅 op="id" 不需要）'
                % (label, node["op"])
            )
    elif ntype == "battle_skill":
        if node["op"] in ("set", "active") and "key" not in node:
            raise LomcError(
                '%s(battle_skill): op="%s" 时必填字段 "key"（仅 reset 不需要）'
                % (label, node["op"])
            )
    elif ntype == "battle_setup":
        if not node.get("enemy") and not node.get("reset_skills") and not node.get("skills"):
            raise LomcError(
                '%s(battle_setup): 至少需要 enemy、reset_skills=true 或一条 skills 配置'
                % label
            )
        if not node.get("enemy") and any(name in node for name in ("team", "level", "people")):
            raise LomcError(
                '%s(battle_setup): team/level/people 必须与 enemy 一起使用' % label
            )
        for name in ("team", "level", "people", "display"):
            if name in node and (isinstance(node[name], bool) or not isinstance(node[name], int)):
                raise LomcError('%s(battle_setup): 字段 "%s" 必须是整数' % (label, name))
        skills = node.get("skills", [])
        if len(skills) > 16:
            raise LomcError('%s(battle_setup): skills 最多 16 条' % label)
        for index, skill in enumerate(skills, 1):
            skill_label = '%s(battle_setup) 第 %d 条技能' % (label, index)
            if not isinstance(skill, dict):
                raise LomcError('%s: 必须是对象' % skill_label)
            unknown = set(skill) - {"key", "index", "active"}
            if unknown:
                raise LomcError('%s: 未知字段 %s' % (skill_label, "、".join(sorted(unknown))))
            if not _check_type("idstr", skill.get("key")):
                raise LomcError('%s: key 必须是非空技能 id' % skill_label)
            slot = skill.get("index", 2)
            if isinstance(slot, bool) or not isinstance(slot, int):
                raise LomcError('%s: index 必须是整数' % skill_label)
            active = skill.get("active", 1)
            if isinstance(active, bool) or active not in (0, 1):
                raise LomcError('%s: active 必须是 0 或 1' % skill_label)
    elif ntype == "combat":
        direct = bool(node.get("key"))
        preset_id = node.get("preset")
        if direct == bool(preset_id):
            raise LomcError(
                '%s(combat): 必须且只能填写 "key" 或 "preset" 其中一个' % label
            )
        if preset_id:
            preset = (battle_presets or {}).get(preset_id)
            if preset is None:
                raise LomcError(
                    '%s(combat): preset 指向不存在的战斗预设 "%s"'
                    % (label, preset_id)
                )
            if preset["kind"] != "combat":
                raise LomcError(
                    '%s(combat): 预设 "%s" 的 kind=%r，不是 combat'
                    % (label, preset_id, preset["kind"])
                )
            mixed = set(node) & {"enemy", "team", "level", "people", "display"}
            if mixed:
                raise LomcError(
                    '%s(combat): 使用 preset 时不能再填写 %s；请在预设中统一配置'
                    % (label, "、".join(sorted(mixed)))
                )
        for name in ("team", "level", "people", "display"):
            if name in node and (isinstance(node[name], bool) or not isinstance(node[name], int)):
                raise LomcError(
                    '%s(combat): 字段 "%s" 必须是整数，实际为 %r'
                    % (label, name, node[name])
                )
    elif ntype == "battle":
        direct = bool(node.get("key"))
        preset_id = node.get("preset")
        if direct == bool(preset_id):
            raise LomcError(
                '%s(battle): 必须且只能填写 "key" 或 "preset" 其中一个' % label
            )
        if preset_id:
            preset = (battle_presets or {}).get(preset_id)
            if preset is None:
                raise LomcError(
                    '%s(battle): preset 指向不存在的战斗预设 "%s"'
                    % (label, preset_id)
                )
            if preset["kind"] != "battle":
                raise LomcError(
                    '%s(battle): 预设 "%s" 的 kind=%r，不是 battle'
                    % (label, preset_id, preset["kind"])
                )
    elif ntype == "reward":
        entries = node["entries"]
        if not 1 <= len(entries) <= 32:
            raise LomcError('%s(reward): entries 必须有 1~32 条' % label)
        for index, entry in enumerate(entries, 1):
            entry_label = '%s(reward) 第 %d 条奖励' % (label, index)
            if not isinstance(entry, dict):
                raise LomcError('%s: 必须是对象' % entry_label)
            kind = entry.get("kind")
            if kind not in _ENUMS["reward_kind"]:
                raise LomcError('%s: kind 必须是 stat/affinity/talent/item/flag' % entry_label)
            allowed = {"kind", "key"}
            if kind != "flag":
                allowed.add("amount")
            if kind == "item":
                allowed.add("category")
            unknown = set(entry) - allowed
            if unknown:
                raise LomcError('%s: 未知字段 %s' % (entry_label, "、".join(sorted(unknown))))
            if not _check_type("idstr", entry.get("key")):
                raise LomcError('%s: key 必须是非空 id' % entry_label)
            if kind == "flag":
                continue
            amount = entry.get("amount")
            if not _check_type("num", amount):
                raise LomcError('%s: amount 必须是有限数值' % entry_label)
            if kind == "talent" and amount not in (-1, 1):
                raise LomcError('%s: talent amount 只能是 1 或 -1' % entry_label)
            if kind == "item":
                if isinstance(amount, bool) or not isinstance(amount, int) or amount <= 0:
                    raise LomcError('%s: item amount 必须是正整数' % entry_label)
                if entry.get("category") not in _ENUMS["item_kind"]:
                    raise LomcError('%s: item category 必须是 book/misc/special' % entry_label)
    elif ntype == "custom_shop":
        items = node["items"]
        if not 1 <= len(items) <= 64:
            raise LomcError('%s(custom_shop): items 必须有 1~64 条' % label)
        discount = node.get("discount", 0)
        if isinstance(discount, bool) or not isinstance(discount, int) or discount not in (0, 1):
            raise LomcError(
                '%s(custom_shop): discount 只能是 0（原价）或 1（原版统一折扣）' % label
            )
        seen = set()
        for index, item in enumerate(items, 1):
            item_label = '%s(custom_shop) 第 %d 件商品' % (label, index)
            if not isinstance(item, dict):
                raise LomcError('%s: 必须是对象' % item_label)
            unknown = set(item) - {"category", "item", "count", "condition"}
            if unknown:
                raise LomcError(
                    '%s: 未知字段 %s；原版没有公开的逐商品自定义价格接口'
                    % (item_label, "、".join(sorted(unknown)))
                )
            category = item.get("category")
            item_id = item.get("item")
            if category not in _ENUMS["shop_item_kind"]:
                raise LomcError('%s: category 必须是 book/misc/special' % item_label)
            if not _check_type("idstr", item_id):
                raise LomcError('%s: item 必须是非空原版物品 id' % item_label)
            count = item.get("count", 1)
            if isinstance(count, bool) or not isinstance(count, int) or not 1 <= count <= 9999:
                raise LomcError('%s: count 必须是 1~9999 的整数' % item_label)
            identity = (category, item_id)
            if identity in seen:
                raise LomcError('%s: 同一类别和 item id 不能重复' % item_label)
            seen.add(identity)
            condition = item.get("condition")
            if condition is None:
                continue
            if not isinstance(condition, dict):
                raise LomcError('%s: condition 必须是对象' % item_label)
            condition_unknown = set(condition) - {"source", "key", "invert"}
            if condition_unknown:
                raise LomcError(
                    '%s: condition 未知字段 %s'
                    % (item_label, "、".join(sorted(condition_unknown)))
                )
            if condition.get("source") not in _ENUMS["shop_condition_source"]:
                raise LomcError('%s: condition.source 必须是 mod/condition' % item_label)
            if not _check_type("idstr", condition.get("key")):
                raise LomcError('%s: condition.key 必须是非空 id' % item_label)
            if "invert" in condition and not isinstance(condition["invert"], bool):
                raise LomcError('%s: condition.invert 必须是布尔值' % item_label)
    elif ntype == "time":
        op = node["op"]
        if op == "set":
            need = ("year", "month", "stage")
        elif op == "mission":
            need = ("name", "year", "month", "stage")
        else:
            need = ()
        for name in need:
            if name not in node:
                raise LomcError('%s(time): op="%s" 时必填字段 "%s"' % (label, op, name))
    elif ntype == "panel":
        if node["panel"] in ("cg", "cgvideo", "endgame") and "key" not in node:
            raise LomcError(
                '%s(panel): panel="%s" 时必填字段 "key"' % (label, node["panel"])
            )
    elif ntype == "goto_scene":
        # 自造 GameOver id 在官方 LibrarySystem 中没有条目；若又不提供文本，
        # GameOverController 会把标题/描述清空，最终得到“死亡但没有文字”的空画面。
        # 直接报错阻止再次打出这种包，优先建议使用语义更明确的 death 节点。
        if node["scene"] == "GameOver":
            key = str(node.get("key") or "")
            is_mod_key = key.isdigit() and int(key) >= 900000
            has_text = any(
                isinstance(node.get(name), str) and node[name].strip()
                for name in ("title", "desc")
            )
            if is_mod_key and not has_text:
                raise LomcError(
                    '%s(goto_scene): scene="GameOver" 使用 mod 专属 key="%s" 时必须提供 '
                    'title/desc，否则官方死亡画面没有文字；建议改用 death 节点'
                    % (label, key)
                )
        if node["scene"] == "End":
            key = str(node.get("key") or "")
            is_mod_key = key.isdigit() and int(key) >= 900000
            has_content = any(
                isinstance(node.get(name), str) and node[name].strip()
                for name in ("title", "desc", "image")
            )
            if (not key or is_mod_key) and not has_content:
                raise LomcError(
                    '%s(goto_scene): scene="End" 使用空 key 或 mod 专属 key="%s" 时必须提供 '
                    'title/desc/image，否则汗青书结局卡没有内容'
                    % (label, key)
                )
        # 汗青书左页插图（契约 §3.1）：仅 End 支持，包内 assets/ 图片相对路径
        if "image" in node:
            image = node["image"]
            if not isinstance(image, str) or not image.strip():
                raise LomcError(
                    '%s(goto_scene): 字段 "image" 必须是非空字符串（包内图片路径），'
                    "实际为 %r" % (label, image)
                )
            if node["scene"] != "End":
                raise LomcError(
                    '%s(goto_scene): 字段 "image" 仅 scene="End" 支持（结局卡背景图），'
                    "scene=%r 不支持（GameOver 死亡画面暂不支持自定义图）"
                    % (label, node["scene"])
                )
            if not image.lower().endswith((".png", ".jpg", ".jpeg")):
                raise LomcError(
                    '%s(goto_scene): 字段 "image" 必须是包内 assets/ 下的 '
                    ".png/.jpg/.jpeg 图片（如 assets/ending.png），实际为 %r"
                    % (label, image)
                )
            if (
                not image.startswith("assets/")
                or image == "assets/"
                or ".." in image.replace("\\", "/").split("/")
            ):
                raise LomcError(
                    '%s(goto_scene): 字段 "image" 必须是包内 assets/ 相对路径'
                    "（不得指向包外），实际为 %r" % (label, image)
                )
    elif ntype == "block":
        for i, var in enumerate(node.get("vars", [])):
            var_label = "%s(block) 第 %d 个 var" % (label, i + 1)
            if not isinstance(var, dict):
                raise LomcError("%s: 必须是对象" % var_label)
            for key in var:
                if key not in ("name", "value"):
                    raise LomcError(
                        '%s: 未知字段 "%s"（允许：name、value）' % (var_label, key)
                    )
            if not isinstance(var.get("name"), str) or not var["name"]:
                raise LomcError('%s: 缺少必填字段 "name"（非空字符串）' % var_label)
            v = var.get("value")
            if not (isinstance(v, str) or _check_type("num", v)):
                raise LomcError(
                    '%s: 字段 "value" 必须是字符串或数值，实际为 %r' % (var_label, v)
                )
    elif ntype == "raw":
        if not node["code"].strip():
            raise LomcError('%s(raw): 字段 "code" 不能为空' % label)
    elif ntype == "death":
        # 死亡文本必须是非空文本（契约 §3.1：text 必填非空，多行合法）
        if not node["text"].strip():
            raise LomcError('%s(death): 字段 "text" 不能为空' % label)
        # death_id 必须是 ≥900000 的 mod 专属 id：官方 GameOver 场景会拿 id 查
        # LibrarySystem 并解锁/记录官方结局，复用官方 id 会污染玩家存档
        did = node["death_id"]
        try:
            ok = did.isdigit() and int(did) >= 900000
        except ValueError:  # 超长数字串 int() 会抛（防御）
            ok = False
        if not ok:
            raise LomcError(
                '%s(death): 字段 "death_id" 必须是 ≥900000 的 mod 专属数字 id'
                "（官方死亡画面 id 会触发官方结局解锁与记录，见 editor_data "
                "death_ids 仅作参考），实际为 %r" % (label, did)
            )
    elif ntype == "branch":
        source = node.get("source", "mod")
        cases = node["cases"]
        if len(cases) < 1:
            raise LomcError("%s(branch): cases 至少需要 1 个分支" % label)
        # 各来源的必填键字段：mod/game/flag_value/condition 用 flag，stat 用 stat
        if source == "stat":
            if not isinstance(node.get("stat"), str) or not node["stat"]:
                raise LomcError(
                    '%s(branch): source="stat" 时必填字段 "stat"'
                    "（editor_data stats 属性 id，如 mental/people/contribution）"
                    % label
                )
            if "flag" in node:
                raise LomcError(
                    '%s(branch): source="stat" 时不支持字段 "flag"（改用 "stat"）'
                    % label
                )
        else:
            if not isinstance(node.get("flag"), str) or not node["flag"]:
                raise LomcError(
                    '%s(branch): source="%s" 时必填字段 "flag"' % (label, source)
                )
            if "stat" in node:
                raise LomcError(
                    '%s(branch): source="%s" 时不支持字段 "stat"' % (label, source)
                )
        seen = set()
        for i, case in enumerate(cases):
            case_label = "%s(branch) 第 %d 个 case" % (label, i + 1)
            if not isinstance(case, dict):
                raise LomcError("%s: 必须是对象" % case_label)
            # 各来源的 case 字段：stat/flag_value 带 op；mod/condition 只带 value
            allowed = (
                ("op", "value", "goto")
                if source in ("stat", "flag_value")
                else ("value", "goto")
            )
            for key in case:
                if key not in allowed:
                    raise LomcError(
                        '%s: 未知字段 "%s"（允许：%s）'
                        % (case_label, key, "、".join(allowed))
                    )
            value = case.get("value")
            if not isinstance(value, int) or isinstance(value, bool):
                raise LomcError(
                    '%s: 字段 "value" 必须是整数，实际为 %r' % (case_label, value)
                )
            if source in ("mod", "condition") and value not in (1, 2):
                what = "已设置）或 2（未设置" if source == "mod" else "真）或 2（假"
                raise LomcError(
                    '%s: source="%s" 时 value 只能是 1（%s），实际为 %d'
                    % (case_label, source, what, value)
                )
            op = ""
            if source in ("stat", "flag_value"):
                op = case.get("op", ">=")
                if op not in BRANCH_OPS:
                    raise LomcError(
                        '%s: 字段 "op" 必须是 %s 之一（缺省 ">="），实际为 %r'
                        % (case_label, "、".join('"%s"' % o for o in BRANCH_OPS), op)
                    )
            key_id = (op, value)
            if key_id in seen:
                raise LomcError(
                    "%s: %s 与其他 case 重复"
                    % (
                        case_label,
                        "op=%s value=%d" % (op, value) if op else "value=%d" % value,
                    )
                )
            seen.add(key_id)
            if not isinstance(case.get("goto"), str):
                raise LomcError(
                    '%s: 缺少必填字段 "goto"（节点 id 字符串）' % case_label
                )


def validate_story(story, source="story.json", warnings=None):
    """校验一个 story.json（已解析为 dict）。不通过则抛 LomcError。

    source 仅用于报错前缀，方便 CLI 指出是哪个文件。
    warnings：可选的 list，传入时把非致命问题（编译仍会成功）追加进去，
    每条都是完整可读的中文句子（不含 source 前缀）。
    """
    try:
        _validate_story_inner(story)
        from .localization import validate_story_localization
        validate_story_localization(story)
    except LomcError as e:
        raise LomcError("%s: %s" % (source, e))
    if warnings is not None:
        _collect_warnings(story, warnings)


def _collect_warnings(story, warnings):
    """非致命问题收集（不中断编译）。"""
    nodes = story.get("nodes", []) if isinstance(story, dict) else []

    def is_transition(n, phase=None):
        return (
            isinstance(n, dict)
            and n.get("type") == "transition"
            and (phase is None or n.get("phase") == phase)
        )

    for idx, n in enumerate(nodes):
        if not isinstance(n, dict) or n.get("type") != "transition":
            continue
        label = n.get("id", "?")
        if n.get("phase") == "in":
            # 黑幕必须被一个更靠后的 out 解除。黑幕跨脚本持续：SetNextScript+Init
            # 不重置场景，in 在结尾会让链式脚本也全程黑屏。
            lifted = any(is_transition(m, "out") for m in nodes[idx + 1 :])
            if not lifted:
                warnings.append(
                    '节点 "%s"(transition, phase=in) 之后没有 phase=out 解除：'
                    "TransitionIn 会隐藏剧情 UI 并盖满黑幕（官方脚本必须成对使用，"
                    "ch1_1 里相距仅十几行），黑幕将一直覆盖到脚本结尾（含链式脚本）。"
                    "请在其后补一个 phase=out 节点，或改用 scene 节点做转场。" % label
                )
        else:
            # out 之前没有 in：无黑幕可撤，是无效操作（官方用法永远是先 in 后 out）
            has_cover = any(is_transition(m, "in") for m in nodes[:idx])
            if not has_cover:
                warnings.append(
                    '节点 "%s"(transition, phase=out) 之前没有 phase=in：'
                    "无黑幕可撤（官方用法永远是先 in 后 out），该节点不会产生任何视觉"
                    "效果，可删除。" % label
                )

    # dice 节点：text/threshold 已废弃；2 带检查点无独立大成功档
    for n in nodes:
        if not isinstance(n, dict) or n.get("type") != "dice":
            continue
        label = n.get("id", "?")
        check = n.get("check", "")
        opt = (n.get("options") or [{}])[0]
        if not isinstance(opt, dict):
            continue
        if opt.get("text") or "threshold" in opt:
            warnings.append(
                '节点 "%s"(dice): text/threshold 字段已废弃（骰子范围与结果带'
                "文本、条件以官方检查点元数据为准），已忽略，可删除。" % label
            )
        meta = get_dice_meta(check) or {}
        if (
            len(meta.get("bands") or []) == 2
            and opt.get("goto_大成功")
            and opt.get("goto_大成功") != opt.get("goto_成功")
        ):
            warnings.append(
                '节点 "%s"(dice): 检查点 "%s" 只有 2 个结果带（无独立大成功档），'
                "goto_大成功 会被忽略（最优带按 goto_成功 分支）。" % (label, check)
            )

    # goto_scene：GameOver/End 用官方 id 会触发结局解锁与记录（污染存档）
    official_ids = get_official_scene_ids()
    for n in nodes:
        if not isinstance(n, dict) or n.get("type") != "goto_scene":
            continue
        scene = n.get("scene")
        key = str(n.get("key") or "")
        end_uses_custom_panel = scene == "End" and any(
            isinstance(n.get(name), str) and n[name].strip()
            for name in ("title", "desc", "image")
        )
        if (
            scene in ("GameOver", "End")
            and key
            and key in official_ids
            and not end_uses_custom_panel
        ):
            warnings.append(
                '节点 "%s"(goto_scene): scene="%s" 的 key="%s" 与官方结局 id '
                % (
                    n.get("id", "?"),
                    scene,
                    key,
                )
                + "重复，会触发官方结局解锁与记录（LibraryItemData.Add，污染玩家"
                "存档）；建议改用 ≥900000 的 mod 专属 id（查不到官方条目，仅展示"
                "对应画面，无副作用）。"
            )
        # End 结局卡片：给了 title 建议同时给 desc（运行时 mod_set_ending_text
        # 的 desc 缺省空串，官方结局画面描述区会空白）
        if scene == "End" and isinstance(n.get("title"), str) and n["title"]:
            if not isinstance(n.get("desc"), str) or not n["desc"].strip():
                warnings.append(
                    '节点 "%s"(goto_scene): scene="End" 给了 title 但未给 desc，'
                    "结局画面的描述区将显示空白；建议补一个非空 desc。"
                    % n.get("id", "?")
                )
        if scene == "End" and n.get("next") not in (None, "", "Title", "Story"):
            warnings.append(
                '节点 "%s"(goto_scene): 汗青书结局的 next=%r 不会生效；'
                "原版 EndGamePanel 确认后固定返回标题画面，编译器已按 Title 处理。"
                % (n.get("id", "?"), n.get("next"))
            )
        if scene == "GameOver" and n.get("next") not in (None, "", "Title"):
            warnings.append(
                '节点 "%s"(goto_scene): 死亡画面的 next=%r 不会生效；'
                "原版按钮固定为读档或标题画面，编译器已按 Title 处理。"
                % (n.get("id", "?"), n.get("next"))
            )

    for n in nodes:
        if not isinstance(n, dict) or n.get("type") != "death":
            continue
        if n.get("next") not in (None, "", "Title"):
            warnings.append(
                '节点 "%s"(death): next=%r 不会生效；原版 GameOverController '
                "只提供读档和返回标题按钮，编译器已按 Title 处理。"
                % (n.get("id", "?"), n.get("next"))
            )


def _validate_battle_presets(raw):
    """校验章节级战斗预设；运行时只接收这里已约束的原版模板参数。"""
    if not isinstance(raw, dict):
        raise LomcError('字段 "battle_presets" 必须是对象（预设 id -> 配置）')
    if len(raw) > 64:
        raise LomcError('字段 "battle_presets" 最多允许 64 个预设')
    validated = {}
    for preset_id, preset in raw.items():
        label = '战斗预设 "%s"' % preset_id
        if not isinstance(preset_id, str) or SCRIPT_ID_RE.fullmatch(preset_id) is None:
            raise LomcError(
                '战斗预设 id 必须符合 [a-zA-Z0-9_-]{1,64}，实际为 %r' % preset_id
            )
        if not isinstance(preset, dict):
            raise LomcError('%s: 配置必须是对象' % label)
        kind = preset.get("kind")
        if kind not in ("combat", "battle"):
            raise LomcError('%s: kind 必须是 "combat" 或 "battle"' % label)
        allowed = {"kind", "key"}
        if kind == "combat":
            allowed.update(("enemy", "team", "level", "people", "display"))
        unknown = set(preset) - allowed
        if unknown:
            raise LomcError('%s: 未知字段 %s' % (label, "、".join(sorted(unknown))))
        if not _check_type("idstr", preset.get("key")):
            raise LomcError('%s: key 必须是非空的原版场景 id' % label)
        if "enemy" in preset and not _check_type("idstr", preset["enemy"]):
            raise LomcError('%s: enemy 必须是非空字符串' % label)
        for name in ("team", "level", "people", "display"):
            if name in preset and (
                isinstance(preset[name], bool) or not isinstance(preset[name], int)
            ):
                raise LomcError('%s: %s 必须是整数' % (label, name))
        validated[preset_id] = preset
    return validated


def _validate_story_inner(story):
    if not isinstance(story, dict):
        raise LomcError("顶层必须是 JSON 对象")

    story_schema = story.get("story_schema")
    if "story_schema" in story and (
        story_schema != STORY_SCHEMA or isinstance(story_schema, bool)
    ):
        raise LomcError(
            '字段 "story_schema" 必须固定为 %d，实际为 %r'
            % (STORY_SCHEMA, story_schema)
        )

    sid = story.get("id")
    if not isinstance(sid, str) or SCRIPT_ID_RE.fullmatch(sid) is None:
        raise LomcError('缺少必填字段 "id"（剧情脚本 id，规则 [a-zA-Z0-9_-]{1,64}）')
    if "title" in story and not isinstance(story["title"], str):
        raise LomcError('字段 "title" 必须是字符串')
    # 顶层可选字段 mood（bool）：false=每次 show/say 前后隐藏官方心情气泡
    if "mood" in story and not isinstance(story["mood"], bool):
        raise LomcError(
            '字段 "mood" 必须是布尔值（true=保留官方心情气泡，false=自动隐藏）'
        )
    start = story.get("start")
    if not isinstance(start, str):
        raise LomcError('缺少必填字段 "start"（起始节点 id）')
    nodes = story.get("nodes")
    if not isinstance(nodes, list) or not nodes:
        raise LomcError('缺少必填字段 "nodes"（非空节点数组）')

    battle_presets = _validate_battle_presets(story.get("battle_presets", {}))

    # 第一遍：节点 id 唯一性与基本结构
    id_set = set()
    for i, node in enumerate(nodes):
        label = _node_label(node if isinstance(node, dict) else {}, i)
        if not isinstance(node, dict):
            raise LomcError("%s: 节点必须是 JSON 对象" % label)
        nid = node.get("id")
        if not isinstance(nid, str) or NODE_ID_RE.fullmatch(nid) is None:
            raise LomcError(
                "%s: 节点 id 必须是 [a-zA-Z0-9_]+（会拼进 Lua 函数名 node_<id>），实际为 %r"
                % (label, nid)
            )
        if nid in id_set:
            raise LomcError('节点 id "%s" 重复' % nid)
        id_set.add(nid)

    if start not in id_set:
        raise LomcError('start 指向不存在的节点 "%s"' % start)

    # 第二遍：字段、goto、跨字段规则、收尾与兜底
    for i, node in enumerate(nodes):
        label = _node_label(node, i)
        ntype = _check_node_fields(node, label)
        _check_node_extra(node, ntype, label, battle_presets)
        _check_goto(node, label, id_set)
        if ntype in _NO_GOTO_TYPES and "goto" in node:
            raise LomcError(
                '%s(%s): 该类型节点不允许显式 "goto"（流转由分支/跳转决定）'
                % (label, ntype)
            )
        is_last = i == len(nodes) - 1
        # 契约 §4：最后一个节点不是 end/goto_scene/raw（或 choice/branch/dice
        # 这类自带全分支回转的结构）且无 goto → 校验错误
        if ntype not in _TERMINAL_TYPES and "goto" not in node and is_last:
            raise LomcError(
                "%s(%s): 是最后一个节点且没有显式 goto，脚本无法正常结束"
                "（请改用 end/goto_scene/raw 节点或显式 goto）" % (label, ntype)
            )
        # 分支兜底（契约 §4）：branch 未命中任何 case 时 else 落顺序下一节点；
        # branch 为最后一个节点且存在未覆盖返回值 → else 无落点，报错
        if ntype == "branch" and is_last:
            source = node.get("source", "mod")
            if source in ("mod", "condition"):
                # 两路都覆盖时无 else 需要
                covered = {c["value"] for c in node["cases"]} == {1, 2}
            else:
                # game：Switch 可返回任意整数（含查不到的 0）；
                # stat/flag_value：属性/旗标数值无上限，永远有未覆盖值
                covered = False
            if not covered:
                raise LomcError(
                    "%s(branch): 是最后一个节点且存在未覆盖的返回值，else 没有落点"
                    "（请把它移到最后之前，或补齐 value 1 和 2 两个 case）" % label
                )


def validate_manifest(manifest, source="manifest.json"):
    """校验 manifest.json（§2）。不通过则抛 LomcError。"""
    try:
        if not isinstance(manifest, dict):
            raise LomcError("顶层必须是 JSON 对象")
        fmt = manifest.get("package_format", manifest.get("format"))
        if fmt != PACKAGE_FORMAT or isinstance(fmt, bool):
            raise LomcError(
                '字段 "package_format" 必须固定为 %d（包格式版本号）'
                % PACKAGE_FORMAT
            )
        if "package_format" in manifest and "format" in manifest:
            legacy_format = manifest.get("format")
            if legacy_format != fmt or isinstance(legacy_format, bool):
                raise LomcError('字段 "package_format" 与旧字段 "format" 的声明不一致')
        for field, current in (
            ("story_schema", STORY_SCHEMA),
            ("content_schema", CONTENT_SCHEMA),
        ):
            value = manifest.get(field)
            if field in manifest and (value != current or isinstance(value, bool)):
                raise LomcError(
                    '字段 "%s" 必须固定为 %d，实际为 %r'
                    % (field, current, value)
                )
        mid = manifest.get("id")
        if not isinstance(mid, str) or MOD_ID_RE.fullmatch(mid) is None:
            raise LomcError(
                '缺少必填字段 "id"（mod 唯一 id，规则 [a-z0-9_-]{1,64}），实际为 %r' % (mid,)
            )
        for name in ("name", "version", "author", "description"):
            _validate_manifest_display_text(name, manifest.get(name))
        _validate_compatibility_metadata(manifest)
        entry = manifest.get("entry")
        if not isinstance(entry, str) or SCRIPT_ID_RE.fullmatch(entry) is None:
            raise LomcError(
                '缺少必填字段 "entry"（入口剧情脚本 id），实际为 %r' % (entry,)
            )
        # campaign（可选，§2）：结构只做浅校验，运行时再解释
        campaign = manifest.get("campaign")
        if campaign is not None:
            if not isinstance(campaign, dict):
                raise LomcError('可选字段 "campaign" 必须是对象')
            if "new_game" in campaign and not isinstance(campaign["new_game"], bool):
                raise LomcError('字段 "campaign.new_game" 必须是布尔值')
            if "disable_official_events" in campaign and not isinstance(
                campaign["disable_official_events"], bool
            ):
                raise LomcError(
                    '字段 "campaign.disable_official_events" 必须是布尔值'
                    "（true=本战役禁用原版地图事件，仅 mod 自己的触发器生效）"
                )
            triggers = campaign.get("triggers", [])
            if not isinstance(triggers, list):
                raise LomcError('字段 "campaign.triggers" 必须是数组')
            for j, trig in enumerate(triggers):
                tlabel = "campaign.triggers 第 %d 项" % (j + 1)
                if not isinstance(trig, dict):
                    raise LomcError("%s: 必须是对象" % tlabel)
                if trig.get("type") != "position":
                    raise LomcError('%s: 字段 "type" 目前只支持 "position"' % tlabel)
                position = trig.get("position")
                if position not in CAMPAIGN_POSITIONS:
                    raise LomcError(
                        '%s: 字段 "position" 必须是合法位置 id（%s），实际为 %r'
                        % (tlabel, "/".join(CAMPAIGN_POSITIONS), position)
                    )
                script = trig.get("script")
                if not isinstance(script, str) or SCRIPT_ID_RE.fullmatch(script) is None:
                    raise LomcError(
                        '%s: 字段 "script" 必须是合法的同包脚本 id，实际为 %r'
                        % (tlabel, script)
                    )
                for cond in ("when_flag_set", "when_flag_clear"):
                    if cond in trig and not isinstance(trig[cond], str):
                        raise LomcError('%s: 字段 "%s" 必须是字符串' % (tlabel, cond))
                # 新可选条件（与 flag 条件并列 AND，数组顺序=优先级）：
                # when_month 1~12、when_stage 1~3、when_affinity {character, min}
                wm = trig.get("when_month")
                if wm is not None and (
                    not isinstance(wm, int) or isinstance(wm, bool) or not 1 <= wm <= 12
                ):
                    raise LomcError(
                        '%s: 字段 "when_month" 必须是 1~12 的整数，实际为 %r'
                        % (tlabel, wm)
                    )
                ws = trig.get("when_stage")
                if ws is not None and (
                    not isinstance(ws, int) or isinstance(ws, bool) or not 1 <= ws <= 3
                ):
                    raise LomcError(
                        '%s: 字段 "when_stage" 必须是 1~3 的整数（旬），实际为 %r'
                        % (tlabel, ws)
                    )
                wa = trig.get("when_affinity")
                if wa is not None:
                    if not isinstance(wa, dict):
                        raise LomcError(
                            '%s: 字段 "when_affinity" 必须是 {"character": <人物>, '
                            '"min": <数值>} 对象，实际为 %r' % (tlabel, wa)
                        )
                    for k in wa:
                        if k not in ("character", "min"):
                            raise LomcError(
                                '%s: when_affinity 含未知字段 "%s"'
                                "（允许：character、min）" % (tlabel, k)
                            )
                    wc = wa.get("character")
                    if not isinstance(wc, str) or not wc:
                        raise LomcError(
                            "%s: when_affinity.character 必须是非空字符串"
                            "（人物 id），实际为 %r" % (tlabel, wc)
                        )
                    wmin = wa.get("min")
                    if (
                        not isinstance(wmin, int)
                        or isinstance(wmin, bool)
                        or not -(2**31) <= wmin <= 2**31 - 1
                    ):
                        raise LomcError(
                            "%s: when_affinity.min 必须是 Int32 范围内的整数，实际为 %r"
                            % (tlabel, wmin)
                        )
                # 未知字段一律报错（防拼写错误静默失效）
                allowed = (
                    "type",
                    "position",
                    "script",
                    "when_flag_set",
                    "when_flag_clear",
                    "when_month",
                    "when_stage",
                    "when_affinity",
                )
                for k in trig:
                    if k not in allowed:
                        raise LomcError(
                            '%s: 未知字段 "%s"（允许：%s）'
                            % (tlabel, k, "、".join(allowed))
                        )
    except LomcError as e:
        raise LomcError("%s: %s" % (source, e))
