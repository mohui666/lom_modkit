# -*- coding: utf-8 -*-
"""面向编辑器用户的发布前体检与保守自动修复。

编译器校验仍然是最终契约；这里把它转换成带章节/步骤定位的报告，并补充
“能编译但很可能不是作者本意”的可达性、占位文字和素材检查。
"""

from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
import re

from asset_store import MAX_IMAGE_BYTES, resolve_image_asset
import content_registry
from i18n import t
from lomc.content import (
    CONTENT_TYPES,
    check_character_portrait,
    check_content_matches_kind,
    load_content_metadata,
    parse_content_ref,
    resolve_content,
    resolve_content_dir,
    validate_content_id,
)
from lomc.errors import LomcError
from lua_preview import get_lomc
import models
import stage_guard
from story_graph import TERMINAL_TYPES, analyze_story
from symbol_analysis import analyze_symbols, find_read_before_write


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
        return t("preflight.error") if self.severity == "error" else t("preflight.warning")


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


def _content_specs(stories: dict[str, dict]):
    """Yield every field whose ``user:`` value has a statically known role."""
    for sid in sorted(stories):
        for node in stories[sid].get("nodes") or []:
            if not isinstance(node, dict):
                continue
            node_id = str(node.get("id") or "")
            ntype = node.get("type")
            if ntype in ("music", "sound"):
                expected_kind = (
                    "music" if ntype == "music"
                    else ("env" if node.get("kind", "sound") == "env" else "sound")
                )
                yield sid, node_id, "name", node.get("name"), "audio", expected_kind, None
            if ntype == "say" and node.get("voice"):
                yield sid, node_id, "voice", node.get("voice"), "audio", None, None
            if ntype in (
                "show", "say", "hide", "move", "face", "focus", "offset",
                "shock", "dim", "rotate", "intro",
            ) and node.get("character"):
                portrait = node.get("portrait") if ntype in ("show", "say") else None
                yield sid, node_id, "character", node.get("character"), "character", None, portrait
            image_active = not (
                (ntype in ("custom_cg", "overlay") and node.get("action", "show") == "hide")
                or (ntype == "background" and node.get("action", "show") in ("fadeout", "clear"))
            )
            if image_active and node.get("image"):
                yield sid, node_id, "image", node.get("image"), "image", None, None


def _looks_like_illegal_path(value: object) -> bool:
    if not isinstance(value, str):
        return False
    text = value.strip().replace("\\", "/")
    body = text[5:] if text.startswith("user:") else text
    return (
        text.startswith(("/", "\\"))
        or bool(re.match(r"^[A-Za-z]:", text))
        or any(part == ".." for part in body.split("/"))
        or (text.startswith("user:") and "/" in body)
    )


def _missing_content_code(field: str, expected_type: str) -> str:
    if field == "voice":
        return "missing_voice"
    if expected_type == "image":
        return "missing_image"
    return "impossible_content_reference"


