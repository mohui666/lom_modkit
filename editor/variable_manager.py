# -*- coding: utf-8 -*-
"""Structured variable/flag usage analysis and project-level manager UI."""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass
from typing import Callable

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QComboBox, QDialog, QDialogButtonBox, QHBoxLayout, QHeaderView, QLabel,
    QPushButton, QTableWidget, QTableWidgetItem, QVBoxLayout,
)

from i18n import t
from reference_inspector import ReferenceDialog, ReferenceTarget
from story_graph import analyze_story


@dataclass(frozen=True)
class SymbolUse:
    kind: str
    name: str
    access: str  # read | write
    story_id: str
    node_id: str | None
    field: str
    order: int
    external_consumption: bool = False


@dataclass(frozen=True)
class SymbolReport:
    kind: str
    name: str
    reads: int
    writes: int
    first_write: SymbolUse | None
    unused: bool | None
    possibly_read_before_write: bool | None
    uses: tuple[SymbolUse, ...]


def _structured_uses(stories: dict[str, dict], manifest: dict | None = None) -> list[SymbolUse]:
    uses: list[SymbolUse] = []
    order = 0
    for story_id in sorted(stories):
        story = stories[story_id]
        for node in story.get("nodes") or []:
            if not isinstance(node, dict):
                continue
            node_id = str(node.get("id") or "") or None
            node_type = node.get("type")
            if node_type == "flag" and isinstance(node.get("flag"), str) and node["flag"]:
                uses.append(SymbolUse("mod_flag", node["flag"], "write", story_id, node_id, "flag", order))
            elif node_type == "game_flag" and isinstance(node.get("flag"), str) and node["flag"]:
                uses.append(SymbolUse("game_flag", node["flag"], "write", story_id, node_id, "flag", order, True))
            elif node_type == "branch" and isinstance(node.get("flag"), str) and node["flag"]:
                source = str(node.get("source") or "mod")
                kind = {
                    "mod": "mod_flag",
                    "flag_value": "game_flag",
                    "game": "checkpoint",
                    "condition": "condition",
                }.get(source)
                if kind:
                    uses.append(SymbolUse(kind, node["flag"], "read", story_id, node_id, "flag", order, kind != "mod_flag"))
            elif node_type == "block":
                flowchart = str(node.get("flowchart") or "common")
                for index, variable in enumerate(node.get("vars") or []):
                    if not isinstance(variable, dict) or not isinstance(variable.get("name"), str) or not variable["name"]:
                        continue
                    uses.append(SymbolUse(
                        "flow_variable", variable["name"], "write", story_id, node_id,
                        "vars[%d].name" % index, order, True,
                    ))
            order += 1
    for index, trigger in enumerate(((manifest or {}).get("campaign") or {}).get("triggers") or []):
        if not isinstance(trigger, dict):
            continue
        story_id = str(trigger.get("script") or "")
        for field in ("when_flag_set", "when_flag_clear"):
            value = trigger.get(field)
            if isinstance(value, str) and value:
                uses.append(SymbolUse("game_flag", value, "read", story_id, None, "manifest.campaign.triggers[%d].%s" % (index, field), order, True))
                order += 1
    return uses


def _possible_mod_read_before_write(stories: dict[str, dict], symbol: str) -> bool:
    """True iff a reachable path can reach a read without crossing a write."""
    for story_id in sorted(stories):
        story = stories[story_id]
        nodes = [node for node in story.get("nodes") or [] if isinstance(node, dict)]
        by_id = {str(node.get("id")): node for node in nodes if node.get("id")}
        graph = analyze_story(story)
        adjacency: dict[str, set[str]] = {node_id: set() for node_id in by_id}
        for edge in graph.edges:
            if not edge.missing and edge.source in adjacency and edge.target in adjacency:
                adjacency[edge.source].add(edge.target)
        start = str(story.get("start") or "")
        pending = [start] if start in by_id else []
        reached_without_write: set[str] = set()
        while pending:
            node_id = pending.pop()
            if node_id in reached_without_write:
                continue
            reached_without_write.add(node_id)
            node = by_id[node_id]
            is_read = node.get("type") == "branch" and node.get("source", "mod") == "mod" and node.get("flag") == symbol
            if is_read:
                return True
            is_write = node.get("type") == "flag" and node.get("flag") == symbol
            if not is_write:
                pending.extend(adjacency.get(node_id, ()))
    return False


