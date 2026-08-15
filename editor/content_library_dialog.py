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
    QWidget,
)

from i18n import t
from content_registry import (
    ContentRegistryError,
    default_namespace,
    list_contents,
    register_audio,
    register_character,
    remove,
    repository_root,
    set_default_namespace,
    suggest_content_id,
)


def _kind_labels() -> dict[str, str]:
    return {
        "music": t("library.kind.music"),
        "sound": t("library.kind.sound"),
        "env": t("library.kind.env"),
    }


class ContentLibraryDialog(QDialog):
    def __init__(self, stories: dict | None = None, parent=None):
        super().__init__(parent)
        self._stories = stories or {}
        self.setWindowTitle(t("library.title"))
        self.resize(760, 460)

        layout = QVBoxLayout(self)
        intro = QLabel(t("library.intro"))
        intro.setWordWrap(True)
        layout.addWidget(intro)

        path = QLabel(t("library.path", path=repository_root()))
        path.setWordWrap(True)
        path.setProperty("context_help", True)
        layout.addWidget(path)

        self.table = QTableWidget(0, 4)
        self.table.setHorizontalHeaderLabels(
            [
                t("library.col.name"),
                t("library.col.id"),
                t("library.col.kind"),
                t("library.col.file"),
            ]
        )
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
        import_btn = QPushButton(t("library.import"))
        import_char_btn = QPushButton(t("library.import_character"))
        delete_btn = QPushButton(t("library.delete"))
        import_btn.clicked.connect(self._import_audio)
        import_char_btn.clicked.connect(self._import_character)
        delete_btn.clicked.connect(self._delete_selected)
        buttons.addWidget(import_btn)
        buttons.addWidget(import_char_btn)
        buttons.addWidget(delete_btn)
        buttons.addStretch(1)
        layout.addLayout(buttons)

        close = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        close.rejected.connect(self.reject)
        close.accepted.connect(self.accept)
        layout.addWidget(close)

        self._reload()

    def _reload(self) -> None:
        records = list_contents()
        self.table.setRowCount(0)
        for rec in records:
            row = self.table.rowCount()
            self.table.insertRow(row)
            self.table.setItem(row, 0, QTableWidgetItem(rec.name))
            self.table.setItem(row, 1, QTableWidgetItem(rec.ref))
            if rec.type == "character":
                kind = t("library.kind.character")
                extra = "、".join(rec.portrait_ids())
            else:
                kind = _kind_labels().get(rec.audio_kind or "", rec.audio_kind or "")
                extra = rec.main_file
            self.table.setItem(row, 2, QTableWidgetItem(kind))
            self.table.setItem(row, 3, QTableWidgetItem(extra))
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

    def _import_character(self) -> None:
        dlg = _ImportCharacterDialog(self)
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return
        try:
            rec = register_character(
                dlg.portraits(),
                dlg.content_id(),
                dlg.display_name(),
            )
        except ContentRegistryError as exc:
            QMessageBox.critical(self, t("library.import_char_fail"), str(exc))
            return
        set_default_namespace(dlg.namespace())
        self._reload()
        QMessageBox.information(
            self,
            "已导入",
            "编号：%s\n在登场/对白步骤的人物列表里选择即可。" % rec.ref,
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
        self.setWindowTitle(t("library.import_title"))
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


class _ImportCharacterDialog(QDialog):
    """导入自定义角色：显示名、ID、至少 normal + 可选更多表情。"""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle(t("library.import_char_title"))
        self.resize(560, 360)
        self._rows: list[tuple[QLineEdit, QLabel, Path | None]] = []
        layout = QVBoxLayout(self)
        form = QFormLayout()
        suggested = suggest_content_id("luoxue")
        ns, local = suggested.split(".", 1)
        self._name = QLineEdit("洛雪")
        self._namespace = QLineEdit(ns)
        self._local = QLineEdit(local)
        form.addRow("显示名称", self._name)
        form.addRow("命名空间", self._namespace)
        form.addRow("内部名称", self._local)
        layout.addLayout(form)
        self._hint = QLabel("完整编号将是 user:%s.%s" % (ns, local))
        self._hint.setWordWrap(True)
        self._hint.setProperty("context_help", True)
        layout.addWidget(self._hint)
        self._namespace.textChanged.connect(self._refresh_hint)
        self._local.textChanged.connect(self._refresh_hint)

        layout.addWidget(QLabel(t("library.char_portraits")))
        self._portrait_box = QVBoxLayout()
        layout.addLayout(self._portrait_box)
        self._add_portrait_row("normal", required=True)
        self._add_portrait_row("happy")
        add_btn = QPushButton(t("library.add_portrait"))
        add_btn.clicked.connect(lambda: self._add_portrait_row(""))
        layout.addWidget(add_btn)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self._accept)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)

    def _refresh_hint(self) -> None:
        self._hint.setText(
            "完整编号将是 user:%s.%s"
            % (self.namespace(), self._local.text().strip().lower())
        )

    def _add_portrait_row(self, key: str, required: bool = False) -> None:
        row = QWidget()
        box = QHBoxLayout(row)
        box.setContentsMargins(0, 0, 0, 0)
        key_edit = QLineEdit(key)
        key_edit.setPlaceholderText("normal")
        if required:
            key_edit.setReadOnly(True)
        path_label = QLabel(t("library.no_file"))
        path_label.setMinimumWidth(180)
        pick = QPushButton(t("library.choose_image"))

        def choose(_checked=False, label=path_label, holder=row) -> None:
            path, _ = QFileDialog.getOpenFileName(
                self,
                t("library.choose_image"),
                str(Path.home()),
                "图片 (*.png *.jpg *.jpeg)",
            )
            if not path:
                return
            holder.setProperty("source_path", path)
            label.setText(Path(path).name)

        pick.clicked.connect(choose)
        box.addWidget(key_edit, 1)
        box.addWidget(path_label, 2)
        box.addWidget(pick)
        self._portrait_box.addWidget(row)
        self._rows.append((key_edit, path_label, None))

    def display_name(self) -> str:
        return self._name.text().strip()

    def namespace(self) -> str:
        return self._namespace.text().strip().lower() or default_namespace()

    def content_id(self) -> str:
        return "%s.%s" % (self.namespace(), self._local.text().strip().lower())

    def portraits(self) -> dict[str, Path]:
        result: dict[str, Path] = {}
        for i in range(self._portrait_box.count()):
            widget = self._portrait_box.itemAt(i).widget()
            if widget is None:
                continue
            key_edit = widget.findChild(QLineEdit)
            if key_edit is None:
                continue
            key = key_edit.text().strip()
            path = widget.property("source_path")
            if not key or not path:
                continue
            result[key] = Path(str(path))
        return result

    def _accept(self) -> None:
        portraits = self.portraits()
        if "normal" not in portraits:
            QMessageBox.warning(self, t("library.import_char_fail"), "必须选择 normal 默认立绘。")
            return
        self.accept()
