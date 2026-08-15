# -*- coding: utf-8 -*-
"""Offline execution of the supported, deterministic Story semantics subset."""

from __future__ import annotations

import copy
import json
from dataclasses import dataclass

from PySide6.QtWidgets import (
    QDialog, QDialogButtonBox, QHBoxLayout, QLabel, QMessageBox, QPlainTextEdit,
    QPushButton, QTableWidget, QTableWidgetItem, QVBoxLayout,
)

from i18n import t


_UNSUPPORTED_TYPES = {
    "raw", "dice", "block", "panel", "enemy", "battle_skill", "mission",
    "time", "autosave", "affinity", "talent", "item",
}
_TERMINAL_TYPES = {"goto_scene", "death"}


@dataclass(frozen=True)
class StoryTestResult:
    name: str
    status: str  # pass | fail | unsupported
    message: str
    visited: tuple[tuple[str, str], ...]
    variables: dict
    flags: dict[str, bool]


def get_story_tests(story: dict) -> list[dict]:
    meta = story.get("_editor")
    tests = meta.get("tests") if isinstance(meta, dict) else None
    return copy.deepcopy(tests) if isinstance(tests, list) else []


def set_story_tests(story: dict, tests: list[dict]) -> None:
    checked = validate_test_cases(tests)
    meta = story.setdefault("_editor", {})
    if checked:
        meta["tests"] = copy.deepcopy(checked)
    else:
        meta.pop("tests", None)
        if not meta: story.pop("_editor", None)


def validate_test_cases(tests) -> list[dict]:
    if not isinstance(tests, list):
        raise ValueError("测试定义必须是 JSON 数组")
    checked = copy.deepcopy(tests)
    names = set()
    for index, case in enumerate(checked):
        if not isinstance(case, dict): raise ValueError("测试 #%d 必须是对象" % (index + 1))
        name = str(case.get("name") or "").strip()
        story = str(case.get("story") or "").strip()
        if not name or name in names: raise ValueError("测试名称不能为空或重复: %s" % name)
        if not story: raise ValueError("测试 %s 缺少 story" % name)
        names.add(name); case["name"] = name; case["story"] = story
        initial = case.get("initial", {})
        if not isinstance(initial, dict) or not isinstance(initial.get("variables", {}), dict) or not isinstance(initial.get("flags", {}), dict):
            raise ValueError("测试 %s 的 initial.variables/flags 必须是对象" % name)
        actions = case.get("actions", {})
        if not isinstance(actions, dict) or not isinstance(actions.get("choices", []), list):
            raise ValueError("测试 %s 的 actions.choices 必须是数组" % name)
        for action in actions.get("choices", []):
            if not isinstance(action, dict) or not isinstance(action.get("node"), str) or not isinstance(action.get("option"), int):
                raise ValueError("测试 %s 的 choice 动作必须含 node 字符串与 option 整数" % name)
        assertions = case.get("assert", {})
        if not isinstance(assertions, dict): raise ValueError("测试 %s 的 assert 必须是对象" % name)
    return checked


def _next_node(nodes: list[dict], index: int, node: dict) -> str | None:
    if isinstance(node.get("goto"), str) and node["goto"]: return node["goto"]
    return str(nodes[index + 1].get("id")) if index + 1 < len(nodes) and nodes[index + 1].get("id") else None


def _branch_target(node: dict, variables: dict, flags: dict[str, bool], fallback: str | None):
    source = str(node.get("source") or "mod")
    key = str(node.get("flag") or node.get("stat") or "")
    if source == "mod": value = 1 if flags.get(key, False) else 2
    elif source == "condition":
        if key not in variables: raise RuntimeError("UNSUPPORTED: Condition %s 没有 initial.variables 初值" % key)
        value = 1 if bool(variables[key]) else 2
    elif source in ("stat", "flag_value", "game"):
        if key not in variables: raise RuntimeError("UNSUPPORTED: %s %s 没有 initial.variables 初值" % (source, key))
        value = variables[key]
    else: raise RuntimeError("UNSUPPORTED: branch source=%s" % source)
    for case in node.get("cases") or []:
        if not isinstance(case, dict): continue
        if source in ("mod", "condition", "game") and value == case.get("value"): return case.get("goto")
        if source in ("stat", "flag_value"):
            op, expected = case.get("op", ">="), case.get("value")
            matched = {">=": value >= expected, ">": value > expected, "<=": value <= expected, "<": value < expected, "==": value == expected, "=": value == expected, "!=": value != expected}.get(op)
            if matched: return case.get("goto")
    return fallback


