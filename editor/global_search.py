# -*- coding: utf-8 -*-
"""项目级 Story 搜索：纯索引 + 可定位结果对话框。"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Iterable

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QHBoxLayout,
    QHeaderView,
    QLineEdit,
    QPushButton,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
)

from i18n import t


@dataclass(frozen=True)
class SearchHit:
    category: str
    story_id: str
    node_id: str | None
    field: str
    value: str
    preview: str

    @property
    def haystack(self) -> str:
        return " ".join(
            (self.category, self.story_id, self.node_id or "", self.field,
             self.value, self.preview)
        ).casefold()


CATEGORIES = (
    "story", "node", "text", "character", "portrait", "voice", "image",
    "variable", "flag", "goto", "content_ref",
)


def _leaves(value, path: str = "") -> Iterable[tuple[str, str]]:
    if isinstance(value, str):
        yield path, value
    elif isinstance(value, dict):
        for key, child in value.items():
            child_path = "%s.%s" % (path, key) if path else str(key)
            yield from _leaves(child, child_path)
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from _leaves(child, "%s[%d]" % (path, index))


def _category(node: dict, field: str, value: str) -> str | None:
    leaf = field.rsplit(".", 1)[-1].split("[", 1)[0]
    node_type = str(node.get("type") or "")
    if value.startswith("user:"):
        if leaf == "character":
            return "character"
        if leaf == "voice" or node_type in ("music", "sound"):
            return "voice"
        if leaf == "image":
            return "image"
        return "content_ref"
    if leaf == "character":
        return "character"
    if leaf == "portrait":
        return "portrait"
    if leaf == "voice":
        return "voice"
    if node_type in ("music", "sound") and leaf == "name":
        return "voice"
    if leaf == "image":
        return "image"
    if leaf == "goto" or leaf.startswith("goto_") or leaf in ("next_script", "next"):
        return "goto"
    if leaf == "flag" or node_type in ("flag", "game_flag"):
        return "flag"
    if leaf in ("var", "variable", "vars"):
        return "variable"
    if node_type == "block" and field.startswith("vars[") and leaf == "name":
        return "variable"
    if leaf == "key" and node_type == "branch":
        return "flag" if node.get("source") in ("mod", "flag_value") else "variable"
    if leaf in ("text", "title", "desc", "name", "band_texts") or ".text" in field:
        return "text"
    return None


def index_project(stories: dict) -> list[SearchHit]:
    hits: list[SearchHit] = []
    for story_id in sorted(stories or {}):
        story = stories[story_id]
        if not isinstance(story, dict):
            continue
        title = str(story.get("title") or "")
        hits.append(SearchHit("story", story_id, None, "story", story_id, title))
        if title:
            hits.append(SearchHit("story", story_id, None, "title", title, title))
        for node in story.get("nodes") or []:
            if not isinstance(node, dict):
                continue
            node_id = str(node.get("id") or "")
            node_type = str(node.get("type") or "")
            hits.append(SearchHit("node", story_id, node_id, "id", node_id, node_type))
            for field, value in _leaves(node):
                if field in ("id", "type") or not value:
                    continue
                category = _category(node, field, value)
                if category is None:
                    continue
                preview = value if len(value) <= 120 else value[:117] + "…"
                hits.append(
                    SearchHit(category, story_id, node_id, field, value, preview)
                )
    return hits


def search_hits(
    hits: Iterable[SearchHit], query: str, category: str = ""
) -> list[SearchHit]:
    terms = [part.casefold() for part in (query or "").split() if part]
    return [
        hit for hit in hits
        if (not category or hit.category == category)
        and all(term in hit.haystack for term in terms)
    ]


class GlobalSearchDialog(QDialog):
    def __init__(
        self,
        stories: dict,
        locate: Callable[[str, str | None], None],
        parent=None,
    ):
        super().__init__(parent)
        self._hits = index_project(stories)
        self._locate = locate
        self.setWindowTitle(t("search.title"))
        self.resize(900, 560)
        layout = QVBoxLayout(self)
        filters = QHBoxLayout()
        self.query_edit = QLineEdit()
        self.query_edit.setPlaceholderText(t("search.placeholder"))
        self.category_filter = QComboBox()
        self.category_filter.addItem(t("search.all"), "")
        for category in CATEGORIES:
            self.category_filter.addItem(t("search.category." + category), category)
        filters.addWidget(self.query_edit, 1)
        filters.addWidget(self.category_filter)
        layout.addLayout(filters)

        self.table = QTableWidget(0, 5)
        self.table.setHorizontalHeaderLabels(
            [t("search.col.type"), t("search.col.story"), t("search.col.node"),
             t("search.col.field"), t("search.col.match")]
        )
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QTableWidget.SelectionMode.SingleSelection)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        header = self.table.horizontalHeader()
        for column in range(4):
            header.setSectionResizeMode(column, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(4, QHeaderView.ResizeMode.Stretch)
        layout.addWidget(self.table, 1)

        row = QHBoxLayout()
        jump = QPushButton(t("search.jump"))
        jump.clicked.connect(self._jump_selected)
        row.addWidget(jump)
        row.addStretch(1)
        close = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        close.rejected.connect(self.reject)
        row.addWidget(close)
        layout.addLayout(row)

        self.query_edit.textChanged.connect(self._refresh)
        self.category_filter.currentIndexChanged.connect(self._refresh)
        self.table.cellDoubleClicked.connect(lambda *_: self._jump_selected())
        self._refresh()
        self.query_edit.setFocus()

    def _refresh(self, *_args) -> None:
        results = search_hits(
            self._hits, self.query_edit.text(),
            str(self.category_filter.currentData() or ""),
        )
        self.table.setRowCount(0)
        for hit in results:
            row = self.table.rowCount()
            self.table.insertRow(row)
            values = (
                t("search.category." + hit.category), hit.story_id,
                hit.node_id or "—", hit.field, hit.preview,
            )
            for column, value in enumerate(values):
                self.table.setItem(row, column, QTableWidgetItem(value))
            self.table.item(row, 0).setData(
                Qt.ItemDataRole.UserRole, (hit.story_id, hit.node_id)
            )
        if self.table.rowCount():
            self.table.selectRow(0)

    def _jump_selected(self) -> None:
        item = self.table.item(self.table.currentRow(), 0)
        if item is None:
            return
        target = item.data(Qt.ItemDataRole.UserRole)
        if not isinstance(target, tuple) or len(target) != 2:
            return
        self._locate(str(target[0]), str(target[1]) if target[1] else None)
        self.accept()
