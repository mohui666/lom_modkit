# -*- coding: utf-8 -*-
"""剧情节点连线与可达性分析。

这里是流程图、发布前体检共用的纯数据层，不依赖 Qt。所有边都按编译器的
实际流转规则生成，避免“图上能走、游戏里走不到”的两套解释。
"""

from __future__ import annotations

from dataclasses import dataclass


CHECK_TYPES = {
    "stat_check", "affinity_check", "item_check", "talent_check", "flag_check", "activity",
    "quest_check",
}
TERMINAL_TYPES = {
    "end", "goto_scene", "death", "combat", "battle", "battle_result", *CHECK_TYPES,
}
NO_FALLTHROUGH_TYPES = {"choice", "dice", *TERMINAL_TYPES}


@dataclass(frozen=True)
class GraphEdge:
    source: str
    target: str
    label: str
    kind: str
    missing: bool = False


@dataclass(frozen=True)
class StoryGraphAnalysis:
    node_order: tuple[str, ...]
    edges: tuple[GraphEdge, ...]
    reachable: frozenset[str]
    unreachable: frozenset[str]
    dead_ends: frozenset[str]
    infinite_loops: frozenset[str]
    missing_targets: frozenset[str]


def _short_label(value: object, fallback: str) -> str:
    text = str(value or "").replace("\n", " ").strip()
    return (text[:16] + "…") if len(text) > 16 else (text or fallback)


def _explicit_edges(node: dict) -> list[tuple[str, str, str]]:
    """返回 ``(目标, 标签, 类型)``，保留分支标签但消除完全重复项。"""
    result: list[tuple[str, str, str]] = []
    goto = node.get("goto")
    if isinstance(goto, str) and goto:
        result.append((goto, "跳转", "goto"))

    node_type = node.get("type")
    if node_type == "choice":
        for index, option in enumerate(node.get("options") or [], 1):
            if not isinstance(option, dict):
                continue
            target = option.get("goto")
            if isinstance(target, str) and target:
                result.append(
                    (target, _short_label(option.get("text"), f"选项 {index}"), "choice")
                )
    elif node_type == "branch":
        for case in node.get("cases") or []:
            if not isinstance(case, dict):
                continue
            target = case.get("goto")
            if isinstance(target, str) and target:
                op = str(case.get("op") or "=")
                result.append((target, f"{op}{case.get('value', '')}", "branch"))
    elif node_type == "dice":
        result_names = {
            "goto_大成功": "大成功",
            "goto_成功": "成功",
            "goto_失败": "失败",
        }
        for index, option in enumerate(node.get("options") or [], 1):
            if not isinstance(option, dict):
                continue
            for key, result_name in result_names.items():
                target = option.get(key)
                if isinstance(target, str) and target:
                    result.append((target, f"第{index}项·{result_name}", "dice"))
    elif node_type in ("combat", "battle", "battle_result"):
        labels = (("win", "友军胜利"), ("lose", "敌军胜利")) if node_type == "battle" else (("win", "胜利"), ("lose", "失败"))
        for key, label in labels:
            target = node.get(key)
            if isinstance(target, str) and target:
                result.append((target, label, node_type))
    elif node_type in CHECK_TYPES:
        labels = (
            (("success", "命中"), ("failure", "未命中"))
            if node_type == "quest_check"
            else (("success", "成功"), ("failure", "失败"))
        )
        for key, label in labels:
            target = node.get(key)
            if isinstance(target, str) and target:
                result.append((target, label, node_type))

    seen: set[tuple[str, str, str]] = set()
    return [item for item in result if not (item in seen or seen.add(item))]


def _reachable(start: str, adjacency: dict[str, set[str]]) -> set[str]:
    reached: set[str] = set()
    pending = [start] if start in adjacency else []
    while pending:
        current = pending.pop()
        if current in reached:
            continue
        reached.add(current)
        pending.extend(adjacency.get(current, ()))
    return reached


def _cyclic_nodes(adjacency: dict[str, set[str]]) -> set[str]:
    """Tarjan SCC：返回属于环（含自环）的节点。"""
    index = 0
    indices: dict[str, int] = {}
    lowlinks: dict[str, int] = {}
    stack: list[str] = []
    on_stack: set[str] = set()
    cyclic: set[str] = set()

    def visit(node: str) -> None:
        nonlocal index
        indices[node] = index
        lowlinks[node] = index
        index += 1
        stack.append(node)
        on_stack.add(node)
        for target in adjacency.get(node, ()):
            if target not in indices:
                visit(target)
                lowlinks[node] = min(lowlinks[node], lowlinks[target])
            elif target in on_stack:
                lowlinks[node] = min(lowlinks[node], indices[target])
        if lowlinks[node] != indices[node]:
            return
        component: list[str] = []
        while stack:
            member = stack.pop()
            on_stack.remove(member)
            component.append(member)
            if member == node:
                break
        if len(component) > 1 or node in adjacency.get(node, set()):
            cyclic.update(component)

    for node in adjacency:
        if node not in indices:
            visit(node)
    return cyclic


def analyze_story(story: dict) -> StoryGraphAnalysis:
    nodes = [node for node in story.get("nodes") or [] if isinstance(node, dict)]
    node_order = tuple(
        node["id"] for node in nodes if isinstance(node.get("id"), str) and node["id"]
    )
    known = set(node_order)
    adjacency: dict[str, set[str]] = {node_id: set() for node_id in node_order}
    edges: list[GraphEdge] = []
    missing_targets: set[str] = set()

    for index, node in enumerate(nodes):
        source = node.get("id")
        if source not in adjacency:
            continue
        explicit = _explicit_edges(node)
        for target, label, kind in explicit:
            missing = target not in known
            edges.append(GraphEdge(source, target, label, kind, missing))
            if missing:
                missing_targets.add(source)
            else:
                adjacency[source].add(target)

        node_type = node.get("type")
        has_goto = isinstance(node.get("goto"), str) and bool(node.get("goto"))
        if (
            index + 1 < len(nodes)
            and node_type not in NO_FALLTHROUGH_TYPES
            and (node_type == "branch" or not has_goto)
        ):
            target = nodes[index + 1].get("id")
            if isinstance(target, str) and target:
                edges.append(GraphEdge(source, target, "下一步", "fallthrough"))
                if target in known:
                    adjacency[source].add(target)
                else:
                    missing_targets.add(source)

    reachable = _reachable(str(story.get("start") or ""), adjacency)
    unreachable = known - reachable
    terminal_nodes = {
        str(node.get("id")) for node in nodes if node.get("type") in TERMINAL_TYPES
    }
    reverse: dict[str, set[str]] = {node_id: set() for node_id in node_order}
    for source, targets in adjacency.items():
        for target in targets:
            reverse[target].add(source)
    can_finish: set[str] = set()
    pending = list(terminal_nodes)
    while pending:
        current = pending.pop()
        if current in can_finish:
            continue
        can_finish.add(current)
        pending.extend(reverse.get(current, ()))

    cyclic = _cyclic_nodes(adjacency)
    infinite_loops = cyclic & reachable - can_finish
    dead_ends = {
        node_id
        for node_id in reachable
        if node_id not in terminal_nodes and not adjacency[node_id]
    }
    return StoryGraphAnalysis(
        node_order=node_order,
        edges=tuple(edges),
        reachable=frozenset(reachable),
        unreachable=frozenset(unreachable),
        dead_ends=frozenset(dead_ends),
        infinite_loops=frozenset(infinite_loops),
        missing_targets=frozenset(missing_targets),
    )
