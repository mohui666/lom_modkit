# -*- coding: utf-8 -*-
"""Conservative project path simulation built on the compiler-aligned CFG."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QDialog, QDialogButtonBox, QHeaderView, QLabel, QTableWidget,
    QTableWidgetItem, QVBoxLayout,
)

from condition_inspector import inspect_conditions
from i18n import t
from story_graph import TERMINAL_TYPES, analyze_story


@dataclass(frozen=True)
class PathIssue:
    severity: str
    code: str
    story_id: str
    node_id: str | None
    detail: str


@dataclass(frozen=True)
class PathSimulation:
    issues: tuple[PathIssue, ...]
    uncertain_conditions: int
    story_count: int
    node_count: int


def _project_finishability(stories: dict[str, dict]):
    edges: dict[str, set[str]] = {story_id: set() for story_id in stories}
    final: set[str] = set()
    unknown: set[str] = set()
    malformed: list[PathIssue] = []
    for story_id, story in stories.items():
        graph = analyze_story(story)
        reachable = set(graph.reachable)
        for node in story.get("nodes") or []:
            if not isinstance(node, dict) or node.get("id") not in reachable:
                continue
            node_type = node.get("type")
            if node_type == "raw":
                unknown.add(story_id)
            elif node_type == "end" and node.get("next_script"):
                target = node.get("next_script")
                if not isinstance(target, str) or target not in stories:
                    malformed.append(PathIssue(
                        "error", "malformed_cross_story", story_id,
                        str(node.get("id") or "") or None,
                        t("paths.detail.bad_story", target=repr(target)),
                    ))
                else:
                    edges[story_id].add(target)
            elif node_type in TERMINAL_TYPES:
                final.add(story_id)
    can_finish = set(final)
    changed = True
    while changed:
        changed = False
        for story_id, targets in edges.items():
            if story_id not in can_finish and any(target in can_finish for target in targets):
                can_finish.add(story_id); changed = True
    return can_finish, unknown, malformed


def simulate_paths(stories: dict[str, dict], manifest: dict | None = None) -> PathSimulation:
    issues: list[PathIssue] = []
    node_count = 0
    condition_reports = inspect_conditions(stories)
    proof_by_node = {(report.story_id, report.node_id): report for report in condition_reports}
    for story_id in sorted(stories):
        story = stories[story_id]
        graph = analyze_story(story)
        node_count += len(graph.node_order)
        for node_id in sorted(graph.unreachable):
            issues.append(PathIssue("warning", "unreachable", story_id, node_id, t("paths.detail.unreachable")))
        for edge in graph.edges:
            if edge.missing:
                issues.append(PathIssue(
                    "error", "broken_target", story_id, edge.source,
                    t("paths.detail.broken", target=edge.target, label=edge.label),
                ))
        for node_id in sorted(graph.infinite_loops):
            issues.append(PathIssue("error", "no_exit_scc", story_id, node_id, t("paths.detail.no_exit")))
        for node in story.get("nodes") or []:
            if not isinstance(node, dict) or node.get("type") != "branch":
                continue
            node_id = str(node.get("id") or "")
            report = proof_by_node.get((story_id, node_id))
            dead_value = 2 if report and report.proof == "always_true" else 1 if report and report.proof == "always_false" else None
            if dead_value is None:
                continue
            for case in node.get("cases") or []:
                if isinstance(case, dict) and case.get("value") == dead_value:
                    issues.append(PathIssue(
                        "warning", "dead_branch", story_id, node_id,
                        t("paths.detail.dead_branch", value=dead_value, target=case.get("goto", "?")),
                    ))

    can_finish, unknown_finish, malformed = _project_finishability(stories)
    issues.extend(malformed)
    for story_id in sorted(stories):
        if story_id not in can_finish and story_id not in unknown_finish:
            issues.append(PathIssue("error", "missing_ending", story_id, None, t("paths.detail.missing_ending")))

    manifest = manifest or {}
    entry = manifest.get("entry")
    if entry is not None and entry not in stories:
        issues.append(PathIssue("error", "malformed_cross_story", str(entry), None, t("paths.detail.bad_entry", target=repr(entry))))
    for index, trigger in enumerate(((manifest.get("campaign") or {}).get("triggers") or [])):
        if isinstance(trigger, dict) and trigger.get("script") not in stories:
            issues.append(PathIssue(
                "error", "malformed_cross_story", str(trigger.get("script") or "?"), None,
                t("paths.detail.bad_trigger", index=index, target=repr(trigger.get("script"))),
            ))
    issues.sort(key=lambda issue: (issue.story_id, issue.node_id or "", issue.code, issue.detail))
    uncertain = sum(report.proof == "unknown" for report in condition_reports)
    return PathSimulation(tuple(issues), uncertain, len(stories), node_count)


class PathSimulatorDialog(QDialog):
    def __init__(self, stories: dict[str, dict], locate: Callable[[str, str | None], None], parent=None, manifest: dict | None = None):
        super().__init__(parent)
        self._locate = locate
        self.simulation = simulate_paths(stories, manifest)
        self.setWindowTitle(t("paths.title")); self.resize(980, 590)
        root = QVBoxLayout(self)
        summary = QLabel(t(
            "paths.summary", stories=self.simulation.story_count,
            nodes=self.simulation.node_count, issues=len(self.simulation.issues),
            unknown=self.simulation.uncertain_conditions,
        )); summary.setWordWrap(True); root.addWidget(summary)
        self.table = QTableWidget(0, 5)
        self.table.setHorizontalHeaderLabels([
            t("paths.col.severity"), t("paths.col.code"), t("search.col.story"),
            t("search.col.node"), t("paths.col.detail"),
        ])
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QTableWidget.SelectionMode.SingleSelection)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        header = self.table.horizontalHeader()
        for column in range(4): header.setSectionResizeMode(column, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(4, QHeaderView.ResizeMode.Stretch)
        root.addWidget(self.table, 1)
        for issue in self.simulation.issues:
            row = self.table.rowCount(); self.table.insertRow(row)
            values = (t("preflight." + issue.severity), t("paths.code." + issue.code), issue.story_id, issue.node_id or "—", issue.detail)
            for column, value in enumerate(values): self.table.setItem(row, column, QTableWidgetItem(value))
            self.table.item(row, 0).setData(Qt.ItemDataRole.UserRole, issue)
        if self.table.rowCount(): self.table.selectRow(0)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close); buttons.rejected.connect(self.reject); root.addWidget(buttons)
        self.table.cellDoubleClicked.connect(lambda *_: self._jump())

    def _jump(self) -> None:
        item = self.table.item(self.table.currentRow(), 0)
        issue = item.data(Qt.ItemDataRole.UserRole) if item is not None else None
        if not isinstance(issue, PathIssue) or issue.story_id not in self._locatable_stories(): return
        self._locate(issue.story_id, issue.node_id); self.accept()

    def _locatable_stories(self) -> set[str]:
        return {issue.story_id for issue in self.simulation.issues if issue.node_id}
