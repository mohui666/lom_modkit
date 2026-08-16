# -*- coding: utf-8 -*-
"""用户内容库对话框：导入/查看/删除自定义音频、角色与统一图片。

自定义角色详情用「基础信息 / 立绘 / 语音」页签管理语音归属；
语音文件仍是独立 audio 资源，不写进角色 content.json。
"""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import QSize, Qt
from PySide6.QtGui import QIcon
from PySide6.QtWidgets import (
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFileDialog,
    QFormLayout,
    QHBoxLayout,
    QHeaderView,
    QInputDialog,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPlainTextEdit,
    QPushButton,
    QSlider,
    QSpinBox,
    QTabWidget,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from audio_preview import AudioPreviewError, play_audio_file, stop_audio
from glass_theme import mark_primary
from content_pack import (
    content_pack_defaults,
    export_content_pack,
    import_content_pack,
    inspect_content_pack,
)
from i18n import t
from content_registry import (
    ContentRecord,
    ContentRegistryError,
    default_namespace,
    find_references,
    get,
    list_character_voices,
    list_contents,
    register_audio,
    register_character,
    register_image,
    remove,
    repository_root,
    set_default_namespace,
    suggest_content_id,
    update_audio,
    update_character,
    update_character_intro,
    update_image,
)
import models


def _kind_labels() -> dict[str, str]:
    return {
        "music": t("library.kind.music"),
        "sound": t("library.kind.sound"),
        "env": t("library.kind.env"),
    }


class _ImportContentTypeDialog(QDialog):
    """三类用户内容的统一入口；这里只负责选择类型，不嵌套具体编辑流程。"""

    _KINDS = ("audio", "character", "image")

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle(t("library.import_content_title"))
        self.setModal(True)
        self.resize(460, 210)

        layout = QVBoxLayout(self)
        title = QLabel(t("library.import_content_heading"))
        title.setWordWrap(True)
        title.setProperty("context_help", True)
        layout.addWidget(title)

        form = QFormLayout()
        self.kind_combo = QComboBox()
        for kind in self._KINDS:
            self.kind_combo.addItem(t(f"library.import_kind.{kind}"), kind)
        self.kind_combo.setMinimumHeight(30)
        self.kind_combo.setAccessibleName(t("library.import_content_kind"))
        form.addRow(t("library.import_content_kind"), self.kind_combo)
        layout.addLayout(form)

        self.description = QLabel()
        self.description.setWordWrap(True)
        self.description.setProperty("context_help", True)
        layout.addWidget(self.description)
        layout.addStretch(1)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok
            | QDialogButtonBox.StandardButton.Cancel
        )
        ok = buttons.button(QDialogButtonBox.StandardButton.Ok)
        if ok is not None:
            ok.setText(t("library.import_continue"))
            ok.setDefault(True)
            mark_primary(ok)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)

        self.kind_combo.currentIndexChanged.connect(self._refresh_description)
        self._refresh_description()

    def _refresh_description(self) -> None:
        self.description.setText(
            t(f"library.import_kind.{self.selected_type()}.description")
        )

    def selected_type(self) -> str:
        value = self.kind_combo.currentData()
        return str(value) if value in self._KINDS else "audio"