def _content_reference_issues(
    stories: dict[str, dict], content_root: Path
) -> tuple[list[PreflightIssue], set[tuple[str, str]]]:
    issues: list[PreflightIssue] = []
    referenced: set[tuple[str, str]] = set()
    root = str(content_root)
    for sid, node_id, field, raw, expected_type, expected_kind, portrait in _content_specs(stories):
        if not isinstance(raw, str) or not raw.startswith("user:"):
            if field == "voice" and raw:
                code = "illegal_content_path" if _looks_like_illegal_path(raw) else "impossible_content_reference"
                issues.append(PreflightIssue(
                    "error", code, sid, node_id,
                    t("preflight.impossible_content", ref=repr(raw)),
                ))
            elif _looks_like_illegal_path(raw):
                issues.append(PreflightIssue(
                    "error", "illegal_content_path", sid, node_id,
                    t("preflight.illegal_content_path", ref=repr(raw)),
                ))
            continue
        try:
            ref = parse_content_ref(raw, label="用户内容引用")
        except LomcError as exc:
            code = "illegal_content_path" if _looks_like_illegal_path(raw) else "impossible_content_reference"
            issues.append(PreflightIssue("error", code, sid, node_id, str(exc)))
            continue
        if ref is None:
            continue
        referenced.add((expected_type, ref.content_id))
        found_types = {
            ctype for ctype in CONTENT_TYPES
            if resolve_content_dir(root, ctype, ref.content_id) is not None
        }
        if expected_type not in found_types:
            if found_types:
                issues.append(PreflightIssue(
                    "error", "wrong_user_content_type", sid, node_id,
                    t(
                        "preflight.wrong_content_type", ref=raw,
                        actual=" / ".join(sorted(found_types)), expected=expected_type,
                    ),
                ))
            else:
                code = _missing_content_code(field, expected_type)
                issues.append(PreflightIssue(
                    "error", code, sid, node_id,
                    t("preflight.missing_content", ref=raw),
                ))
            continue
        try:
            meta, _main_path = resolve_content(root, expected_type, ref.content_id)
            if expected_type == "character":
                check_character_portrait(meta, portrait, raw)
            elif expected_kind:
                check_content_matches_kind(meta, expected_kind, raw)
        except LomcError as exc:
            detail = str(exc)
            if "没有表情" in detail or "立绘不存在" in detail:
                code = "missing_portrait"
            elif "文件不存在" in detail or "文件是空的" in detail:
                code = _missing_content_code(field, expected_type)
                if expected_type == "character":
                    code = "missing_portrait"
            elif "不能用在" in detail:
                code = "wrong_user_content_type"
            elif "路径" in detail or "文件名" in detail or "同目录" in detail:
                code = "illegal_content_path"
            else:
                code = "stale_content_metadata"
            issues.append(PreflightIssue("error", code, sid, node_id, detail))
    return issues, referenced


def _repository_issues(
    content_root: Path, referenced: set[tuple[str, str]]
) -> list[PreflightIssue]:
    """Audit the on-disk registry, including entries normal scans intentionally skip."""
    issues: list[PreflightIssue] = []
    valid: dict[tuple[str, str], tuple[dict, Path]] = {}
    user_root = content_root / "assets" / "user"
    if not user_root.is_dir():
        return issues
    for type_dir in sorted(user_root.iterdir(), key=lambda path: path.name):
        if not type_dir.is_dir():
            continue
        if type_dir.name not in CONTENT_TYPES:
            issues.append(PreflightIssue(
                "error", "illegal_content_path", "", "",
                t("preflight.illegal_content_folder", path=str(type_dir)),
            ))
            continue
        for folder in sorted(type_dir.iterdir(), key=lambda path: path.name):
            if not folder.is_dir():
                continue
            label = "user:%s" % folder.name
            try:
                validate_content_id(folder.name)
            except LomcError as exc:
                issues.append(PreflightIssue("error", "illegal_content_path", "", "", str(exc)))
                continue
            meta_path = folder / "content.json"
            try:
                meta = load_content_metadata(str(meta_path))
                if meta["id"] != folder.name or meta["type"] != type_dir.name:
                    raise LomcError(
                        "%s 的 metadata 与目录身份不一致（id=%r, type=%r）"
                        % (label, meta["id"], meta["type"])
                    )
                resolve_content(str(content_root), type_dir.name, folder.name)
            except LomcError as exc:
                detail = str(exc)
                code = (
                    "illegal_content_path"
                    if "路径" in detail or "同目录" in detail or "文件名" in detail
                    else "stale_content_metadata"
                )
                issues.append(PreflightIssue("error", code, "", "", detail))
                continue
            valid[(type_dir.name, folder.name)] = (meta, folder)
    for identity, (meta, _folder) in sorted(valid.items()):
        if identity not in referenced:
            issues.append(PreflightIssue(
                "warning", "unused_content", "", "",
                t("preflight.unused_content", ref="user:" + identity[1], kind=meta["type"]),
            ))

    index_path = content_root / "registry.json"
    if index_path.is_file():
        try:
            raw_index = json.loads(index_path.read_text(encoding="utf-8-sig"))
            entries = raw_index.get("contents") if isinstance(raw_index, dict) else None
            indexed = {
                (item.get("type"), item.get("id"))
                for item in entries or [] if isinstance(item, dict)
            }
            if not isinstance(entries, list) or indexed != set(valid):
                issues.append(PreflightIssue(
                    "warning", "stale_content_metadata", "", "",
                    t("preflight.stale_registry"),
                ))
        except (OSError, UnicodeError, json.JSONDecodeError):
            issues.append(PreflightIssue(
                "warning", "stale_content_metadata", "", "",
                t("preflight.stale_registry"),
            ))
    return issues


