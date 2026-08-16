# -*- coding: utf-8 -*-
"""Structured variable/flag usage analysis and project-level manager UI."""

from __future__ import annotations

from typing import Callable

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QComboBox, QDialog, QDialogButtonBox, QHBoxLayout, QHeaderView, QLabel,
    QPushButton, QTableWidget, QTableWidgetItem, QVBoxLayout,
)

from i18n import t
from reference_inspector import ReferenceDialog, ReferenceTarget
from symbol_analysis import SymbolReport, SymbolUse, analyze_symbols


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