class ContentLibraryDialog(QDialog):
    def __init__(
        self,
        stories: dict | None = None,
        parent=None,
        editor_data: dict | None = None,
    ):
        super().__init__(parent)
        self._stories = stories or {}
        self._editor_data = editor_data or {}
        self.setWindowTitle(t("library.title"))
        self.resize(920, 600)

        layout = QVBoxLayout(self)
        intro = QLabel(t("library.intro"))
        intro.setWordWrap(True)
        layout.addWidget(intro)

        path = QLabel(t("library.path", path=repository_root()))
        path.setWordWrap(True)
        path.setProperty("context_help", True)
        layout.addWidget(path)

        # 创建内容、跨项目分享与“对所选内容操作”分层放置。试听只在选中
        # 音频时出现，避免把一个条件动作伪装成全局工具栏命令。
        create_actions = QHBoxLayout()
        import_btn = QPushButton(t("library.import_content"))
        import_btn.setObjectName("libraryImportContentButton")
        import_btn.setMinimumHeight(32)
        import_btn.setToolTip(t("library.import_content_tip"))
        mark_primary(import_btn)
        import_pack_btn = QPushButton(t("content_pack.import"))
        export_pack_btn = QPushButton(t("content_pack.export"))
        import_btn.clicked.connect(self._import_content)
        import_pack_btn.clicked.connect(self._import_content_pack)
        export_pack_btn.clicked.connect(self._export_content_pack)
        create_actions.addWidget(import_btn)
        create_actions.addSpacing(10)
        create_actions.addWidget(import_pack_btn)
        create_actions.addWidget(export_pack_btn)
        create_actions.addStretch(1)
        layout.addLayout(create_actions)

        filters = QHBoxLayout()
        self.search_edit = QLineEdit()
        self.search_edit.setPlaceholderText(t("library.search"))
        self.type_filter = QComboBox()
        self.type_filter.addItem(t("library.filter_all"), "")
        self.type_filter.addItem(t("library.filter_character"), "character")
        self.type_filter.addItem(t("library.filter_audio"), "audio")
        self.type_filter.addItem(t("library.filter_image"), "image")
        filters.addWidget(self.search_edit, 1)
        filters.addWidget(self.type_filter)
        layout.addLayout(filters)

        self.table = QTableWidget(0, 5)
        self.table.setHorizontalHeaderLabels(
            [
                t("library.col.name"),
                t("library.col.id"),
                t("library.col.kind"),
                t("library.col.file"),
                t("library.col.usage"),
            ]
        )
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QTableWidget.SelectionMode.SingleSelection)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self.table.setIconSize(QSize(64, 48))
        header = self.table.horizontalHeader()
        header.setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        header.setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        header.setSectionResizeMode(2, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(3, QHeaderView.ResizeMode.Stretch)
        header.setSectionResizeMode(4, QHeaderView.ResizeMode.ResizeToContents)
        layout.addWidget(self.table, 1)

        self.metadata_view = QPlainTextEdit()
        self.metadata_view.setReadOnly(True)
        self.metadata_view.setMaximumHeight(82)
        self.metadata_view.setPlaceholderText(t("library.metadata_empty"))
        layout.addWidget(self.metadata_view)

        self.audio_actions = QWidget()
        audio_row = QHBoxLayout(self.audio_actions)
        audio_row.setContentsMargins(0, 0, 0, 0)
        audio_label = QLabel(t("library.audio_preview_heading"))
        self.preview_btn = QPushButton(t("library.preview_selected_audio"))
        stop_btn = QPushButton(t("library.preview_stop"))
        self.preview_btn.clicked.connect(self._preview_selected)
        stop_btn.clicked.connect(stop_audio)
        audio_row.addWidget(audio_label)
        audio_row.addWidget(self.preview_btn)
        audio_row.addWidget(stop_btn)
        audio_row.addStretch(1)
        self.audio_actions.setVisible(False)
        layout.addWidget(self.audio_actions)

        buttons = QHBoxLayout()
        self.edit_btn = QPushButton(t("library.edit"))
        self.references_btn = QPushButton(t("library.references"))
        self.delete_btn = QPushButton(t("library.delete"))
        self.edit_btn.clicked.connect(self._edit_selected)
        self.delete_btn.clicked.connect(self._delete_selected)
        self.references_btn.clicked.connect(self._show_references)
        self.table.cellDoubleClicked.connect(lambda *_: self._edit_selected())
        buttons.addWidget(self.edit_btn)
        buttons.addWidget(self.references_btn)
        buttons.addWidget(self.delete_btn)
        buttons.addStretch(1)
        layout.addLayout(buttons)

        close = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        close.rejected.connect(self.reject)
        close.accepted.connect(self.accept)
        layout.addWidget(close)

        self._all_records: list[ContentRecord] = []
        self.search_edit.textChanged.connect(self._apply_filters)
        self.type_filter.currentIndexChanged.connect(self._apply_filters)
        self.table.itemSelectionChanged.connect(self._update_metadata)
        self._reload()
        self._update_metadata()

    def closeEvent(self, event) -> None:
        stop_audio()
        super().closeEvent(event)

    def _reload(self) -> None:
        self._all_records = list_contents()
        self._apply_filters()

    def _apply_filters(self, *_args) -> None:
        query = self.search_edit.text().strip().casefold()
        wanted_type = str(self.type_filter.currentData() or "")
        self.table.setRowCount(0)
        for rec in self._all_records:
            if wanted_type and rec.type != wanted_type:
                continue
            searchable = " ".join(
                [
                    rec.name, rec.ref, rec.type, rec.audio_kind or "",
                    rec.main_file, rec.character or "", rec.title or "",
                    " ".join(rec.portrait_ids()) if rec.type == "character" else "",
                ]
            ).casefold()
            if query and query not in searchable:
                continue
            row = self.table.rowCount()
            self.table.insertRow(row)
            name_item = QTableWidgetItem(rec.name)
            if rec.type in ("character", "image"):
                preview_file = rec.main_file
                name_item.setIcon(QIcon(str(rec.folder / preview_file)))
                self.table.setRowHeight(row, 52)
            self.table.setItem(row, 0, name_item)
            self.table.setItem(row, 1, QTableWidgetItem(rec.ref))
            if rec.type == "character":
                kind = t("library.kind.character")
                extra = "、".join(rec.portrait_ids())
            elif rec.type == "image":
                kind = t("library.kind.image")
                extra = rec.main_file
            else:
                kind = _kind_labels().get(rec.audio_kind or "", rec.audio_kind or "")
                extra = rec.main_file
                if rec.character:
                    extra = "%s · %s" % (extra, rec.character)
            self.table.setItem(row, 2, QTableWidgetItem(kind))
            self.table.setItem(row, 3, QTableWidgetItem(extra))
            refs = find_references(rec.content_id, self._stories)
            usage = (
                t("library.usage.used", count=len(refs))
                if refs else t("library.usage.unused")
            )
            usage_item = QTableWidgetItem(usage)
            usage_item.setData(Qt.ItemDataRole.UserRole, len(refs))
            self.table.setItem(row, 4, usage_item)
            self.table.item(row, 1).setData(Qt.ItemDataRole.UserRole, rec.content_id)
        self._update_metadata()

    def _selected_record(self) -> ContentRecord | None:
        content_id = self._selected_id()
        if not content_id:
            return None
        try:
            return get(content_id)
        except ContentRegistryError:
            return None

    def _update_metadata(self) -> None:
        rec = self._selected_record()
        selected = rec is not None
        self.edit_btn.setEnabled(selected)
        self.references_btn.setEnabled(selected)
        self.delete_btn.setEnabled(selected)
        self.audio_actions.setVisible(bool(rec is not None and rec.type == "audio"))
        if rec is None or rec.type != "audio":
            stop_audio()
        if rec is None:
            self.metadata_view.clear()
            return
        parts = [
            "type=%s" % rec.type, "id=%s" % rec.ref,
            "name=%s" % rec.name, "file=%s" % rec.main_file,
        ]
        if rec.audio_kind:
            parts.append("audio_kind=%s" % rec.audio_kind)
        if rec.character:
            parts.append("character=%s" % rec.character)
        if rec.type == "character":
            parts.append("portraits=%s" % ", ".join(rec.portrait_ids()))
            parts.append("scale=%s%%" % rec.scale)
            parts.append("art_facing=%s" % rec.art_facing)
            if rec.title:
                parts.append("title=%s" % rec.title)
            if rec.intro:
                parts.append("intro=yes")
        self.metadata_view.setPlainText(" · ".join(parts))

    def _import_content(self) -> None:
        dialog = _ImportContentTypeDialog(self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        handlers = {
            "audio": self._import_audio,
            "character": self._import_character,
            "image": self._import_image,
        }
        handlers[dialog.selected_type()]()

    def _preview_selected(self) -> None:
        rec = self._selected_record()
        if rec is None:
            QMessageBox.information(
                self, t("library.preview"), t("library.select_content")
            )
            return
        if rec.type != "audio":
            QMessageBox.information(
                self, t("library.preview"), t("library.preview_audio_only")
            )
            return
        try:
            play_audio_file(rec.folder / rec.main_file)
        except (ContentRegistryError, AudioPreviewError) as exc:
            QMessageBox.warning(self, t("library.preview_fail"), str(exc))

    def _show_references(self) -> None:
        rec = self._selected_record()
        if rec is None:
            QMessageBox.information(
                self, t("library.references"), t("library.select_content")
            )
            return
        refs = find_references(rec.content_id, self._stories)
        lines = [
            "%s → %s (%s.%s)"
            % (
                item.get("story_id", "?"), item.get("node_id", "?"),
                item.get("node_type", "?"), item.get("field", "?"),
            )
            for item in refs
        ] or [t("library.no_references")]
        dlg = QDialog(self)
        dlg.setWindowTitle(t("library.references_title", id=rec.ref))
        dlg.resize(560, 320)
        box = QVBoxLayout(dlg)
        view = QPlainTextEdit("\n".join(lines))
        view.setReadOnly(True)
        box.addWidget(view)
        close = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        close.rejected.connect(dlg.reject)
        box.addWidget(close)
        dlg.exec()

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
        dlg = _ImportAudioDialog(Path(path), self, editor_data=self._editor_data)
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return
        try:
            rec = register_audio(
                Path(path),
                dlg.content_id(),
                dlg.display_name(),
                dlg.audio_kind(),
                character=dlg.character(),
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
        dlg = _ImportCharacterDialog(
            self,
            stories=self._stories,
            editor_data=self._editor_data,
        )
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return
        try:
            rec = register_character(
                dlg.portraits(),
                dlg.content_id(),
                dlg.display_name(),
                title=dlg.character_title(),
                scale=dlg.character_scale(),
                art_facing=dlg.character_art_facing(),
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

    def _import_image(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self,
            t("library.import_image_title"),
            str(Path.home()),
            "图片 (*.png *.jpg *.jpeg)",
        )
        if not path:
            return
        dlg = _ImportImageDialog(Path(path), self)
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return
        try:
            rec = register_image(
                Path(path), dlg.content_id(), dlg.display_name()
            )
        except ContentRegistryError as exc:
            QMessageBox.critical(self, t("library.import_image_fail"), str(exc))
            return
        set_default_namespace(dlg.namespace())
        self._reload()
        QMessageBox.information(
            self,
            t("library.import_image_title"),
            "编号：%s\n背景、CG 与 Overlay 共用这条图片内容。" % rec.ref,
        )

    def _import_content_pack(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self,
            t("content_pack.import_title"),
            str(Path.home()),
            "lom_modkit Content Pack (*.lomcontent)",
        )
        if not path:
            return
        try:
            preview = inspect_content_pack(path)
        except ContentRegistryError as exc:
            QMessageBox.critical(self, t("content_pack.import_fail"), str(exc))
            return
        if preview.collision_type is not None:
            QMessageBox.warning(
                self,
                t("content_pack.collision_title"),
                t(
                    "content_pack.collision",
                    id="user:" + preview.content_id,
                    type=preview.collision_type,
                ),
            )
            return
        answer = QMessageBox.question(
            self,
            t("content_pack.import_title"),
            t(
                "content_pack.confirm",
                name=preview.name,
                id="user:" + preview.content_id,
                type=preview.content_type,
                version=preview.version,
                author=preview.author,
                license=preview.license,
                count=len(preview.files),
                dependencies=(
                    ", ".join("user:" + item for item in preview.dependencies)
                    or t("content_pack.none")
                ),
                missing=(
                    ", ".join("user:" + item for item in preview.missing_dependencies)
                    or t("content_pack.none")
                ),
            ),
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
            QMessageBox.StandardButton.No,
        )
        if answer != QMessageBox.StandardButton.Yes:
            return
        try:
            imported = import_content_pack(path)
        except ContentRegistryError as exc:
            QMessageBox.critical(self, t("content_pack.import_fail"), str(exc))
            return
        self._reload()
        QMessageBox.information(
            self,
            t("content_pack.import_title"),
            t("content_pack.import_done", id="user:" + imported.content_id),
        )

    def _export_content_pack(self) -> None:
        rec = self._selected_record()
        if rec is None:
            QMessageBox.information(
                self, t("content_pack.export"), t("library.select_content")
            )
            return
        defaults = content_pack_defaults(rec.content_id)
        dialog = _ContentPackExportDialog(rec, defaults, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        path, _ = QFileDialog.getSaveFileName(
            self,
            t("content_pack.export_title"),
            str(Path.home() / f"{rec.content_id}-{dialog.version()}.lomcontent"),
            "lom_modkit Content Pack (*.lomcontent)",
        )
        if not path:
            return
        if not path.lower().endswith(".lomcontent"):
            path += ".lomcontent"
        try:
            info = export_content_pack(
                path,
                rec.content_id,
                version=dialog.version(),
                author=dialog.author(),
                license_name=dialog.license_name(),
                dependencies=dialog.dependencies(),
            )
        except ContentRegistryError as exc:
            QMessageBox.critical(self, t("content_pack.export_fail"), str(exc))
            return
        QMessageBox.information(
            self,
            t("content_pack.export_title"),
            t(
                "content_pack.export_done",
                path=info.path,
                count=len(info.files),
                hash=info.logical_content_hash,
            ),
        )

    def _edit_selected(self) -> None:
        content_id = self._selected_id()
        if not content_id:
            QMessageBox.information(self, t("library.edit"), "请先选中一条用户内容。")
            return
        try:
            rec = get(content_id)
        except ContentRegistryError as exc:
            QMessageBox.warning(self, t("library.edit_fail"), str(exc))
            return
        if rec.type == "audio":
            dlg = _EditAudioDialog(rec, self, editor_data=self._editor_data)
            if dlg.exec() != QDialog.DialogCode.Accepted:
                return
            try:
                update_audio(
                    rec.content_id,
                    name=dlg.display_name(),
                    character=dlg.character(),
                )
            except ContentRegistryError as exc:
                QMessageBox.critical(self, t("library.edit_fail"), str(exc))
                return
            self._reload()
            return
        if rec.type == "image":
            name, ok = QInputDialog.getText(
                self, t("library.edit"), t("library.col.name"), text=rec.name
            )
            if not ok:
                return
            try:
                update_image(rec.content_id, name)
            except ContentRegistryError as exc:
                QMessageBox.critical(self, t("library.edit_fail"), str(exc))
                return
            self._reload()
            return
        if rec.type != "character":
            QMessageBox.information(
                self, t("library.edit"), t("library.edit_unsupported")
            )
            return
        dlg = _ImportCharacterDialog(
            self,
            existing=rec,
            stories=self._stories,
            editor_data=self._editor_data,
        )
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return
        try:
            update_character(
                rec.content_id,
                name=dlg.display_name(),
                portraits=dlg.portraits(),
                remove_portraits=dlg.removed_portraits(),
                title=dlg.character_title(),
                scale=dlg.character_scale(),
                art_facing=dlg.character_art_facing(),
            )
        except ContentRegistryError as exc:
            QMessageBox.critical(self, t("library.edit_fail"), str(exc))
            return
        self._reload()

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


class _ContentPackExportDialog(QDialog):
    def __init__(self, rec: ContentRecord, defaults: dict, parent=None):
        super().__init__(parent)
        self.setWindowTitle(t("content_pack.metadata_title"))
        layout = QFormLayout(self)
        identity = QLabel(f"{rec.name}（{rec.ref} · {rec.type}）")
        identity.setWordWrap(True)
        self._version = QLineEdit(defaults.get("version", "1.0.0"))
        self._author = QLineEdit(defaults.get("author", ""))
        self._license = QLineEdit(defaults.get("license", "All Rights Reserved"))
        self._dependencies = QLineEdit(
            ", ".join("user:" + item for item in defaults.get("dependencies", []))
        )
        self._dependencies.setPlaceholderText(t("content_pack.dependencies_hint"))
        layout.addRow(t("content_pack.content"), identity)
        layout.addRow(t("content_pack.version"), self._version)
        layout.addRow(t("content_pack.author"), self._author)
        layout.addRow(t("content_pack.license"), self._license)
        layout.addRow(t("content_pack.dependencies"), self._dependencies)
        hint = QLabel(t("content_pack.hint"))
        hint.setWordWrap(True)
        hint.setProperty("context_help", True)
        layout.addRow(hint)
        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addRow(buttons)

    def version(self) -> str:
        return self._version.text().strip()

    def author(self) -> str:
        return self._author.text().strip()

    def license_name(self) -> str:
        return self._license.text().strip()

    def dependencies(self) -> list[str]:
        text = self._dependencies.text().replace("，", ",").replace(";", ",")
        return [item.strip() for item in text.split(",") if item.strip()]


def _fill_character_combo(
    combo: QComboBox,
    editor_data: dict | None,
    current: str | None = None,
    locked: str | None = None,
) -> None:
    combo.clear()
    combo.addItem(t("library.character_none"), "")
    if locked:
        combo.addItem(locked, locked)
        idx = combo.findData(locked)
        combo.setCurrentIndex(idx if idx >= 0 else 0)
        combo.setEnabled(False)
        return
    custom, official = models.character_combo_items(editor_data or {})
    for item_id, label in custom:
        combo.addItem(label, item_id)
    if custom and official:
        combo.insertSeparator(combo.count())
    for item_id, label in official:
        combo.addItem(label, item_id)
    if current:
        idx = combo.findData(current)
        if idx >= 0:
            combo.setCurrentIndex(idx)
        else:
            combo.addItem(current, current)
            combo.setCurrentIndex(combo.findData(current))


class _ImportAudioDialog(QDialog):
    def __init__(
        self,
        source: Path,
        parent=None,
        *,
        editor_data: dict | None = None,
        audio_kind: str | None = None,
        character: str | None = None,
        lock_character: bool = False,
    ):
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
        if audio_kind in ("music", "sound", "env"):
            idx = self._kind.findData(audio_kind)
            if idx >= 0:
                self._kind.setCurrentIndex(idx)
            self._kind.setEnabled(False)
        self._character = QComboBox()
        _fill_character_combo(
            self._character,
            editor_data,
            current=character,
            locked=character if lock_character else None,
        )
        layout.addRow("显示名称", self._name)
        layout.addRow("命名空间", self._namespace)
        layout.addRow("内部名称", self._local)
        layout.addRow("用途", self._kind)
        layout.addRow(t("library.col.character"), self._character)
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

    def character(self) -> str | None:
        value = self._character.currentData()
        if value not in (None, ""):
            return str(value)
        text = self._character.currentText().strip()
        if not text or text == t("library.character_none"):
            return None
        return text


class _ImportImageDialog(QDialog):
    """统一图片导入：稳定 ID + 显示名称，文件类型由选择器限定。"""

    def __init__(self, source: Path, parent=None):
        super().__init__(parent)
        self.setWindowTitle(t("library.import_image_title"))
        layout = QFormLayout(self)
        suggested = suggest_content_id(source.name)
        ns, local = suggested.split(".", 1)
        self._name = QLineEdit(source.stem)
        self._namespace = QLineEdit(ns)
        self._local = QLineEdit(local)
        layout.addRow(t("library.col.name"), self._name)
        layout.addRow("命名空间", self._namespace)
        layout.addRow("内部名称", self._local)
        self._preview = QLabel()
        self._preview.setPixmap(QIcon(str(source)).pixmap(QSize(240, 135)))
        self._preview.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addRow("缩略图", self._preview)
        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addRow(buttons)

    def display_name(self) -> str:
        return self._name.text().strip()

    def namespace(self) -> str:
        return self._namespace.text().strip().lower() or default_namespace()

    def content_id(self) -> str:
        return "%s.%s" % (self.namespace(), self._local.text().strip().lower())


class _ImportCharacterDialog(QDialog):
    """导入或编辑自定义角色。编辑时编号锁定，可换图、加表情、改显示名。"""

    def __init__(
        self,
        parent=None,
        existing: ContentRecord | None = None,
        stories: dict | None = None,
        editor_data: dict | None = None,
    ):
        super().__init__(parent)
        self._existing = existing
        self._removed: list[str] = []
        self._stories = stories or {}
        self._editor_data = editor_data or {}
        self.setWindowTitle(
            t("library.edit_char_title") if existing else t("library.import_char_title")
        )
        self.resize(620, 460)
        layout = QVBoxLayout(self)
        tabs = QTabWidget()
        layout.addWidget(tabs, 1)

        basic = QWidget()
        basic_layout = QVBoxLayout(basic)
        form = QFormLayout()
        if existing:
            ns, local = existing.content_id.split(".", 1)
            self._name = QLineEdit(existing.name)
            self._title = QLineEdit(existing.title or "")
            self._namespace = QLineEdit(ns)
            self._local = QLineEdit(local)
            self._namespace.setReadOnly(True)
            self._local.setReadOnly(True)
        else:
            suggested = suggest_content_id("luoxue")
            ns, local = suggested.split(".", 1)
            self._name = QLineEdit("洛雪")
            self._title = QLineEdit("")
            self._namespace = QLineEdit(ns)
            self._local = QLineEdit(local)
        form.addRow("显示名称", self._name)
        form.addRow(t("library.char_title"), self._title)
        self._scale = QSlider(Qt.Orientation.Horizontal)
        self._scale.setRange(50, 130)
        self._scale.setTickPosition(QSlider.TickPosition.TicksBelow)
        self._scale.setTickInterval(10)
        self._scale.setSingleStep(1)
        self._scale.setPageStep(5)
        self._scale.setValue(int(existing.scale) if existing else 100)
        self._scale_value = QLabel("%d%%" % self._scale.value())
        self._scale_value.setMinimumWidth(44)
        self._scale.valueChanged.connect(
            lambda value: self._scale_value.setText("%d%%" % value)
        )
        scale_row = QWidget()
        scale_layout = QHBoxLayout(scale_row)
        scale_layout.setContentsMargins(0, 0, 0, 0)
        scale_layout.addWidget(self._scale, 1)
        scale_layout.addWidget(self._scale_value)
        form.addRow(t("library.char_scale"), scale_row)
        scale_hint = QLabel(t("library.char_scale_hint"))
        scale_hint.setWordWrap(True)
        scale_hint.setProperty("context_help", True)
        form.addRow(scale_hint)
        self._art_facing = QComboBox()
        self._art_facing.addItem(t("library.art_facing.left"), "left")
        self._art_facing.addItem(t("library.art_facing.right"), "right")
        current_facing = (existing.art_facing if existing else "left") or "left"
        facing_idx = self._art_facing.findData(current_facing)
        self._art_facing.setCurrentIndex(facing_idx if facing_idx >= 0 else 0)
        form.addRow(t("library.art_facing"), self._art_facing)
        form.addRow("命名空间", self._namespace)
        form.addRow("内部名称", self._local)
        basic_layout.addLayout(form)
        self._hint = QLabel("完整编号将是 user:%s.%s" % (ns, local))
        self._hint.setWordWrap(True)
        self._hint.setProperty("context_help", True)
        basic_layout.addWidget(self._hint)
        self._namespace.textChanged.connect(self._refresh_hint)
        self._local.textChanged.connect(self._refresh_hint)
        basic_layout.addStretch(1)
        tabs.addTab(basic, t("library.tab.basic"))

        portraits = QWidget()
        portraits_layout = QVBoxLayout(portraits)
        portraits_layout.addWidget(QLabel(t("library.char_portraits")))
        self._portrait_box = QVBoxLayout()
        portraits_layout.addLayout(self._portrait_box)
        if existing and existing.portraits:
            for key in existing.portrait_ids():
                self._add_portrait_row(
                    key,
                    required=key == "normal",
                    current_file=(existing.portraits or {}).get(key, ""),
                )
        else:
            self._add_portrait_row("normal", required=True)
            self._add_portrait_row("happy")
        add_btn = QPushButton(t("library.add_portrait"))
        add_btn.clicked.connect(lambda: self._add_portrait_row(""))
        portraits_layout.addWidget(add_btn)
        portraits_layout.addStretch(1)
        tabs.addTab(portraits, t("library.tab.portraits"))

        voices = QWidget()
        voices_layout = QVBoxLayout(voices)
        if existing:
            voices_layout.addWidget(
                _CharacterVoicePage(existing.ref, self._stories, voices)
            )
        else:
            hint = QLabel(t("library.voice_after_create"))
            hint.setWordWrap(True)
            hint.setProperty("context_help", True)
            voices_layout.addWidget(hint)
            voices_layout.addStretch(1)
        tabs.addTab(voices, t("library.tab.voices"))

        intro_tab = QWidget()
        intro_layout = QVBoxLayout(intro_tab)
        if existing:
            intro_layout.addWidget(_CharacterIntroPage(existing, intro_tab))
        else:
            hint = QLabel(t("library.intro_after_create"))
            hint.setWordWrap(True)
            hint.setProperty("context_help", True)
            intro_layout.addWidget(hint)
            intro_layout.addStretch(1)
        tabs.addTab(intro_tab, t("library.tab.intro"))

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

    def _add_portrait_row(
        self, key: str, required: bool = False, current_file: str = ""
    ) -> None:
        row = QWidget()
        box = QHBoxLayout(row)
        box.setContentsMargins(0, 0, 0, 0)
        key_edit = QLineEdit(key)
        key_edit.setPlaceholderText("normal")
        if required:
            key_edit.setReadOnly(True)
        path_label = QLabel(current_file or t("library.no_file"))
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
        if self._existing and key and key != "normal":
            drop = QPushButton(t("library.remove_portrait"))

            def drop_row(_checked=False, holder=row, portrait_key=key) -> None:
                self._removed.append(portrait_key)
                holder.hide()
                holder.setProperty("dropped", True)

            drop.clicked.connect(drop_row)
            box.addWidget(drop)
        self._portrait_box.addWidget(row)

    def display_name(self) -> str:
        return self._name.text().strip()

    def character_title(self) -> str:
        return self._title.text().strip()

    def character_scale(self) -> int:
        return int(self._scale.value())

    def character_art_facing(self) -> str:
        return str(self._art_facing.currentData() or "left")

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
            if widget.property("dropped"):
                continue
            if not key or not path:
                continue
            result[key] = Path(str(path))
        return result

    def removed_portraits(self) -> list[str]:
        return list(self._removed)

    def _accept(self) -> None:
        portraits = self.portraits()
        if self._existing is None and "normal" not in portraits:
            QMessageBox.warning(self, t("library.import_char_fail"), "必须选择 normal 默认立绘。")
            return
        self.accept()

    def closeEvent(self, event) -> None:
        stop_audio()
        super().closeEvent(event)


class _EditAudioDialog(QDialog):
    """改音频显示名和可选角色归属，不改文件。"""

    def __init__(
        self,
        rec: ContentRecord,
        parent=None,
        editor_data: dict | None = None,
        lock_character: str | None = None,
    ):
        super().__init__(parent)
        self._rec = rec
        self.setWindowTitle(t("library.edit_audio_title"))
        layout = QFormLayout(self)
        self._name = QLineEdit(rec.name)
        self._character = QComboBox()
        self._character.setEditable(lock_character is None)
        _fill_character_combo(
            self._character,
            editor_data,
            current=lock_character or rec.character,
            locked=lock_character,
        )
        layout.addRow("显示名称", self._name)
        layout.addRow(t("library.col.character"), self._character)
        hint = QLabel(t("library.character_hint"))
        hint.setWordWrap(True)
        hint.setProperty("context_help", True)
        layout.addRow(hint)
        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addRow(buttons)

    def display_name(self) -> str:
        return self._name.text().strip()

    def character(self) -> str | None:
        value = self._character.currentData()
        if value not in (None, ""):
            return str(value)
        text = self._character.currentText().strip()
        if not text or text == t("library.character_none"):
            return None
        return text


class _CharacterVoicePage(QWidget):
    """自定义角色的语音页：列出归属语音，导入时自动关联当前角色。"""

    def __init__(self, character_ref: str, stories: dict, parent=None):
        super().__init__(parent)
        self._character_ref = character_ref
        self._stories = stories
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        intro = QLabel(t("library.voice_intro"))
        intro.setWordWrap(True)
        layout.addWidget(intro)

        self.table = QTableWidget(0, 3)
        self.table.setHorizontalHeaderLabels(
            [
                t("library.col.name"),
                t("library.col.id"),
                t("library.col.file"),
            ]
        )
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QTableWidget.SelectionMode.SingleSelection)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        header = self.table.horizontalHeader()
        header.setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        header.setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        header.setSectionResizeMode(2, QHeaderView.ResizeMode.Stretch)
        layout.addWidget(self.table, 1)

        buttons = QHBoxLayout()
        import_btn = QPushButton(t("library.import_voice"))
        preview_btn = QPushButton(t("library.preview_voice"))
        edit_btn = QPushButton(t("library.edit_voice"))
        unlink_btn = QPushButton(t("library.unlink_voice"))
        delete_btn = QPushButton(t("library.delete_voice"))
        import_btn.clicked.connect(self._import_voice)
        preview_btn.clicked.connect(self._preview_selected)
        edit_btn.clicked.connect(self._edit_selected)
        unlink_btn.clicked.connect(self._unlink_selected)
        delete_btn.clicked.connect(self._delete_selected)
        self.table.cellDoubleClicked.connect(lambda *_: self._edit_selected())
        buttons.addWidget(import_btn)
        buttons.addWidget(preview_btn)
        buttons.addWidget(edit_btn)
        buttons.addWidget(unlink_btn)
        buttons.addWidget(delete_btn)
        buttons.addStretch(1)
        layout.addLayout(buttons)
        self._reload()

    def _reload(self) -> None:
        records = list_character_voices(self._character_ref)
        self.table.setRowCount(0)
        for rec in records:
            row = self.table.rowCount()
            self.table.insertRow(row)
            self.table.setItem(row, 0, QTableWidgetItem(rec.name))
            self.table.setItem(row, 1, QTableWidgetItem(rec.ref))
            self.table.setItem(row, 2, QTableWidgetItem(rec.main_file))
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

    def _import_voice(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self,
            t("library.choose_voice"),
            str(Path.home()),
            "音频 (*.ogg *.wav)",
        )
        if not path:
            return
        dlg = _ImportAudioDialog(
            Path(path),
            self,
            audio_kind="sound",
            character=self._character_ref,
            lock_character=True,
        )
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return
        try:
            register_audio(
                Path(path),
                dlg.content_id(),
                dlg.display_name(),
                "sound",
                character=self._character_ref,
            )
        except ContentRegistryError as exc:
            QMessageBox.critical(self, t("library.import_fail"), str(exc))
            return
        set_default_namespace(dlg.namespace())
        self._reload()

    def _preview_selected(self) -> None:
        content_id = self._selected_id()
        if not content_id:
            QMessageBox.information(self, t("library.preview_voice"), t("library.select_voice"))
            return
        try:
            rec = get(content_id)
            play_audio_file(rec.folder / rec.main_file)
        except (ContentRegistryError, AudioPreviewError) as exc:
            QMessageBox.warning(self, t("library.preview_fail"), str(exc))

    def _edit_selected(self) -> None:
        content_id = self._selected_id()
        if not content_id:
            QMessageBox.information(self, t("library.edit_voice"), t("library.select_voice"))
            return
        try:
            rec = get(content_id)
        except ContentRegistryError as exc:
            QMessageBox.warning(self, t("library.edit_fail"), str(exc))
            return
        dlg = _EditAudioDialog(rec, self, lock_character=self._character_ref)
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return
        try:
            update_audio(rec.content_id, name=dlg.display_name(), character=self._character_ref)
        except ContentRegistryError as exc:
            QMessageBox.critical(self, t("library.edit_fail"), str(exc))
            return
        self._reload()

    def _unlink_selected(self) -> None:
        content_id = self._selected_id()
        if not content_id:
            QMessageBox.information(self, t("library.unlink_voice"), t("library.select_voice"))
            return
        try:
            update_audio(content_id, character=None)
        except ContentRegistryError as exc:
            QMessageBox.warning(self, t("library.edit_fail"), str(exc))
            return
        self._reload()

    def _delete_selected(self) -> None:
        content_id = self._selected_id()
        if not content_id:
            QMessageBox.information(self, t("library.delete_voice"), t("library.select_voice"))
            return
        try:
            remove(content_id, stories=self._stories)
        except ContentRegistryError as exc:
            QMessageBox.warning(self, t("library.delete_fail"), str(exc))
            return
        self._reload()


class _CharacterIntroPage(QWidget):
    """自定义角色的介绍卡：一对一资料，图片放在角色目录里。"""

    def __init__(self, rec: ContentRecord, parent=None):
        super().__init__(parent)
        self._content_id = rec.content_id
        self._image_path: Path | None = None
        intro = rec.intro or {}
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        hint = QLabel(t("library.intro_card_hint"))
        hint.setWordWrap(True)
        hint.setProperty("context_help", True)
        layout.addWidget(hint)
        form = QFormLayout()
        self._title = QLineEdit(str(intro.get("title") or ""))
        self._name = QLineEdit(str(intro.get("name") or rec.name))
        self._text = QPlainTextEdit(str(intro.get("text") or ""))
        self._text.setMinimumHeight(90)
        self._text.setMaximumHeight(140)
        self._image_label = QLabel(str(intro.get("image") or t("library.no_file")))
        pick = QPushButton(t("library.choose_image"))
        pick.clicked.connect(self._pick_image)
        img_row = QWidget()
        img_box = QHBoxLayout(img_row)
        img_box.setContentsMargins(0, 0, 0, 0)
        img_box.addWidget(self._image_label, 1)
        img_box.addWidget(pick)
        self._scale = QSpinBox()
        self._scale.setRange(40, 160)
        self._scale.setValue(int(intro.get("image_scale") or 100))
        self._scale.setSuffix(" %")
        self._x = QSpinBox()
        self._x.setRange(-30, 30)
        self._x.setValue(int(intro.get("image_x") or 0))
        self._y = QSpinBox()
        self._y.setRange(-30, 30)
        self._y.setValue(int(intro.get("image_y") or 0))
        form.addRow(t("field.title"), self._title)
        form.addRow(t("field.name"), self._name)
        form.addRow(t("field.text"), self._text)
        form.addRow(t("field.image"), img_row)
        form.addRow(t("field.image_scale"), self._scale)
        form.addRow(t("field.image_x"), self._x)
        form.addRow(t("field.image_y"), self._y)
        layout.addLayout(form)
        buttons = QHBoxLayout()
        save = QPushButton(t("library.save_intro"))
        clear = QPushButton(t("library.clear_intro"))
        save.clicked.connect(self._save)
        clear.clicked.connect(self._clear)
        buttons.addWidget(save)
        buttons.addWidget(clear)
        buttons.addStretch(1)
        layout.addLayout(buttons)
        layout.addStretch(1)

    def _pick_image(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self,
            t("library.choose_image"),
            str(Path.home()),
            "图片 (*.png *.jpg *.jpeg)",
        )
        if not path:
            return
        self._image_path = Path(path)
        self._image_label.setText(Path(path).name)

    def _save(self) -> None:
        try:
            rec = update_character_intro(
                self._content_id,
                title=self._title.text(),
                name=self._name.text(),
                text=self._text.toPlainText(),
                image=self._image_path,
                image_scale=self._scale.value(),
                image_x=self._x.value(),
                image_y=self._y.value(),
            )
        except ContentRegistryError as exc:
            QMessageBox.critical(self, t("library.edit_fail"), str(exc))
            return
        self._image_path = None
        if rec.intro and rec.intro.get("image"):
            self._image_label.setText(str(rec.intro["image"]))
        QMessageBox.information(self, t("library.save_intro"), t("library.intro_saved"))

    def _clear(self) -> None:
        try:
            update_character_intro(self._content_id, clear=True)
        except ContentRegistryError as exc:
            QMessageBox.warning(self, t("library.edit_fail"), str(exc))
            return
        self._title.clear()
        self._text.clear()
        self._image_path = None
        self._image_label.setText(t("library.no_file"))
        self._scale.setValue(100)
        self._x.setValue(0)
        self._y.setValue(0)
