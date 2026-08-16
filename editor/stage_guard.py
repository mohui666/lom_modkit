# -*- coding: utf-8 -*-
"""人物登场防线：动作类节点要求目标人物已在台上（先 show 再动作）。

游戏侧对不在台上的人物执行 move/face/hide/rotate/say 等操作会因“角色不存在”
崩掉剧情协程而黑屏（preview.build_playtest_prelude 对同一坑有试玩侧处置）。
本模块提供编辑/写入侧的纯数据判断与修复，不依赖 Qt：

- required_character(node)：节点动作需要的台上人物 id（不需要则 None）
- missing_stage_linear(nodes, index)：线性回看，判断 index 处节点是否缺登场
- find_stage_issues(story)：图级 must-分析，列出“某条路径上未登场/已退场”的步骤
- ensure_stage(story, node_id)：在该步骤前插入人物登场并重定向入边
"""

from __future__ import annotations

import models
from story_graph import analyze_story

# 需要目标人物已在台上的节点类型（hide 同样要求：隐藏不存在的人物也会崩）。
ACTION_TYPES = ("move", "face", "hide", "focus", "offset", "shock", "dim", "rotate")

# 自动补登场节点的默认站位（中景中间位，最不容易被遮挡）。
_DEFAULT_POSITION = "M"


def required_character(node: dict) -> str | None:
    """节点动作需要的台上人物 id；该节点不依赖台上人物时返回 None。"""
    if not isinstance(node, dict):
        return None
    node_type = node.get("type")
    if node_type in ACTION_TYPES:
        cid = node.get("character")
        return cid if isinstance(cid, str) and cid else None
    if node_type == "say":
        mode = node.get("mode") or "character"
        if mode in ("character", "think"):
            cid = node.get("character")
            return cid if isinstance(cid, str) and cid else None
    return None


def _transfer(stage: set[str], node: dict) -> set[str]:
    """show 登场加入台上集合；hide 退场移出；其余不变。"""
    node_type = node.get("type")
    cid = node.get("character")
    if not isinstance(cid, str) or not cid:
        return stage
    if node_type == "show":
        return stage | {cid}
    if node_type == "hide":
        return stage - {cid}
    return stage


def missing_stage_linear(nodes: list, index: int) -> str | None:
    """线性回看 0..index-1：nodes[index] 的动作人物未登场或已退场时返回其 id。

    新建/更新节点时的本地防线：按列表顺序找离 index 最近的该人物 show/hide，
    是 show 则视为在台上，是 hide 或从未登场则需要补登场。
    """
    if not (0 <= index < len(nodes)):
        return None
    cid = required_character(nodes[index])
    if cid is None:
        return None
    for prev in reversed(nodes[:index]):
        if not isinstance(prev, dict) or prev.get("character") != cid:
            continue
        prev_type = prev.get("type")
        if prev_type == "show":
            return None
        if prev_type == "hide":
            return cid
    return cid


def _stage_ins(story: dict) -> tuple[dict[str, frozenset[str]], frozenset[str], list]:
    """图级 must-分析：每个节点入场时“确定在台上”的人物集合（所有路径的交集）。

    返回 (入场集合 by 节点 id, 可达节点集, 有效节点列表)。不可达节点不参与判断。
    """
    analysis = analyze_story(story)
    nodes = [
        n
        for n in story.get("nodes") or []
        if isinstance(n, dict) and isinstance(n.get("id"), str) and n.get("id")
    ]
    by_id = {n["id"]: n for n in nodes}
    universe = {
        n["character"] for n in nodes if isinstance(n.get("character"), str) and n["character"]
    }
    preds: dict[str, set[str]] = {nid: set() for nid in by_id}
    for edge in analysis.edges:
        if not edge.missing and edge.source in by_id and edge.target in by_id:
            preds[edge.target].add(edge.source)
    start = str(story.get("start") or "")
    # must-分析取最大不动点：除起点为空集外初始化为全集，迭代只会收缩
    ins: dict[str, set[str]] = {
        nid: (set() if nid == start else set(universe)) for nid in by_id
    }
    changed = True
    while changed:
        changed = False
        for nid in by_id:
            if nid == start or not preds[nid]:
                continue
            meet: set[str] | None = None
            for pred in preds[nid]:
                out = _transfer(ins[pred], by_id[pred])
                meet = out if meet is None else (meet & out)
            if meet is not None and meet != ins[nid]:
                ins[nid] = meet
                changed = True
    return (
        {nid: frozenset(s) for nid, s in ins.items()},
        analysis.reachable,
        nodes,
    )


def find_stage_issues(story: dict) -> list[tuple[str, str]]:
    """返回 [(node_id, character)]：可达、但某条路径上动作人物未登场/已退场。"""
    ins, reachable, nodes = _stage_ins(story)
    issues: list[tuple[str, str]] = []
    for node in nodes:
        nid = node["id"]
        if nid not in reachable:
            continue
        cid = required_character(node)
        if cid and cid not in ins.get(nid, frozenset()):
            issues.append((nid, cid))
    return issues


def make_show_node(story: dict, character: str) -> dict:
    """构造自动补的登场节点（中景中位、无表情覆盖，编译器按契约放行）。"""
    return {
        "id": models.make_node_id(story, "show"),
        "type": "show",
        "character": character,
        "position": _DEFAULT_POSITION,
    }


def ensure_stage(story: dict, node_id: str) -> dict | None:
    """在 node_id 前插入其动作人物的登场节点，并把指向 node_id 的入边改指新节点。

    列表前驱的顺序衔接（fallthrough）天然经过新节点；显式 goto/选项/分支
    跳转需要重定向，起始点同理。返回新插入的 show 节点；该节点不需要台上
    人物或不存在时返回 None。
    """
    nodes = story.get("nodes")
    if not isinstance(nodes, list):
        return None
    target = None
    index = -1
    for i, node in enumerate(nodes):
        if isinstance(node, dict) and node.get("id") == node_id:
            target, index = node, i
            break
    if target is None:
        return None
    cid = required_character(target)
    if cid is None:
        return None
    show = make_show_node(story, cid)
    nodes.insert(index, show)
    old, new = node_id, show["id"]
    for node in nodes:
        if not isinstance(node, dict) or node is show or node is target:
            continue  # 自身自环保持原样，避免每次循环都多过一次登场
        if node.get("goto") == old:
            node["goto"] = new
        for option in node.get("options") or []:
            if not isinstance(option, dict):
                continue
            for key in ("goto", "goto_大成功", "goto_成功", "goto_失败"):
                if option.get(key) == old:
                    option[key] = new
        for band in node.get("bands") or []:
            if isinstance(band, dict) and band.get("goto") == old:
                band["goto"] = new
        for case in node.get("cases") or []:
            if isinstance(case, dict) and case.get("goto") == old:
                case["goto"] = new
    if story.get("start") == old:
        story["start"] = new
    return show
