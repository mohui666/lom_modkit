# -*- coding: utf-8 -*-
"""story.json / manifest.json 校验。

严格遵循 docs/mod_format.md（v3 契约）：
- §2 manifest.json 字段
- §3 story/*.json 结构与 §3.1 全量 38 种节点类型表（字段除标注"可选"外均为必填）
- §4 补充规则（末节点收尾、禁止显式 goto 的类型、分支兜底等）

所有错误抛出 LomcError，消息带节点 id / 字段名。
"""

import re

from .errors import LomcError

# §1：剧情脚本 id 规则
SCRIPT_ID_RE = re.compile(r"^[a-zA-Z0-9_\-]+$")
# §2：mod id 规则
MOD_ID_RE = re.compile(r"^[a-z0-9_\-]+$")
# 节点 id 会拼进 Lua 函数名 node_<id>，必须落在 Lua 标识符安全字符集内
NODE_ID_RE = re.compile(r"^[a-zA-Z0-9_]+$")

SAY_MODES = ("character", "think", "narrative", "center")
FACINGS = ("left", "right")
BRANCH_SOURCES = ("mod", "game")

# 枚举字段的合法值（§3.1）
_ENUMS = {
    "music_op": ("play", "stop", "fadeout"),
    "sound_kind": ("sound", "env"),
    "sound_op": ("play", "fadeout"),
    "transition_phase": ("in", "out"),
    "transition_dir": ("lr", "rl", "tb", "bt"),
    "block_fc": ("view", "common"),
    "cg_action": ("show", "hide"),
    "cg_kind": ("picture", "item", "big", "map", "family", "title"),
    "item_kind": ("book", "misc", "special"),
    "gameflag_op": ("set", "add"),
    "enemy_op": ("team", "level", "people", "id"),
    "bskill_op": ("set", "active", "reset"),
    "time_op": ("set", "round", "month", "mission"),
    "autosave_kind": ("story", "free", "prologue"),
    "goto_scene": ("Free", "Title", "Combat", "Battle", "GameOver", "End", "Story", "DemoEnd"),
    "panel_kind": ("martial", "weapon", "poison", "cg", "cgvideo", "shop", "newshop", "credit", "endgame"),
    "branch_source": BRANCH_SOURCES,
    "mode": SAY_MODES,
    "facing": FACINGS,
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
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if tag == "bool":
        return isinstance(value, bool)
    if tag == "list":
        return isinstance(value, list)
    if tag == "talent_level":
        return value in (1, -1)
    if tag == "save_button":
        return value in (0, 1) and not isinstance(value, bool)
    if tag == "script_id":
        return isinstance(value, str) and SCRIPT_ID_RE.match(value) is not None
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
        {"character": "str", "portrait": "str", "mode": "mode"},
    ),
    "choice": ({"options": "list"}, {"dialog": "idstr"}),
    "shock": ({"character": "str"}, {"duration": "num"}),
    "mask": ({"show": "bool"}, {}),
    "intro": ({"character": "str"}, {}),
    "effect": (
        {"name": "str"},
        {"x": "num", "y": "num", "a": "num", "b": "num", "c": "num", "d": "num"},
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
    "mission": ({"name": "idstr", "key": "idstr"}, {}),
    "time": (
        {"op": "time_op"},
        {"year": "num", "month": "num", "stage": "num", "name": "idstr"},
    ),
    "autosave": ({}, {"kind": "autosave_kind", "save_button": "save_button"}),
    # ---- 流程类 ----
    "branch": ({"flag": "idstr", "cases": "list"}, {"source": "branch_source"}),
    "dice": ({"check": "idstr", "options": "list"}, {}),
    "goto_scene": (
        {"scene": "goto_scene"},
        {"key": "idstr", "next": "str"},
    ),
    "panel": (
        {"panel": "panel_kind"},
        {"key": "idstr", "discount": "num", "mode": "num"},
    ),
    "wait": ({"seconds": "num"}, {}),
    "end": ({}, {"next_script": "script_id"}),
    "raw": ({"code": "str"}, {}),
}