def _story_level_no_exit_scc(stories: dict[str, dict]) -> list[tuple[str, str]]:
    """Return cross-story cycles that cannot reach a project terminal."""
    edges: dict[str, set[str]] = {sid: set() for sid in stories}
    edge_node: dict[tuple[str, str], str] = {}
    final: set[str] = set()
    uncertain: set[str] = set()
    for sid, story in stories.items():
        reachable = set(analyze_story(story).reachable)
        for node in story.get("nodes") or []:
            if not isinstance(node, dict) or node.get("id") not in reachable:
                continue
            if node.get("type") == "raw":
                uncertain.add(sid)
            if node.get("type") not in TERMINAL_TYPES:
                continue
            next_script = node.get("next_script")
            if node.get("type") == "end" and isinstance(next_script, str) and next_script in stories:
                target = next_script
                edges[sid].add(target)
                edge_node[(sid, target)] = str(node.get("id") or "")
            elif not (node.get("type") == "end" and node.get("next_script")):
                final.add(sid)
    can_finish = set(final)
    changed = True
    while changed:
        changed = False
        for sid, targets in edges.items():
            if sid not in can_finish and any(target in can_finish for target in targets):
                can_finish.add(sid)
                changed = True

    index = 0
    indices: dict[str, int] = {}
    low: dict[str, int] = {}
    stack: list[str] = []
    on_stack: set[str] = set()
    components: list[set[str]] = []

    def visit(sid: str) -> None:
        nonlocal index
        indices[sid] = low[sid] = index
        index += 1
        stack.append(sid)
        on_stack.add(sid)
        for target in edges[sid]:
            if target not in indices:
                visit(target)
                low[sid] = min(low[sid], low[target])
            elif target in on_stack:
                low[sid] = min(low[sid], indices[target])
        if low[sid] == indices[sid]:
            component: set[str] = set()
            while True:
                item = stack.pop()
                on_stack.remove(item)
                component.add(item)
                if item == sid:
                    break
            components.append(component)

    for sid in sorted(stories):
        if sid not in indices:
            visit(sid)
    result: list[tuple[str, str]] = []
    for component in components:
        cyclic = len(component) > 1 or any(sid in edges[sid] for sid in component)
        if not cyclic or component & can_finish or component & uncertain:
            continue
        for sid in sorted(component):
            target = next((item for item in sorted(edges[sid]) if item in component), "")
            result.append((sid, edge_node.get((sid, target), "")))
    return result


