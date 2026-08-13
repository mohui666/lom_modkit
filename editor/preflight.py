# -*- coding: utf-8 -*-
"""面向编辑器用户的发布前体检与保守自动修复。

编译器校验仍然是最终契约；这里把它转换成带章节/步骤定位的报告，并补充
“能编译但很可能不是作者本意”的可达性、占位文字和素材检查。
"""

from __future__ import annotations

from dataclasses import dataclass
import re

from asset_store import MAX_IMAGE_BYTES, resolve_image_asset
from lua_preview import get_lomc
import models
from story_graph import analyze_story


_NODE_IN_MESSAGE = re.compile(r'节点\s*["“]([^"”]+)["”]')
_PLACEHOLDER_TEXTS = (
    "在这里填写",
    "选项一",
    "选项二",
    "新的结局",
)


@dataclass(frozen=True)
class PreflightIssue:
    severity: str  # error / warning
    code: str
    story_id: str
    node_id: str
    message: str
    fixable: bool = False

    @property
    def severity_text(self) -> str:
        return "错误" if self.severity == "error" else "提醒"


def _message_node_id(message: str) -> str:
    match = _NODE_IN_MESSAGE.search(message)
    return match.group(1) if match else ""


def _node_targets(node: dict) -> set[str]:
    """收集一个节点明确写出的本章节跳转。"""
    targets: set[str] = set()
    goto = node.get("goto")
    if isinstance(goto, str) and goto:
        targets.add(goto)
    for option in node.get("options") or []:
        if not isinstance(option, dict):
            continue
        for key in ("goto", "goto_大成功", "goto_成功", "goto_失败"):
            value = option.get(key)
            if isinstance(value, str) and value:
                targets.add(value)
    for case in node.get("cases") or []:
        if isinstance(case, dict):
            value = case.get("goto")
            if isinstance(value, str) and value:
                targets.add(value)
    return targets


def _reachable_node_ids(story: dict) -> set[str]:
    nodes = story.get("nodes")
    if not isinstance(nodes, list):
        return set()
    valid_nodes = [n for n in nodes if isinstance(n, dict)]
    by_id = {
        n.get("id"): (index, n)
        for index, n in enumerate(valid_nodes)
        if isinstance(n.get("id"), str)
    }
    start = story.get("start")
    if start not in by_id:
        return set()
    edges: dict[str, set[str]] = {}
    no_fallthrough = {"choice", "dice", "end", "goto_scene", "death"}
    for index, node in enumerate(valid_nodes):
        node_id = node.get("id")
        if node_id not in by_id:
            continue
        targets = _node_targets(node)
        # branch 未命中任何 case 时会顺序继续；普通节点没有显式 goto 也顺序继续。
        if (
            index + 1 < len(valid_nodes)
            and node.get("type") not in no_fallthrough
            and (node.get("type") == "branch" or not node.get("goto"))
        ):
            next_id = valid_nodes[index + 1].get("id")
            if isinstance(next_id, str):
                targets.add(next_id)
        edges[node_id] = {target for target in targets if target in by_id}

    reached: set[str] = set()
    pending = [start]
    while pending:
        current = pending.pop()
        if current in reached:
            continue
        reached.add(current)
        pending.extend(edges.get(current, ()))
    return reached


def _uses_placeholder(node: dict) -> bool:
    values: list[str] = []
    for key in ("text", "title", "desc", "name"):
        value = node.get(key)
        if isinstance(value, str):
            values.append(value)
    for option in node.get("options") or []:
        if isinstance(option, dict) and isinstance(option.get("text"), str):
            values.append(option["text"])
    return any(marker in value for marker in _PLACEHOLDER_TEXTS for value in values)