def run_story_test(stories: dict[str, dict], case: dict, max_steps: int = 1000) -> StoryTestResult:
    case = validate_test_cases([case])[0]
    name = case["name"]
    if case["story"] not in stories:
        return StoryTestResult(name, "fail", "起始章节不存在: %s" % case["story"], (), {}, {})
    initial = case.get("initial", {})
    variables = copy.deepcopy(initial.get("variables", {}))
    flags = {str(key): bool(value) for key, value in initial.get("flags", {}).items()}
    choice_actions: dict[str, list[int]] = {}
    for action in case.get("actions", {}).get("choices", []):
        choice_actions.setdefault(action["node"], []).append(action["option"])
    visited: list[tuple[str, str]] = []
    story_id = case["story"]; node_id = str(stories[story_id].get("start") or "")
    ending = False
    try:
        for _step in range(max_steps):
            story = stories[story_id]; nodes = [node for node in story.get("nodes") or [] if isinstance(node, dict)]
            by_id = {str(node.get("id")): (index, node) for index, node in enumerate(nodes) if node.get("id")}
            if node_id not in by_id: raise ValueError("节点不存在: %s/%s" % (story_id, node_id))
            index, node = by_id[node_id]; visited.append((story_id, node_id)); node_type = str(node.get("type") or "")
            if node_type in _UNSUPPORTED_TYPES: raise RuntimeError("UNSUPPORTED: 节点 %s/%s 类型 %s 需要 Runtime" % (story_id, node_id, node_type))
            fallback = _next_node(nodes, index, node)
            if node_type == "flag": flags[str(node.get("flag") or "")] = True
            elif node_type == "stat_set": variables[str(node.get("key") or "")] = node.get("value")
            elif node_type == "stat":
                key = str(node.get("key") or "")
                if key not in variables: raise RuntimeError("UNSUPPORTED: stat %s 没有 initial.variables 初值" % key)
                variables[key] += node.get("delta", 0)
            elif node_type == "game_flag":
                key = str(node.get("flag") or "")
                if node.get("op", "set") == "add":
                    if key not in variables: raise RuntimeError("UNSUPPORTED: game_flag %s add 没有初值" % key)
                    variables[key] += node.get("value", 0)
                else: variables[key] = node.get("value")
            elif node_type == "branch": fallback = _branch_target(node, variables, flags, fallback)
            elif node_type == "choice":
                actions = choice_actions.get(node_id) or []
                if not actions: raise RuntimeError("UNSUPPORTED: choice %s 没有 actions.choices" % node_id)
                option_index = actions.pop(0); options = node.get("options") or []
                if not (0 <= option_index < len(options)): raise ValueError("choice %s 的 option 越界" % node_id)
                fallback = options[option_index].get("goto")
            elif node_type == "end":
                next_story = node.get("next_script")
                if next_story:
                    if next_story not in stories: raise ValueError("next_script 不存在: %s" % next_story)
                    story_id = next_story; node_id = str(stories[story_id].get("start") or ""); continue
                ending = True; break
            elif node_type in _TERMINAL_TYPES: ending = True; break
            if not fallback: raise ValueError("节点没有后继: %s/%s" % (story_id, node_id))
            node_id = str(fallback)
        else: raise RuntimeError("UNSUPPORTED: 超过 %d 步，可能是循环" % max_steps)
    except RuntimeError as exc:
        if str(exc).startswith("UNSUPPORTED:"):
            return StoryTestResult(name, "unsupported", str(exc)[12:].strip(), tuple(visited), variables, flags)
        raise
    except (TypeError, ValueError) as exc:
        return StoryTestResult(name, "fail", str(exc), tuple(visited), variables, flags)

    assertions = case.get("assert", {})
    failures = []
    required_nodes = assertions.get("reaches_node", [])
    if isinstance(required_nodes, str): required_nodes = [required_nodes]
    visited_ids = {node for _story, node in visited}
    for required in required_nodes:
        if required not in visited_ids: failures.append("未到达节点 %s" % required)
    if assertions.get("reaches_ending") is True and not ending: failures.append("未到达结局")
    for key, expected in assertions.get("variables", {}).items():
        if variables.get(key) != expected: failures.append("变量 %s: 期望 %r，实际 %r" % (key, expected, variables.get(key)))
    for key, expected in assertions.get("flags", {}).items():
        if flags.get(key, False) != bool(expected): failures.append("Flag %s: 期望 %r，实际 %r" % (key, bool(expected), flags.get(key, False)))
    return StoryTestResult(name, "fail" if failures else "pass", "; ".join(failures) or "OK", tuple(visited), variables, flags)


def run_test_suite(stories: dict[str, dict], tests: list[dict]) -> list[StoryTestResult]:
    return [run_story_test(stories, case) for case in validate_test_cases(tests)]


class StoryTestRunnerDialog(QDialog):
    def __init__(self, stories: dict[str, dict], current_story: str, parent=None):
        super().__init__(parent); self._stories = stories; self._current_story = current_story; self._saved = None
        self.setWindowTitle(t("tests.title")); self.resize(900, 680)
        root = QVBoxLayout(self); intro = QLabel(t("tests.intro")); intro.setWordWrap(True); root.addWidget(intro)
        tests = get_story_tests(stories[current_story])
        if not tests: tests = [{"name": "example", "story": current_story, "initial": {"variables": {}, "flags": {}}, "actions": {"choices": []}, "assert": {"reaches_ending": True}}]
        self.editor = QPlainTextEdit(json.dumps(tests, ensure_ascii=False, indent=2)); root.addWidget(self.editor, 1)
        run = QPushButton(t("tests.run")); run.clicked.connect(self._run); root.addWidget(run)
        self.table = QTableWidget(0, 3); self.table.setHorizontalHeaderLabels([t("tests.col.name"), t("tests.col.status"), t("tests.col.message")]); root.addWidget(self.table, 1)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Cancel)
        save = buttons.addButton(t("tests.save"), QDialogButtonBox.ButtonRole.AcceptRole); save.clicked.connect(self._save)
        buttons.rejected.connect(self.reject); root.addWidget(buttons)

    def saved_tests(self): return copy.deepcopy(self._saved)
    def _parse(self):
        try: return validate_test_cases(json.loads(self.editor.toPlainText()))
        except (ValueError, json.JSONDecodeError) as exc: QMessageBox.warning(self, t("tests.title"), str(exc)); return None
    def _run(self):
        tests = self._parse()
        if tests is None: return
        self.table.setRowCount(0)
        for result in run_test_suite(self._stories, tests):
            row = self.table.rowCount(); self.table.insertRow(row)
            for col, value in enumerate((result.name, t("tests.status." + result.status), result.message)): self.table.setItem(row, col, QTableWidgetItem(value))
    def _save(self):
        tests = self._parse()
        if tests is None: return
        self._saved = tests; self.accept()