def run_preflight(
    stories: dict[str, dict],
    editor_data: dict,
    entry_story: str | None = None,
    *,
    manifest: dict | None = None,
    content_root: str | Path | None = None,
) -> list[PreflightIssue]:
    """返回稳定排序的完整体检报告，不修改项目。"""
    issues: list[PreflightIssue] = []
    manifest = manifest if isinstance(manifest, dict) else {}
    explicit_content_audit = content_root is not None
    resolved_content_root = (
        Path(content_root) if content_root is not None else content_registry.repository_root()
    )
    if entry_story is None:
        raw_entry = manifest.get("entry")
        entry_story = raw_entry if isinstance(raw_entry, str) else None
    manifest = dict(manifest)
    manifest["entry"] = entry_story
    if not isinstance(entry_story, str) or not entry_story or entry_story not in stories:
        issues.append(PreflightIssue(
            "error", "invalid_entry", str(entry_story or ""), "",
            t("preflight.invalid_entry", entry=repr(entry_story)),
        ))

    content_issues, referenced_content = _content_reference_issues(
        stories, resolved_content_root
    )
    issues.extend(content_issues)
    if explicit_content_audit:
        issues.extend(_repository_issues(resolved_content_root, referenced_content))

    lomc, lomc_error = get_lomc()
    if lomc is None:
        issues.append(PreflightIssue(
            "error", "compiler_missing", "", "",
            t("preflight.compiler_missing", err=lomc_error),
        ))
        return issues

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
        for stage_node_id, stage_cid in stage_guard.find_stage_issues(story):
            cname = models.character_name(editor_data, stage_cid)
            issues.append(
                PreflightIssue(
                    "warning",
                    "stage_missing",
                    sid,
                    stage_node_id,
                    t("preflight.stage_missing", name=cname),
                    fixable=True,
                )
            )
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
                        t("preflight.unreachable_node"),
                    )
                )
            if node_id in graph.dead_ends:
                issues.append(
                    PreflightIssue(
                        "warning",
                        "broken_flow",
                        sid,
                        node_id,
                        t("preflight.broken_flow"),
                    )
                )
            if node_id in graph.infinite_loops:
                issues.append(
                    PreflightIssue(
                        "error",
                        "no_exit_scc",
                        sid,
                        node_id,
                        t("preflight.infinite_loop"),
                    )
                )
            if _uses_placeholder(node):
                issues.append(
                    PreflightIssue(
                        "warning",
                        "placeholder_text",
                        sid,
                        node_id,
                        t("preflight.placeholder"),
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
                        t("preflight.back_pos", pos=node.get("position")),
                    )
                )
            image = node.get("image")
            if node.get("type") == "custom_cg" and node.get("action", "show") == "hide":
                image = None
            if node.get("type") == "background" and node.get("action", "show") in (
                "fadeout",
                "clear",
            ):
                image = None
            if isinstance(image, str) and image:
                if image.startswith("user:"):
                    source = None  # handled by the typed repository audit above
                else:
                    source = resolve_image_asset(image)
                if source is None and not image.startswith("user:"):
                    issues.append(
                        PreflightIssue(
                            "error",
                            "missing_image",
                            sid,
                            node_id,
                            t("preflight.missing_image", image=repr(image)),
                        )
                    )
                elif source is not None and source.stat().st_size > MAX_IMAGE_BYTES:
                    issues.append(
                        PreflightIssue(
                            "error",
                            "large_image",
                            sid,
                            node_id,
                            t("preflight.large_image", image=repr(image)),
                        )
                    )
            if node.get("type") == "end":
                target = node.get("next_script")
                if target not in (None, "") and (
                    not isinstance(target, str) or target not in story_ids
                ):
                    issues.append(
                        PreflightIssue(
                            "error",
                            "invalid_cross_story_goto",
                            sid,
                            node_id,
                            t("preflight.missing_story", target=repr(target)),
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
                    t(
                        "preflight.unreachable_story",
                        story=repr(sid),
                        entry=repr(entry_story),
                    ),
                )
            )

    campaign = manifest.get("campaign")
    triggers = campaign.get("triggers") if isinstance(campaign, dict) else []
    for index, trigger in enumerate(triggers or []):
        if not isinstance(trigger, dict):
            continue
        target = trigger.get("script")
        if not isinstance(target, str) or target not in stories:
            issues.append(PreflightIssue(
                "error", "invalid_cross_story_goto", str(target or ""), "",
                t("preflight.invalid_trigger_story", index=index, target=repr(target)),
            ))

    for sid, node_id in _story_level_no_exit_scc(stories):
        issues.append(PreflightIssue(
            "error", "no_exit_scc", sid, node_id,
            t("preflight.no_exit_scc"),
        ))

    for report in analyze_symbols(stories, manifest):
        if report.kind != "mod_flag" or not report.possibly_read_before_write:
            continue
        location = find_read_before_write(stories, report.name, manifest)
        issues.append(PreflightIssue(
            "warning", "possible_read_before_write",
            location[0] if location else "", location[1] if location else "",
            t("preflight.read_before_write", name=report.name),
        ))

    order = {"error": 0, "warning": 1}
    unique = {
        (item.severity, item.code, item.story_id, item.node_id, item.message): item
        for item in issues
    }
    return sorted(
        unique.values(),
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
        # 登场防线：动作人物在某条路径上未登场/已退场时，在该步骤前补登场。
        # 每修一个都可能连带修好后续步骤，修完重新分析直到没有遗漏。
        for _ in range(len(nodes) + 1):
            stage_issues = stage_guard.find_stage_issues(story)
            if not stage_issues:
                break
            stage_node_id, stage_cid = stage_issues[0]
            if stage_guard.ensure_stage(story, stage_node_id) is None:
                break
            cname = models.character_name(editor_data, stage_cid)
            fixes.append(
                f"章节 {sid} / 步骤 {stage_node_id}：在前面自动插入 {cname} 的登场"
            )
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
