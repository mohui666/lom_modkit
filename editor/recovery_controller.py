# -*- coding: utf-8 -*-
"""Crash-recovery dialog and restoration workflow."""

from __future__ import annotations

import copy
import json

from PySide6.QtWidgets import (
    QAbstractItemView, QDialog, QDialogButtonBox, QHBoxLayout, QHeaderView,
    QLabel, QMessageBox, QPlainTextEdit, QPushButton, QTableWidget,
    QTableWidgetItem, QVBoxLayout,
)

import models
from glass_theme import mark_primary
from i18n import t
from recovery_store import (
    RecoveryCandidate, RecoveryError, finish_candidate, list_recovery_candidates,
)


class RecoveryDialog(QDialog):
    """Choose, inspect, restore or discard an abnormal-exit snapshot."""

    def __init__(self, candidates: list[RecoveryCandidate], parent=None):
        super().__init__(parent)
        self.candidates = candidates
        self.action = "later"
        self.candidate: RecoveryCandidate | None = None
        self.setWindowTitle(t("recovery.title"))
        self.resize(860, 430)
        layout = QVBoxLayout(self)
        layout.addWidget(QLabel(
            "上次编辑器可能异常退出。恢复只载入内存，不会自动覆盖原项目文件。"
        ))
        self.table = QTableWidget(len(candidates), 4)
        self.table.setHorizontalHeaderLabels((
            t("recovery.saved_at"), t("recovery.project"),
            t("recovery.source"), t("recovery.chapter"),
        ))
        self.table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)
        self.table.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        for row, candidate in enumerate(candidates):
            values = (
                candidate.saved_at, candidate.project_name,
                candidate.source_path or "未命名项目", str(len(candidate.story_ids)),
            )
            for column, value in enumerate(values):
                item = QTableWidgetItem(value)
                item.setToolTip(value)
                self.table.setItem(row, column, item)
        header = self.table.horizontalHeader()
        header.setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(1, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(2, QHeaderView.ResizeMode.Stretch)
        header.setSectionResizeMode(3, QHeaderView.ResizeMode.ResizeToContents)
        if candidates:
            self.table.selectRow(0)
        self.table.itemDoubleClicked.connect(lambda _item: self._inspect())
        layout.addWidget(self.table, stretch=1)
        buttons = QHBoxLayout()
        inspect_btn = QPushButton(t("recovery.inspect"))
        inspect_btn.clicked.connect(self._inspect)
        buttons.addWidget(inspect_btn)
        buttons.addStretch(1)
        discard_btn = QPushButton(t("recovery.discard"))
        discard_btn.clicked.connect(lambda: self._finish("discard"))
        buttons.addWidget(discard_btn)
        later_btn = QPushButton(t("recovery.later"))
        later_btn.clicked.connect(self.reject)
        buttons.addWidget(later_btn)
        restore_btn = QPushButton(t("recovery.restore"))
        mark_primary(restore_btn)
        restore_btn.clicked.connect(lambda: self._finish("restore"))
        buttons.addWidget(restore_btn)
        layout.addLayout(buttons)

    def _selected(self) -> RecoveryCandidate | None:
        row = self.table.currentRow()
        return self.candidates[row] if 0 <= row < len(self.candidates) else None

    def _finish(self, action: str) -> None:
        candidate = self._selected()
        if candidate is not None:
            self.action = action
            self.candidate = candidate
            self.accept()

    def _inspect(self) -> None:
        candidate = self._selected()
        if candidate is None:
            return
        dialog = QDialog(self)
        dialog.setWindowTitle(t("recovery.inspect_title"))
        dialog.resize(780, 600)
        layout = QVBoxLayout(dialog)
        layout.addWidget(QLabel(
            f"项目：{candidate.project_name}　当前章节：{candidate.current_story_id}\n"
            f"来源：{candidate.source_path or '未命名项目'}"
        ))
        preview = QPlainTextEdit()
        preview.setReadOnly(True)
        text = json.dumps(candidate.document, ensure_ascii=False, indent=2)
        if len(text) > 500_000:
            text = text[:500_000] + "\n\n……检查预览已截断，恢复数据本身未修改。"
        preview.setPlainText(text)
        layout.addWidget(preview, stretch=1)
        close = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        close.rejected.connect(dialog.reject)
        layout.addWidget(close)
        dialog.exec()


class RecoveryControllerMixin:
    """Startup recovery workflow separated from project file operations."""

    def restore_abnormal_session(self) -> bool:
        session = self._recovery_session
        if session is None:
            return False
        while True:
            candidates = list_recovery_candidates(
                session.root, exclude_session_id=session.session_id
            )
            if not candidates:
                return False
            dialog = RecoveryDialog(candidates, self)
            if dialog.exec() != QDialog.DialogCode.Accepted or dialog.candidate is None:
                return False
            candidate = dialog.candidate
            if dialog.action == "discard":
                try:
                    finish_candidate(candidate, "discarded")
                except RecoveryError as exc:
                    QMessageBox.warning(
                        self, t("app.title"), t("error.recovery_discard", error=exc)
                    )
                    return False
                continue
            if dialog.action == "restore":
                return self._restore_recovery_candidate(candidate)

    def _restore_recovery_candidate(self, candidate: RecoveryCandidate) -> bool:
        document = candidate.document
        try:
            stories = copy.deepcopy(document["stories"])
            current_id = str(document["current_story_id"])
            if current_id not in stories:
                raise RecoveryError("恢复快照当前章节不存在")
            repaired = models.normalize_character_ids(stories, self.editor_data)
            manifest = document.get("manifest")
            if not isinstance(manifest, dict):
                manifest = {}
        except Exception as exc:
            QMessageBox.critical(
                self, t("app.title"), t("error.recovery_load", error=exc)
            )
            return False

        self._stories = stories
        self._current_id = current_id
        self.manifest = copy.deepcopy(manifest)
        self.manifest_base = copy.deepcopy(manifest)
        self._story_paths = {story_id: None for story_id in stories}
        self._set_project_source(candidate.source_kind, None)
        self._saved_snapshot = {}
        self._undo_stack.clear()
        self._redo_stack.clear()
        self._pending_before = None
        self._commit_timer.stop()
        self._refresh_all()
        self._set_dirty(True)
        session = self._recovery_session
        try:
            if session is None:
                raise RecoveryError("当前恢复会话不可用")
            session.write_snapshot(
                stories=self._snapshot(), current_story_id=self._current_id,
                manifest=copy.deepcopy(self.manifest),
                story_paths=dict(self._story_paths), source_kind=self._source_kind,
                source_path=None,
            )
            finish_candidate(candidate, "recovered")
        except RecoveryError as exc:
            QMessageBox.warning(
                self, t("app.title"),
                "内容已载入内存，但无法转移恢复副本：%s\n"
                "请立即使用“另存为”或“导出 Mod”。" % exc,
            )
        note = f"；已修复 {repaired} 个人物内部 ID" if repaired else ""
        self.statusBar().showMessage(
            "已从异常退出副本恢复（尚未覆盖任何正式文件）" + note, 7000
        )
        return True
