# -*- coding: utf-8 -*-
"""Qt presentation for the read-only package inspector."""

from __future__ import annotations

import html

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QDialog,
    QDialogButtonBox,
    QHeaderView,
    QLabel,
    QPlainTextEdit,
    QSplitter,
    QTableWidget,
    QTableWidgetItem,
    QTextBrowser,
    QVBoxLayout,
    QWidget,
)

from i18n import t
from package_inspector import PackageInspection


def _human_size(size: int) -> str:
    value = float(size)
    for unit in ("B", "KiB", "MiB", "GiB"):
        if value < 1024 or unit == "GiB":
            return f"{int(value)} {unit}" if unit == "B" else f"{value:.1f} {unit}"
        value /= 1024
    return f"{size} B"


class PackageInspectorDialog(QDialog):
    def __init__(self, inspection: PackageInspection, parent=None):
        super().__init__(parent)
        self.inspection = inspection
        self.setWindowTitle(t("inspector.title"))
        self.resize(1050, 720)
        layout = QVBoxLayout(self)
        title = QLabel(
            t("inspector.file", name=inspection.path.name, size=_human_size(inspection.package_size))
        )
        title.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
        layout.addWidget(title)

        splitter = QSplitter(Qt.Orientation.Vertical)
        upper = QSplitter(Qt.Orientation.Horizontal)
        self.summary = QTextBrowser()
        self.summary.setOpenExternalLinks(False)
        self.summary.setHtml(self._summary_html())
        upper.addWidget(self.summary)

        self.table = QTableWidget(len(inspection.entries), 5)
        self.table.setHorizontalHeaderLabels(
            [
                t("inspector.path"),
                t("inspector.category"),
                t("inspector.size"),
                t("inspector.compressed"),
                "SHA-256",
            ]
        )
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        for row, entry in enumerate(inspection.entries):
            values = (
                entry.name,
                entry.category,
                _human_size(entry.size),
                _human_size(entry.compressed_size),
                entry.sha256,
            )
            for column, value in enumerate(values):
                item = QTableWidgetItem(value)
                if column in (2, 3):
                    item.setTextAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
                self.table.setItem(row, column, item)
        header = self.table.horizontalHeader()
        header.setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        header.setSectionResizeMode(1, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(2, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(3, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(4, QHeaderView.ResizeMode.Stretch)
        upper.addWidget(self.table)
        upper.setStretchFactor(0, 2)
        upper.setStretchFactor(1, 5)
        splitter.addWidget(upper)

        preview_panel = QWidget()
        preview_layout = QVBoxLayout(preview_panel)
        preview_layout.setContentsMargins(0, 0, 0, 0)
        self.preview_label = QLabel(t("inspector.preview_hint"))
        self.preview = QPlainTextEdit()
        self.preview.setReadOnly(True)
        self.preview.setLineWrapMode(QPlainTextEdit.LineWrapMode.NoWrap)
        preview_layout.addWidget(self.preview_label)
        preview_layout.addWidget(self.preview)
        splitter.addWidget(preview_panel)
        splitter.setStretchFactor(0, 4)
        splitter.setStretchFactor(1, 3)
        layout.addWidget(splitter, 1)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)
        self.table.currentCellChanged.connect(self._show_preview)
        if inspection.entries:
            self.table.selectRow(0)
            self._show_preview(0, 0, -1, -1)

    def _summary_html(self) -> str:
        item = self.inspection
        manifest = item.manifest
        status = t("inspector.status_ok") if not item.errors else t(
            "inspector.status_errors", count=len(item.errors)
        )
        logical = item.logical_content_hash or "—"
        if item.content_hash_valid is True:
            hash_status = t("inspector.hash_match")
        elif item.content_hash_valid is False:
            hash_status = t("inspector.hash_mismatch")
        else:
            hash_status = t("inspector.hash_missing")
        chunks = [
            f"<h3>{html.escape(status)}</h3>",
            "<table>",
            f"<tr><td>{html.escape(t('inspector.mod'))}</td><td><b>{html.escape(str(manifest.get('name') or manifest.get('id') or '—'))}</b></td></tr>",
            f"<tr><td>ID</td><td>{html.escape(str(manifest.get('id') or '—'))}</td></tr>",
            f"<tr><td>{html.escape(t('inspector.version'))}</td><td>{html.escape(str(manifest.get('version') or '—'))}</td></tr>",
            f"<tr><td>{html.escape(t('inspector.author'))}</td><td>{html.escape(str(manifest.get('author') or '—'))}</td></tr>",
            f"<tr><td>{html.escape(t('inspector.entries'))}</td><td>{len(item.entries)}</td></tr>",
            f"<tr><td>{html.escape(t('inspector.stories'))}</td><td>{len(item.stories)}</td></tr>",
            f"<tr><td>{html.escape(t('inspector.lua'))}</td><td>{len(item.lua_files)}</td></tr>",
            f"<tr><td>{html.escape(t('inspector.assets'))}</td><td>{len(item.bundled_assets)}</td></tr>",
            f"<tr><td>{html.escape(t('inspector.user_contents'))}</td><td>{len(item.user_contents)}</td></tr>",
            f"<tr><td>{html.escape(t('inspector.content_hash'))}</td><td>{html.escape(hash_status)}<br><code>{logical}</code></td></tr>",
            f"<tr><td>{html.escape(t('inspector.package_hash'))}</td><td><code>{item.package_sha256}</code></td></tr>",
            "</table>",
        ]
        if item.errors:
            chunks.append(f"<h4>{html.escape(t('inspector.errors'))}</h4><ul>")
            chunks.extend(f"<li>{html.escape(message)}</li>" for message in item.errors)
            chunks.append("</ul>")
        if item.warnings:
            chunks.append(f"<h4>{html.escape(t('inspector.warnings'))}</h4><ul>")
            chunks.extend(f"<li>{html.escape(message)}</li>" for message in item.warnings)
            chunks.append("</ul>")
        chunks.append(
            f"<h4>{html.escape(t('inspector.references'))}</h4>"
            f"<p>{html.escape(t('inspector.reference_counts', referenced=len(item.referenced_assets), missing=len(item.missing_assets), unused=len(item.unreferenced_assets)))}</p>"
        )
        if item.missing_assets:
            chunks.append("<p><b>" + html.escape(t("inspector.missing")) + "</b><br>" + "<br>".join(html.escape(x) for x in item.missing_assets) + "</p>")
        if item.unreferenced_assets:
            chunks.append("<p><b>" + html.escape(t("inspector.unused")) + "</b><br>" + "<br>".join(html.escape(x) for x in item.unreferenced_assets) + "</p>")
        return "".join(chunks)

    def _show_preview(self, row: int, _column: int, _old_row: int, _old_column: int) -> None:
        if row < 0 or row >= len(self.inspection.entries):
            self.preview.clear()
            return
        entry = self.inspection.entries[row]
        self.preview_label.setText(entry.name)
        if entry.preview:
            suffix = "\n\n" + t("inspector.truncated") if entry.preview_truncated else ""
            self.preview.setPlainText(entry.preview + suffix)
        else:
            self.preview.setPlainText(t("inspector.binary", sha=entry.sha256))