def run_preflight(
    stories: dict[str, dict],
    editor_data: dict,
    entry_story: str | None = None,
) -> list[PreflightIssue]:
    """返回稳定排序的完整体检报告，不修改项目。"""
    issues: list[PreflightIssue] = []
    lomc, lomc_error = get_lomc()
    if lomc is None:
        return [
            PreflightIssue(
                "error", "compiler_missing", "", "", f"编译器不可用：{lomc_error}"
            )
        ]

    story_ids = set(stories)
    for sid in sorted(stories):
        story = stories[sid]
        local_warnings: list[str] = []
        try:
            lomc.validate_story(story, f"章节 {sid}", local_warnings)
        except Exception as exc:
            message = str(exc)
            issues.append(
                PreflightIssue(
                    "error",
                    "compiler_error",
                    sid,
                    _message_node_id(message),
                    message,
                )
            )
        for message in local_warnings:
            issues.append(
                PreflightIssue(
                    "warning",
                    "compiler_warning",
                    sid,
                    _message_node_id(message),
                    message,
                )
            )

        nodes = story.get("nodes") if isinstance(story, dict) else None
        if not isinstance(nodes, list):
            continue
        graph = analyze_story(story)
        reached = set(graph.reachable)
        for node in nodes:
            if not isinstance(node, dict):
                continue
            node_id = node.get("id") if isinstance(node.get("id"), str) else ""
            if node_id and node_id not in reached:
                issues.append(
                    PreflightIssue(
                        "warning",
                        "unreachable_node",
                        sid,
                        node_id,
                        "这个步骤从剧情开头无法到达。若不是备用内容，请检查前面的跳转。",
                    )
                )
            if node_id in graph.dead_ends:
                issues.append(
                    PreflightIssue(
                        "warning",
                        "broken_flow",
                        sid,
                        node_id,
                        "剧情走到这里后没有下一步，也不是结局。请添加后续步骤或改为结束剧情。",
                    )
                )
            if node_id in graph.infinite_loops:
                issues.append(
                    PreflightIssue(
                        "warning",
                        "infinite_loop",
                        sid,
                        node_id,
                        "这个步骤位于无法走到任何结局的循环中，玩家会被困住。",
                    )
                )
            if _uses_placeholder(node):
                issues.append(
                    PreflightIssue(
                        "warning",
                        "placeholder_text",
                        sid,
                        node_id,
                        "仍含有示例或占位文字，发布前建议改成正式内容。",
                    )
                )
            if (
                node.get("type") == "show"
                and node.get("position") in ("LB2", "RB2")
                and node.get("fadeDuration", 0) == 0
                and node.get("moveDuration", 0) == 0
            ):
                issues.append(
                    PreflightIssue(
                        "warning",
                        "back_stage_position",
                        sid,
                        node_id,
                        f"人物直接登场在 {node.get('position')}（靠后站位），可能被前景遮挡。"
                        "普通登场建议改用 L2、M 或 R2。",
                    )
                )
            image = node.get("image")
            if isinstance(image, str) and image:
                source = resolve_image_asset(image)
                if source is None:
                    issues.append(
                        PreflightIssue(
                            "error",
                            "missing_image",
                            sid,
                            node_id,
                            f"找不到图片 {image!r}，请重新选择图片。",
                        )
                    )
                elif source.stat().st_size > MAX_IMAGE_BYTES:
                    issues.append(
                        PreflightIssue(
                            "error",
                            "large_image",
                            sid,
                            node_id,
                            f"图片 {image!r} 超过 8MB，请压缩后重新选择。",
                        )
                    )
            if node.get("type") == "end":
                target = node.get("next_script")
                if isinstance(target, str) and target and target not in story_ids:
                    issues.append(
                        PreflightIssue(
                            "error",
                            "missing_story",
                            sid,
                            node_id,
                            f"下一章节 {target!r} 不存在。",
                        )
                    )

    # 从入口章节检查跨章节链路；未被引用的章节常见于忘记设置“下一章节”。
    if entry_story in stories:
        reached_stories: set[str] = set()
        pending = [entry_story]
        while pending:
            sid = pending.pop()
            if sid in reached_stories or sid not in stories:
                continue
            reached_stories.add(sid)
            for node in stories[sid].get("nodes") or []:
                if isinstance(node, dict) and node.get("type") == "end":
                    target = node.get("next_script")
                    if isinstance(target, str) and target:
                        pending.append(target)
        for sid in sorted(story_ids - reached_stories):
            issues.append(
                PreflightIssue(
                    "warning",
                    "unreachable_story",
                    sid,
                    "",
                    f"章节 {sid!r} 无法从开始章节 {entry_story!r} 到达。",
                )
            )

    order = {"error": 0, "warning": 1}
    return sorted(
        issues,
        key=lambda item: (
            order.get(item.severity, 9),
            item.story_id,
            item.node_id,
            item.code,
            item.message,
        ),
    )


def apply_safe_fixes(stories: dict[str, dict], editor_data: dict) -> list[str]:
    """只做不需要猜测作者意图的修复；调用方负责建立撤销快照。"""
    fixes: list[str] = []
    repaired = models.normalize_character_ids(stories, editor_data)
    if repaired:
        fixes.append(f"修正 {repaired} 个人物内部 ID")

    for sid, story in stories.items():
        nodes = story.get("nodes") if isinstance(story, dict) else None
        if not isinstance(nodes, list) or not nodes:
            continue
        ids = [n.get("id") for n in nodes if isinstance(n, dict)]
        if story.get("start") not in ids and isinstance(ids[0], str):
            story["start"] = ids[0]
            fixes.append(f"章节 {sid}：把开头恢复为第一个步骤 {ids[0]}")
        for node in nodes:
            if not isinstance(node, dict):
                continue
            label = f"章节 {sid} / 步骤 {node.get('id', '?')}"
            if node.get("goto") == "":
                node.pop("goto", None)
                fixes.append(f"{label}：移除空的高级跳转")
            if node.get("type") == "end" and node.get("next_script") == "":
                node.pop("next_script", None)
                fixes.append(f"{label}：移除空的下一章节")
            if node.get("type") == "dice":
                for option in node.get("options") or []:
                    if not isinstance(option, dict):
                        continue
                    removed = False
                    for key in ("text", "threshold"):
                        if key in option:
                            option.pop(key, None)
                            removed = True
                    if removed:
                        fixes.append(f"{label}：移除已废弃的骰子字段")
            if node.get("type") == "goto_scene":
                scene = node.get("scene")
                allowed = (None, "", "Title", "Story") if scene == "End" else (
                    None,
                    "",
                    "Title",
                )
                if scene in ("End", "GameOver") and node.get("next") not in allowed:
                    node["next"] = "Title"
                    fixes.append(f"{label}：把无效的结局去向恢复为标题画面")
            if node.get("type") == "death" and node.get("next") not in (
                None,
                "",
                "Title",
            ):
                node["next"] = "Title"
                fixes.append(f"{label}：把无效的死亡画面去向恢复为标题画面")
    return fixes
