# -*- coding: utf-8 -*-
"""用户内容库对话框：导入/查看/删除本地自定义音频。"""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFileDialog,
    QFormLayout,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPushButton,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
)

from content_registry import (
    ContentRegistryError,
    default_namespace,
    list_contents,
    register_audio,
    remove,
    repository_root,
    set_default_namespace,
    suggest_content_id,
)


_KIND_LABELS = {
    "music": "音乐",
    "sound": "音效",
    "env": "环境音",
}


class ContentLibraryDialog(QDialog):
    def __init__(self, stories: dict | None = None, parent=None):
        super().__init__(parent)
        self._stories = stories or {}
        self.setWindowTitle("用户内容库")
        self.resize(760, 460)

        layout = QVBoxLayout(self)
        intro = QLabel(
            "导入的音频会得到稳定编号（例如 user:mohui.boss_theme），"
            "剧情里只保存这个编号，不保存你电脑上的文件路径。"
            "导出 Mod 时只会打入当前剧情真正用到的音频。"
        )
        intro.setWordWrap(True)
        layout.addWidget(intro)

        path = QLabel("存放位置：" + str(repository_root()))
        path.setWordWrap(True)
        path.setProperty("context_help", True)
        layout.addWidget(path)

        self.table = QTableWidget(0, 4)
        self.table.setHorizontalHeaderLabels(["显示名称", "编号", "用途", "文件"])
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QTableWidget.SelectionMode.SingleSelection)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        header = self.table.horizontalHeader()
        header.setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        header.setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        header.setSectionResizeMode(2, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(3, QHeaderView.ResizeMode.Stretch)
        layout.addWidget(self.table, 1)

        buttons = QHBoxLayout()
        import_btn = QPushButton("导入音频…")
        delete_btn = QPushButton("删除所选")
        import_btn.clicked.connect(self._import_audio)
        delete_btn.clicked.connect(self._delete_selected)
        buttons.addWidget(import_btn)
        buttons.addWidget(delete_btn)
        buttons.addStretch(1)
        layout.addLayout(buttons)

        close = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        close.rejected.connect(self.reject)
        close.accepted.connect(self.accept)
        layout.addWidget(close)

        self._reload()

    def _reload(self) -> None:
        records = list_contents(content_type="audio")
        self.table.setRowCount(0)
        for rec in records:
            row = self.table.rowCount()
            self.table.insertRow(row)
            self.table.setItem(row, 0, QTableWidgetItem(rec.name))
            self.table.setItem(row, 1, QTableWidgetItem(rec.ref))
            self.table.setItem(row, 2, QTableWidgetItem(_KIND_LABELS.get(rec.audio_kind or "", rec.audio_kind or "")))
            self.table.setItem(row, 3, QTableWidgetItem(rec.main_file))
            self.table.item(row, 1).setData(Qt.ItemDataRole.UserRole, rec.content_id)

    def _selected_id(self) -> str | None:
        row = self.table.currentRow()
        if row < 0:
            return None
        item = self.table.item(row, 1)
        if item is None:
            return None
        value = item.data(Qt.ItemDataRole.UserRole)
        return str(value) if value else None

    def _import_audio(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self,
            "选择音频",
            str(Path.home()),
            "音频 (*.ogg *.wav)",
        )
        if not path:
            return
        dlg = _ImportAudioDialog(Path(path), self)
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return
        try:
            rec = register_audio(
                Path(path),
                dlg.content_id(),
                dlg.display_name(),
                dlg.audio_kind(),
            )
        except ContentRegistryError as exc:
            QMessageBox.critical(self, "无法导入音频", str(exc))
            return
        set_default_namespace(dlg.namespace())
        self._reload()
        QMessageBox.information(
            self,
            "已导入",
            "编号：%s\n在音乐或音效步骤里选择即可。" % rec.ref,
        )

    def _delete_selected(self) -> None:
        content_id = self._selected_id()
        if not content_id:
            QMessageBox.information(self, "删除", "请先选中一条用户内容。")
            return
        try:
            remove(content_id, stories=self._stories)
        except ContentRegistryError as exc:
            QMessageBox.warning(self, "无法删除", str(exc))
            return
        self._reload()


class _ImportAudioDialog(QDialog):
    def __init__(self, source: Path, parent=None):
        super().__init__(parent)
        self.setWindowTitle("导入音频")
        layout = QFormLayout(self)
        suggested = suggest_content_id(source.name)
        ns, local = suggested.split(".", 1)

        self._name = QLineEdit(source.stem)
        self._namespace = QLineEdit(ns)
        self._local = QLineEdit(local)
        self._kind = QComboBox()
        self._kind.addItem("音乐（music 步骤）", "music")
        self._kind.addItem("音效（sound 步骤）", "sound")
        self._kind.addItem("环境音（sound 步骤的环境音声道）", "env")
        layout.addRow("显示名称", self._name)
        layout.addRow("命名空间", self._namespace)
        layout.addRow("内部名称", self._local)
        layout.addRow("用途", self._kind)
        hint = QLabel("完整编号将是 user:%s.%s" % (ns, local))
        hint.setWordWrap(True)
        hint.setProperty("context_help", True)
        layout.addRow(hint)
        self._hint = hint
        self._namespace.textChanged.connect(self._refresh_hint)
        self._local.textChanged.connect(self._refresh_hint)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addRow(buttons)

    def _refresh_hint(self) -> None:
        self._hint.setText(
            "完整编号将是 user:%s.%s"
            % (self.namespace(), self._local.text().strip().lower())
        )

    def display_name(self) -> str:
        return self._name.text().strip()

    def namespace(self) -> str:
        return self._namespace.text().strip().lower() or default_namespace()

    def content_id(self) -> str:
        return "%s.%s" % (self.namespace(), self._local.text().strip().lower())

    def audio_kind(self) -> str:
        return str(self._kind.currentData() or "music")
