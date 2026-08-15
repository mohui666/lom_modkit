# -*- coding: utf-8 -*-
"""体检报告窗口。"""

from __future__ import annotations

from collections.abc import Callable

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QDialog,
    QDialogButtonBox,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QMessageBox,
    QPushButton,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
)

from i18n import t
from preflight import PreflightIssue


class PreflightDialog(QDialog):
    def __init__(
        self,
        issues: list[PreflightIssue],
        on_locate: Callable[[PreflightIssue], None],
        on_fix: Callable[[], tuple[list[str], list[PreflightIssue]]],
        parent=None,
    ):
        super().__init__(parent)
        self._issues = list(issues)
        self._on_locate = on_locate
        self._on_fix = on_fix
        self.setWindowTitle(t("preflight.title"))
        self.resize(920, 560)

        layout = QVBoxLayout(self)
        self.summary = QLabel()
        self.summary.setWordWrap(True)
        layout.addWidget(self.summary)

        self.table = QTableWidget(0, 4)
        self.table.setHorizontalHeaderLabels(
            [
                t("preflight.col.level"),
                t("preflight.col.story"),
                t("preflight.col.step"),
                t("preflight.col.issue"),
            ]
        )
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QTableWidget.SelectionMode.SingleSelection)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self.table.setWordWrap(True)
        self.table.verticalHeader().setVisible(False)
        for column in (0, 1, 2):
            self.table.horizontalHeader().setSectionResizeMode(
                column, QHeaderView.ResizeMode.ResizeToContents
            )
        self.table.horizontalHeader().setSectionResizeMode(
            3, QHeaderView.ResizeMode.Stretch
        )
        self.table.cellDoubleClicked.connect(lambda _row, _col: self._locate())
        layout.addWidget(self.table, 1)

        hint = QLabel(t("preflight.hint"))
        hint.setWordWrap(True)
        layout.addWidget(hint)

        actions = QHBoxLayout()
        self.locate_btn = QPushButton(t("preflight.locate"))
        self.fix_btn = QPushButton(t("preflight.fix"))
        self.locate_btn.clicked.connect(self._locate)
        self.fix_btn.clicked.connect(self._fix)
        actions.addWidget(self.locate_btn)
        actions.addWidget(self.fix_btn)
        actions.addStretch(1)
        layout.addLayout(actions)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        buttons.rejected.connect(self.reject)
        close_btn = buttons.button(QDialogButtonBox.StandardButton.Close)
        if close_btn is not None:
            close_btn.setText(t("help.close"))
        layout.addWidget(buttons)
        self._refresh_table()

    @property
    def issues(self) -> list[PreflightIssue]:
        return list(self._issues)

    def _refresh_table(self) -> None:
        errors = sum(issue.severity == "error" for issue in self._issues)
        warnings = len(self._issues) - errors
        if not self._issues:
            self.summary.setText(t("preflight.ok"))
        else:
            self.summary.setText(t("preflight.summary", errors=errors, warnings=warnings))
        self.table.setRowCount(len(self._issues))
        for row, issue in enumerate(self._issues):
            values = (
                issue.severity_text,
                issue.story_id or "—",
                issue.node_id or "—",
                issue.message,
            )
            for column, value in enumerate(values):
                item = QTableWidgetItem(value)
                item.setData(Qt.ItemDataRole.UserRole, row)
                self.table.setItem(row, column, item)
        if self._issues:
            self.table.selectRow(0)
        self.table.resizeRowsToContents()
        self.locate_btn.setEnabled(bool(self._issues))
        self.fix_btn.setEnabled(bool(self._issues))

    def _selected_issue(self) -> PreflightIssue | None:
        row = self.table.currentRow()
        return self._issues[row] if 0 <= row < len(self._issues) else None

    def _locate(self) -> None:
        issue = self._selected_issue()
        if issue is None or not issue.story_id:
            return
        self.accept()
        self._on_locate(issue)

    def _fix(self) -> None:
        fixes, issues = self._on_fix()
        self._issues = list(issues)
        self._refresh_table()
        if fixes:
            QMessageBox.information(
                self,
                "自动修复完成",
                "已完成以下安全修复：\n\n" + "\n".join(f"• {item}" for item in fixes),
            )
        else:
            QMessageBox.information(
                self,
                "没有可自动修复的项目",
                "剩余问题需要你决定具体文字、图片或跳转目标。双击问题可以直接定位。",
            )
