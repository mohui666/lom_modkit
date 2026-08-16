# -*- coding: utf-8 -*-
"""Readable, conservative inspection of structured branch conditions."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QDialog, QDialogButtonBox, QHBoxLayout, QHeaderView, QLabel, QPushButton,
    QTableWidget, QTableWidgetItem, QVBoxLayout,
)

from i18n import t
from reference_inspector import ReferenceDialog, ReferenceTarget
from story_graph import analyze_story


@dataclass(frozen=True)
class ConditionReport:
    story_id: str
    node_id: str
    source: str
    summary: str
    variables: tuple[str, ...]
    flags: tuple[str, ...]
    targets: tuple[str, ...]
    proof: str  # always_true | always_false | unknown
    proof_reason: str


def _adjacency(story: dict) -> tuple[dict[str, dict], dict[str, set[str]], set[str]]:
    nodes = {
        str(node.get("id")): node
        for node in story.get("nodes") or []
        if isinstance(node, dict) and node.get("id")
    }
    graph = analyze_story(story)
    adjacency = {node_id: set() for node_id in nodes}
    for edge in graph.edges:
        if not edge.missing and edge.source in adjacency and edge.target in adjacency:
            adjacency[edge.source].add(edge.target)
    return nodes, adjacency, set(graph.reachable)


def _flag_written_on_all_paths(story: dict, branch_id: str, flag: str) -> bool:
    nodes, adjacency, reachable = _adjacency(story)
    if branch_id not in reachable:
        return False
    start = str(story.get("start") or "")
    pending = [start] if start in nodes else []
    reached_without_write: set[str] = set()
    while pending:
        node_id = pending.pop()
        if node_id in reached_without_write:
            continue
        reached_without_write.add(node_id)
        if node_id == branch_id:
            return False
        node = nodes[node_id]
        writes = node.get("type") == "flag" and node.get("flag") == flag
        if not writes:
            pending.extend(adjacency.get(node_id, ()))
    return True


def _targets(node: dict, next_id: str | None) -> tuple[str, ...]:
    values = [
        str(case.get("goto"))
        for case in node.get("cases") or []
        if isinstance(case, dict) and case.get("goto")
    ]
    source = str(node.get("source") or "mod")
    covered = {case.get("value") for case in node.get("cases") or [] if isinstance(case, dict)}
    needs_fallback = source not in ("mod", "condition") or covered != {1, 2}
    if next_id and needs_fallback:
        values.append(next_id)
    return tuple(dict.fromkeys(values))


def _case_text(node: dict) -> str:
    source = str(node.get("source") or "mod")
    pieces = []
    for case in node.get("cases") or []:
        if not isinstance(case, dict):
            continue
        if source in ("mod", "condition"):
            condition = t("conditions.true") if case.get("value") == 1 else t("conditions.false")
        elif source == "game":
            condition = "= %s" % case.get("value", "?")
        else:
            condition = "%s %s" % (case.get("op", ">="), case.get("value", "?"))
        pieces.append("%s → %s" % (condition, case.get("goto", "?")))
    return "; ".join(pieces)


def inspect_conditions(stories: dict[str, dict]) -> list[ConditionReport]:
    reports: list[ConditionReport] = []
    for story_id in sorted(stories):
        story = stories[story_id]
        nodes = [node for node in story.get("nodes") or [] if isinstance(node, dict)]
        for index, node in enumerate(nodes):
            if node.get("type") != "branch":
                continue
            node_id = str(node.get("id") or "")
            source = str(node.get("source") or "mod")
            flag = str(node.get("flag") or "")
            stat = str(node.get("stat") or "")
            subject = {
                "mod": t("conditions.subject.mod", name=flag),
                "condition": t("conditions.subject.condition", name=flag),
                "game": t("conditions.subject.game", name=flag),
                "flag_value": t("conditions.subject.flag_value", name=flag),
                "stat": t("conditions.subject.stat", name=stat),
            }.get(source, source)
            proof = "unknown"
            reason = t("conditions.proof.unknown")
            if source == "mod" and flag and _flag_written_on_all_paths(story, node_id, flag):
                proof = "always_true"
                reason = t("conditions.proof.dominating_write", name=flag)
            next_id = str(nodes[index + 1].get("id")) if index + 1 < len(nodes) and nodes[index + 1].get("id") else None
            variables = (stat,) if source == "stat" and stat else ()
            flags = (flag,) if source in ("mod", "condition", "game", "flag_value") and flag else ()
            reports.append(ConditionReport(
                story_id, node_id, source,
                "%s：%s" % (subject, _case_text(node)),
                variables, flags, _targets(node, next_id), proof, reason,
            ))
    return reports


class ConditionInspectorDialog(QDialog):
    def __init__(self, stories: dict[str, dict], locate: Callable[[str, str | None], None], parent=None, manifest: dict | None = None):
        super().__init__(parent)
        self._stories = stories; self._locate = locate; self._manifest = manifest or {}
        self._reports = inspect_conditions(stories)
        self.setWindowTitle(t("conditions.title")); self.resize(1080, 570)
        root = QVBoxLayout(self)
        intro = QLabel(t("conditions.intro")); intro.setWordWrap(True); root.addWidget(intro)
        self.table = QTableWidget(0, 6)
        self.table.setHorizontalHeaderLabels([
            t("search.col.story"), t("search.col.node"), t("conditions.col.summary"),
            t("conditions.col.symbols"), t("conditions.col.targets"), t("conditions.col.proof"),
        ])
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QTableWidget.SelectionMode.SingleSelection)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        header = self.table.horizontalHeader()
        for column in (0, 1, 3, 4, 5): header.setSectionResizeMode(column, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(2, QHeaderView.ResizeMode.Stretch)
        root.addWidget(self.table, 1)
        for report in self._reports:
            row = self.table.rowCount(); self.table.insertRow(row)
            symbols = ", ".join(report.variables + report.flags) or "—"
            proof = t("conditions.proof." + report.proof)
            for column, value in enumerate((report.story_id, report.node_id, report.summary, symbols, ", ".join(report.targets), proof)):
                self.table.setItem(row, column, QTableWidgetItem(value))
            self.table.item(row, 0).setData(Qt.ItemDataRole.UserRole, report)
            self.table.item(row, 5).setToolTip(report.proof_reason)
        if self.table.rowCount(): self.table.selectRow(0)
        actions = QHBoxLayout()
        refs = QPushButton(t("references.action")); refs.clicked.connect(self._references); actions.addWidget(refs)
        jump = QPushButton(t("search.jump")); jump.clicked.connect(self._jump); actions.addWidget(jump)
        actions.addStretch(1)
        close = QDialogButtonBox(QDialogButtonBox.StandardButton.Close); close.rejected.connect(self.reject); actions.addWidget(close)
        root.addLayout(actions); self.table.cellDoubleClicked.connect(lambda *_: self._jump())

    def _selected(self) -> ConditionReport | None:
        item = self.table.item(self.table.currentRow(), 0)
        report = item.data(Qt.ItemDataRole.UserRole) if item is not None else None
        return report if isinstance(report, ConditionReport) else None

    def _jump(self) -> None:
        report = self._selected()
        if report is None: return
        self._locate(report.story_id, report.node_id); self.accept()

    def _references(self) -> None:
        report = self._selected()
        if report is None: return
        if report.flags:
            target = ReferenceTarget("flag", report.flags[0])
        elif report.variables:
            target = ReferenceTarget("variable", report.variables[0])
        else:
            return
        ReferenceDialog(self._stories, target, self._locate, self, manifest=self._manifest).exec()