# 任何节点都允许的通用字段
_COMMON_FIELDS = ("id", "type", "goto")

# 不允许显式 goto 的节点类型（契约 §4：流转由自身结构/场景跳转决定）
_NO_GOTO_TYPES = ("choice", "branch", "dice", "end", "goto_scene")

# 可以作最后一个节点收尾的类型（其余类型在末位且无 goto → 校验错误）
_TERMINAL_TYPES = ("end", "choice", "branch", "dice", "goto_scene", "raw")

# dice 选项的字段（§3.1：每项 text/threshold + 三向 goto）
_DICE_OPTION_FIELDS = ("text", "threshold", "goto_大成功", "goto_成功", "goto_失败")
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
                if isinstance(opt.get(key), str):
                    targets.append(opt[key])
    for case in node.get("cases", []):
        if isinstance(case, dict) and isinstance(case.get("goto"), str):
            targets.append(case["goto"])
    for t in targets:
        if t not in id_set:
            raise LomcError('%s: goto 指向不存在的节点 "%s"' % (label, t))


def _check_options(node, label, min_n, max_n, fields, type_map):
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
        for key in opt:
            if key not in fields:
                raise LomcError(
                    '%s: 未知字段 "%s"（允许：%s）'
                    % (opt_label, key, "、".join(fields))
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


def _check_node_extra(node, ntype, label):
    """各节点类型的跨字段 / 结构性规则。"""
    if ntype == "say":
        mode = node.get("mode", "character")
        if mode in ("character", "think") and "character" not in node:
            raise LomcError(
                '%s(say): mode="%s" 时必填字段 "character"（narrative/center 可省略）'
                % (label, mode)
            )
    elif ntype == "choice":
        _check_options(
            node, label, 2, 4, ("text", "goto"), {"text": "str", "goto": "str"}
        )
    elif ntype == "dice":
        _check_options(
            node, label, 1, 4, _DICE_OPTION_FIELDS,
            {"text": "str", "threshold": "num", "goto_大成功": "str",
             "goto_成功": "str", "goto_失败": "str"},
        )
    elif ntype == "sound":
        if node.get("op", "play") == "fadeout" and node.get("kind", "sound") != "env":
            raise LomcError(
                '%s(sound): op="fadeout" 仅支持 kind="env"（契约 §3.1）' % label
            )
    elif ntype == "cg":
        action, kind = node["action"], node["kind"]
        if action == "show":
            # 各 kind 的必填参数（§3.1 + 官方脚本实证）
            need = {
                "picture": ("key",), "item": ("key",), "big": ("key",),
                "title": ("key",), "map": ("key", "key2"),
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
                raise LomcError(
                    '%s(time): op="%s" 时必填字段 "%s"' % (label, op, name)
                )
    elif ntype == "panel":
        if node["panel"] in ("cg", "cgvideo", "endgame") and "key" not in node:
            raise LomcError(
                '%s(panel): panel="%s" 时必填字段 "key"' % (label, node["panel"])
            )
    elif ntype == "block":
        for i, var in enumerate(node.get("vars", [])):
            var_label = '%s(block) 第 %d 个 var' % (label, i + 1)
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
    elif ntype == "branch":
        source = node.get("source", "mod")
        cases = node["cases"]
        if len(cases) < 1:
            raise LomcError("%s(branch): cases 至少需要 1 个分支" % label)
        seen = set()
        for i, case in enumerate(cases):
            case_label = "%s(branch) 第 %d 个 case" % (label, i + 1)
            if not isinstance(case, dict):
                raise LomcError("%s: 必须是对象" % case_label)
            for key in case:
                if key not in ("value", "goto"):
                    raise LomcError(
                        '%s: 未知字段 "%s"（允许：value、goto）' % (case_label, key)
                    )
            value = case.get("value")
            if not isinstance(value, int) or isinstance(value, bool):
                raise LomcError(
                    '%s: 字段 "value" 必须是整数，实际为 %r' % (case_label, value)
                )
            if source == "mod" and value not in (1, 2):
                raise LomcError(
                    '%s: source="mod" 时 value 只能是 1（已设置）或 2（未设置），实际为 %d'
                    % (case_label, value)
                )
            if value in seen:
                raise LomcError("%s: value=%d 与其他 case 重复" % (case_label, value))
            seen.add(value)
            if not isinstance(case.get("goto"), str):
                raise LomcError('%s: 缺少必填字段 "goto"（节点 id 字符串）' % case_label)


def validate_story(story, source="story.json"):
    """校验一个 story.json（已解析为 dict）。不通过则抛 LomcError。

    source 仅用于报错前缀，方便 CLI 指出是哪个文件。
    """
    try:
        _validate_story_inner(story)
    except LomcError as e:
        raise LomcError("%s: %s" % (source, e))


def _validate_story_inner(story):
    if not isinstance(story, dict):
        raise LomcError("顶层必须是 JSON 对象")

    sid = story.get("id")
    if not isinstance(sid, str) or not SCRIPT_ID_RE.match(sid):
        raise LomcError('缺少必填字段 "id"（剧情脚本 id，规则 [a-zA-Z0-9_-]+）')
    if "title" in story and not isinstance(story["title"], str):
        raise LomcError('字段 "title" 必须是字符串')
    start = story.get("start")
    if not isinstance(start, str):
        raise LomcError('缺少必填字段 "start"（起始节点 id）')
    nodes = story.get("nodes")
    if not isinstance(nodes, list) or not nodes:
        raise LomcError('缺少必填字段 "nodes"（非空节点数组）')

    # 第一遍：节点 id 唯一性与基本结构
    id_set = set()
    for i, node in enumerate(nodes):
        label = _node_label(node if isinstance(node, dict) else {}, i)
        if not isinstance(node, dict):
            raise LomcError("%s: 节点必须是 JSON 对象" % label)
        nid = node.get("id")
        if not isinstance(nid, str) or not NODE_ID_RE.match(nid):
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
        _check_node_extra(node, ntype, label)
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
            if node.get("source", "mod") == "mod":
                covered = {c["value"] for c in node["cases"]} == {1, 2}
            else:  # game：Switch 可返回任意整数（含查不到的 0），永远有未覆盖值
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
        fmt = manifest.get("format")
        if fmt != 1 or isinstance(fmt, bool):
            raise LomcError('字段 "format" 必须固定为 1（格式版本号）')
        mid = manifest.get("id")
        if not isinstance(mid, str) or not MOD_ID_RE.match(mid):
            raise LomcError(
                '缺少必填字段 "id"（mod 唯一 id，规则 [a-z0-9_-]+），实际为 %r' % (mid,)
            )
        for name in ("name", "version", "author", "description"):
            if not isinstance(manifest.get(name), str) or not manifest[name]:
                raise LomcError('缺少必填字段 "%s"（非空字符串）' % name)
        entry = manifest.get("entry")
        if not isinstance(entry, str) or not SCRIPT_ID_RE.match(entry):
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
            triggers = campaign.get("triggers", [])
            if not isinstance(triggers, list):
                raise LomcError('字段 "campaign.triggers" 必须是数组')
            for j, trig in enumerate(triggers):
                tlabel = "campaign.triggers 第 %d 项" % (j + 1)
                if not isinstance(trig, dict):
                    raise LomcError("%s: 必须是对象" % tlabel)
                if trig.get("type") != "position":
                    raise LomcError('%s: 字段 "type" 目前只支持 "position"' % tlabel)
                if not isinstance(trig.get("position"), str) or not trig["position"]:
                    raise LomcError('%s: 缺少必填字段 "position"' % tlabel)
                if not isinstance(trig.get("script"), str) or not trig["script"]:
                    raise LomcError('%s: 缺少必填字段 "script"（同包脚本 id）' % tlabel)
                for cond in ("when_flag_set", "when_flag_clear"):
                    if cond in trig and not isinstance(trig[cond], str):
                        raise LomcError('%s: 字段 "%s" 必须是字符串' % (tlabel, cond))
    except LomcError as e:
        raise LomcError("%s: %s" % (source, e))