def analyze_symbols(stories: dict[str, dict], manifest: dict | None = None) -> list[SymbolReport]:
    uses = _structured_uses(stories, manifest)
    grouped: dict[tuple[str, str], list[SymbolUse]] = defaultdict(list)
    for use in uses:
        grouped[(use.kind, use.name)].append(use)
    reports: list[SymbolReport] = []
    for (kind, name), symbol_uses in sorted(grouped.items()):
        reads = [use for use in symbol_uses if use.access == "read"]
        writes = [use for use in symbol_uses if use.access == "write"]
        first_write = min(writes, key=lambda use: use.order) if writes else None
        # Only session-local mod flags have closed-world consumption semantics.
        # Official flags and flowchart variables may be read by original-game code.
        unused: bool | None = (not reads and bool(writes)) if kind == "mod_flag" else None
        read_before: bool | None = (
            _possible_mod_read_before_write(stories, name) if kind == "mod_flag" and reads else None
        )
        reports.append(SymbolReport(
            kind, name, len(reads), len(writes), first_write, unused,
            read_before, tuple(sorted(symbol_uses, key=lambda use: use.order)),
        ))
    return reports


class VariableManagerDialog(QDialog):
    def __init__(
        self,
        stories: dict[str, dict],
        locate: Callable[[str, str | None], None],
        parent=None,
        manifest: dict | None = None,
    ):
        super().__init__(parent)
        self._stories = stories
        self._manifest = manifest or {}
        self._locate = locate
        self._reports = analyze_symbols(stories, manifest)
        self.setWindowTitle(t("variables.title"))
        self.resize(960, 560)
        root = QVBoxLayout(self)
        intro = QLabel(t("variables.intro")); intro.setWordWrap(True); root.addWidget(intro)
        self.filter_combo = QComboBox()
        self.filter_combo.addItem(t("variables.all"), "")
        for kind in ("mod_flag", "game_flag", "checkpoint", "condition", "flow_variable"):
            self.filter_combo.addItem(t("variables.kind." + kind), kind)
        root.addWidget(self.filter_combo)
        self.table = QTableWidget(0, 7)
        self.table.setHorizontalHeaderLabels([
            t("variables.col.kind"), t("variables.col.name"), t("variables.col.reads"),
            t("variables.col.writes"), t("variables.col.first_write"),
            t("variables.col.unused"), t("variables.col.before_write"),
        ])
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QTableWidget.SelectionMode.SingleSelection)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        header = self.table.horizontalHeader()
        for column in range(7):
            header.setSectionResizeMode(column, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        root.addWidget(self.table, 1)
        actions = QHBoxLayout()
        refs = QPushButton(t("references.action")); refs.clicked.connect(self._references); actions.addWidget(refs)
        jump = QPushButton(t("search.jump")); jump.clicked.connect(self._jump); actions.addWidget(jump)
        actions.addStretch(1)
        close = QDialogButtonBox(QDialogButtonBox.StandardButton.Close); close.rejected.connect(self.reject); actions.addWidget(close)
        root.addLayout(actions)
        self.filter_combo.currentIndexChanged.connect(self._refresh)
        self.table.cellDoubleClicked.connect(lambda *_: self._jump())
        self._refresh()

    def _refresh(self, *_args) -> None:
        kind = str(self.filter_combo.currentData() or "")
        self.table.setRowCount(0)
        for report in self._reports:
            if kind and report.kind != kind:
                continue
            first = report.first_write
            first_text = ("%s · %s" % (first.story_id, first.node_id or "manifest")) if first else "—"
            values = (
                t("variables.kind." + report.kind), report.name, str(report.reads),
                str(report.writes), first_text, self._tri(report.unused),
                self._tri(report.possibly_read_before_write),
            )
            row = self.table.rowCount(); self.table.insertRow(row)
            for column, value in enumerate(values):
                self.table.setItem(row, column, QTableWidgetItem(value))
            self.table.item(row, 0).setData(Qt.ItemDataRole.UserRole, report)
        if self.table.rowCount(): self.table.selectRow(0)

    @staticmethod
    def _tri(value: bool | None) -> str:
        if value is True: return t("variables.yes")
        if value is False: return t("variables.no")
        return t("variables.unknown")

    def _selected(self) -> SymbolReport | None:
        item = self.table.item(self.table.currentRow(), 0)
        report = item.data(Qt.ItemDataRole.UserRole) if item is not None else None
        return report if isinstance(report, SymbolReport) else None

    def _jump(self) -> None:
        report = self._selected()
        if report is None or not report.uses: return
        use = report.uses[0]
        self._locate(use.story_id, use.node_id)
        self.accept()

    def _references(self) -> None:
        report = self._selected()
        if report is None: return
        kind = "variable" if report.kind == "flow_variable" else "flag"
        ReferenceDialog(
            self._stories, ReferenceTarget(kind, report.name), self._locate,
            self, manifest=self._manifest,
        ).exec()
