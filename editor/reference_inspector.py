# -*- coding: utf-8 -*-
"""轻量 Find References：复用 Global Search 索引，不修改 Story。"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QDialog,
    QDialogButtonBox,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QPushButton,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
)

from global_search import SearchHit, index_project
from i18n import t


@dataclass(frozen=True)
class ReferenceTarget:
    kind: str
    symbol: str
    story_id: str | None = None


def target_for_search_hit(hit: SearchHit) -> ReferenceTarget | None:
    if hit.category == "story":
        return ReferenceTarget("story", hit.story_id)
    if hit.category == "node":
        return ReferenceTarget("node", hit.value, hit.story_id)
    if hit.category == "goto":
        return ReferenceTarget("node", hit.value, hit.story_id)
    if hit.category in ("voice", "image", "content_ref") and hit.value.startswith("user:"):
        return ReferenceTarget("content", hit.value)
    if hit.category == "character":
        return ReferenceTarget("character", hit.value)
    if hit.category == "variable":
        return ReferenceTarget("variable", hit.value)
    if hit.category == "flag":
        return ReferenceTarget("flag", hit.value)
    return None


def find_symbol_references(
    stories: dict, target: ReferenceTarget, manifest: dict | None = None
) -> list[SearchHit]:
    hits = index_project(stories)
    symbol = target.symbol
    if target.kind == "content":
        return [
            hit for hit in hits
            if hit.value == symbol and hit.category in
            ("character", "voice", "image", "content_ref")
        ]
    if target.kind == "character":
        found = [hit for hit in hits if hit.category == "character" and hit.value == symbol]
        for index, trigger in enumerate(((manifest or {}).get("campaign") or {}).get("triggers") or []):
            affinity = trigger.get("when_affinity") if isinstance(trigger, dict) else None
            if isinstance(affinity, dict) and affinity.get("character") == symbol:
                found.append(SearchHit(
                    "character", str(trigger.get("script") or "?"), None,
                    "manifest.campaign.triggers[%d].when_affinity.character" % index,
                    symbol, symbol,
                ))
        return found
    if target.kind == "variable":
        return [hit for hit in hits if hit.category == "variable" and hit.value == symbol]
    if target.kind == "flag":
        found = [hit for hit in hits if hit.category == "flag" and hit.value == symbol]
        for index, trigger in enumerate(((manifest or {}).get("campaign") or {}).get("triggers") or []):
            if not isinstance(trigger, dict):
                continue
            for field in ("when_flag_set", "when_flag_clear"):
                if trigger.get(field) == symbol:
                    found.append(SearchHit(
                        "flag", str(trigger.get("script") or "?"), None,
                        "manifest.campaign.triggers[%d].%s" % (index, field),
                        symbol, symbol,
                    ))
        return found
    if target.kind == "node":
        return [
            hit for hit in hits
            if hit.category == "goto" and hit.value == symbol
            and (not target.story_id or hit.story_id == target.story_id)
        ]
    if target.kind == "story":
        found = [
            hit for hit in hits
            if hit.category == "goto" and hit.field.endswith("next_script")
            and hit.value == symbol
        ]
        entry = (manifest or {}).get("entry")
        if entry == symbol:
            found.insert(
                0,
                SearchHit("story", symbol, None, "manifest.entry", symbol, symbol),
            )
        for index, trigger in enumerate(((manifest or {}).get("campaign") or {}).get("triggers") or []):
            if isinstance(trigger, dict) and trigger.get("script") == symbol:
                found.append(
                    SearchHit(
                        "story", symbol, None,
                        "manifest.campaign.triggers[%d].script" % index,
                        symbol, symbol,
                    )
                )
        return found
    return []


class ReferenceDialog(QDialog):
    def __init__(
        self,
        stories: dict,
        target: ReferenceTarget,
        locate: Callable[[str, str | None], None],
        parent=None,
        manifest: dict | None = None,
    ):
        super().__init__(parent)
        self._locate = locate
        self._references = find_symbol_references(stories, target, manifest)
        self.setWindowTitle(t("references.title", symbol=target.symbol))
        self.resize(760, 430)
        layout = QVBoxLayout(self)
        summary = QLabel(
            t("references.summary", kind=t("references.kind." + target.kind),
              symbol=target.symbol, count=len(self._references))
        )
        summary.setWordWrap(True)
        layout.addWidget(summary)
        self.table = QTableWidget(0, 4)
        self.table.setHorizontalHeaderLabels(
            [t("search.col.story"), t("search.col.node"),
             t("search.col.field"), t("search.col.match")]
        )
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QTableWidget.SelectionMode.SingleSelection)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        header = self.table.horizontalHeader()
        for column in range(3):
            header.setSectionResizeMode(column, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(3, QHeaderView.ResizeMode.Stretch)
        layout.addWidget(self.table, 1)
        for hit in self._references:
            row = self.table.rowCount()
            self.table.insertRow(row)
            for column, value in enumerate(
                (hit.story_id, hit.node_id or "—", hit.field, hit.preview)
            ):
                self.table.setItem(row, column, QTableWidgetItem(value))
            self.table.item(row, 0).setData(
                Qt.ItemDataRole.UserRole, (hit.story_id, hit.node_id)
            )
        if self.table.rowCount():
            self.table.selectRow(0)
        else:
            self.table.setDisabled(True)

        buttons = QHBoxLayout()
        jump = QPushButton(t("search.jump"))
        jump.setEnabled(bool(self._references))
        jump.clicked.connect(self._jump_selected)
        buttons.addWidget(jump)
        buttons.addStretch(1)
        close = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        close.rejected.connect(self.reject)
        buttons.addWidget(close)
        layout.addLayout(buttons)
        self.table.cellDoubleClicked.connect(lambda *_: self._jump_selected())

    def _jump_selected(self) -> None:
        item = self.table.item(self.table.currentRow(), 0)
        if item is None:
            return
        target = item.data(Qt.ItemDataRole.UserRole)
        if not isinstance(target, tuple) or len(target) != 2:
            return
        self._locate(str(target[0]), str(target[1]) if target[1] else None)
        self.accept()
