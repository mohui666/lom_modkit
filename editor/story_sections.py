# -*- coding: utf-8 -*-
"""Editor-only story sections/groups. They never change nodes or CFG flow."""

from __future__ import annotations

import copy
import re
from dataclasses import dataclass

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QComboBox, QDialog, QDialogButtonBox, QFormLayout, QHBoxLayout, QLabel,
    QLineEdit, QListWidget, QListWidgetItem, QMessageBox, QPushButton, QSpinBox,
    QVBoxLayout,
)

from i18n import t


_META_KEY = "_editor"
_ID_RE = re.compile(r"^[a-zA-Z0-9_-]+$")


@dataclass(frozen=True)
class StructureRow:
    kind: str  # section | group | node
    item_id: str
    title: str
    depth: int
    collapsed: bool
    start: int
    end: int
    node_index: int = -1


def get_sections(story: dict) -> list[dict]:
    meta = story.get(_META_KEY)
    if not isinstance(meta, dict) or not isinstance(meta.get("sections"), list):
        return []
    return meta["sections"]


def set_sections(story: dict, sections: list[dict]) -> None:
    meta = story.setdefault(_META_KEY, {})
    if not isinstance(meta, dict):
        meta = {}
        story[_META_KEY] = meta
    if sections:
        meta["sections"] = copy.deepcopy(sections)
    else:
        meta.pop("sections", None)
        if not meta:
            story.pop(_META_KEY, None)


def _node_positions(story: dict) -> dict[str, int]:
    return {
        str(node.get("id")): index
        for index, node in enumerate(story.get("nodes") or [])
        if isinstance(node, dict) and node.get("id")
    }


def _bounds(entry: dict, positions: dict[str, int]) -> tuple[int, int] | None:
    start = positions.get(str(entry.get("start") or ""))
    end = positions.get(str(entry.get("end") or ""))
    if start is None or end is None:
        return None
    return (min(start, end), max(start, end))


def _next_id(story: dict, kind: str) -> str:
    used = {
        str(item.get("id"))
        for section in get_sections(story)
        for item in [section] + list(section.get("groups") or [])
        if isinstance(item, dict)
    }
    index = 1
    while "%s%d" % (kind, index) in used:
        index += 1
    return "%s%d" % (kind, index)


def add_structure(
    story: dict,
    title: str,
    start_index: int,
    end_index: int,
    kind: str = "section",
    parent_id: str | None = None,
) -> dict:
    nodes = story.get("nodes") or []
    if kind not in ("section", "group"):
        raise ValueError("结构类型必须是 section 或 group")
    title = str(title or "").strip()
    if not title or len(title) > 100 or any(ord(ch) < 32 for ch in title):
        raise ValueError("分组名称不能为空、最长 100 字符且不能包含控制字符")
    lo, hi = sorted((int(start_index), int(end_index)))
    if not (0 <= lo <= hi < len(nodes)):
        raise ValueError("分组节点范围越界")
    positions = _node_positions(story)
    entry = {
        "id": _next_id(story, kind),
        "title": title,
        "start": nodes[lo]["id"],
        "end": nodes[hi]["id"],
        "collapsed": False,
    }
    sections = get_sections(story)
    if kind == "section":
        for other in sections:
            bounds = _bounds(other, positions)
            if bounds and not (hi < bounds[0] or lo > bounds[1]):
                raise ValueError("Section 不能与已有 Section 重叠")
        if not sections:
            set_sections(story, [])
            sections = story.setdefault(_META_KEY, {}).setdefault("sections", [])
        sections.append(entry)
        return entry

    parent = next((item for item in sections if item.get("id") == parent_id), None)
    if parent is None:
        raise ValueError("Group 必须选择一个父 Section")
    parent_bounds = _bounds(parent, positions)
    if parent_bounds is None or lo < parent_bounds[0] or hi > parent_bounds[1]:
        raise ValueError("Group 范围必须完整位于父 Section 内")
    groups = parent.setdefault("groups", [])
    for other in groups:
        bounds = _bounds(other, positions)
        if bounds and not (hi < bounds[0] or lo > bounds[1]):
            raise ValueError("同一 Section 内的 Group 不能重叠")
    groups.append(entry)
    return entry


def remove_structure(story: dict, item_id: str) -> bool:
    sections = get_sections(story)
    for index, section in enumerate(sections):
        if section.get("id") == item_id:
            del sections[index]
            set_sections(story, sections)
            return True
        groups = section.get("groups") or []
        for group_index, group in enumerate(groups):
            if group.get("id") == item_id:
                del groups[group_index]
                if not groups:
                    section.pop("groups", None)
                set_sections(story, sections)
                return True
    return False


def set_collapsed(story: dict, item_id: str, collapsed: bool) -> bool:
    for section in get_sections(story):
        for item in [section] + list(section.get("groups") or []):
            if item.get("id") == item_id:
                changed = bool(item.get("collapsed")) != bool(collapsed)
                item["collapsed"] = bool(collapsed)
                return changed
    return False


def set_all_collapsed(story: dict, collapsed: bool) -> int:
    changed = 0
    for section in get_sections(story):
        for item in [section] + list(section.get("groups") or []):
            if bool(item.get("collapsed")) != bool(collapsed):
                item["collapsed"] = bool(collapsed)
                changed += 1
    return changed


def expand_for_node(story: dict, node_index: int) -> bool:
    positions = _node_positions(story)
    changed = False
    for section in get_sections(story):
        bounds = _bounds(section, positions)
        if bounds and bounds[0] <= node_index <= bounds[1]:
            if section.get("collapsed"):
                section["collapsed"] = False
                changed = True
            for group in section.get("groups") or []:
                group_bounds = _bounds(group, positions)
                if group_bounds and group_bounds[0] <= node_index <= group_bounds[1] and group.get("collapsed"):
                    group["collapsed"] = False
                    changed = True
    return changed


def structure_rows(story: dict) -> list[StructureRow]:
    """Return visible tree rows in the exact underlying ``nodes[]`` order."""
    nodes = story.get("nodes") or []
    positions = _node_positions(story)
    sections: list[tuple[int, int, dict]] = []
    for section in get_sections(story):
        bounds = _bounds(section, positions)
        if bounds:
            sections.append((bounds[0], bounds[1], section))
    sections.sort(key=lambda item: (item[0], item[1], str(item[2].get("id"))))
    rows: list[StructureRow] = []
    for index, node in enumerate(nodes):
        section_match = next((item for item in sections if item[0] <= index <= item[1]), None)
        if section_match is None:
            rows.append(StructureRow("node", str(node.get("id") or ""), "", 0, False, index, index, index))
            continue
        section_start, section_end, section = section_match
        if index == section_start:
            rows.append(StructureRow(
                "section", str(section.get("id") or ""), str(section.get("title") or ""),
                0, bool(section.get("collapsed")), section_start, section_end,
            ))
        if section.get("collapsed"):
            continue
        groups: list[tuple[int, int, dict]] = []
        for group in section.get("groups") or []:
            bounds = _bounds(group, positions)
            if bounds:
                groups.append((bounds[0], bounds[1], group))
        group_match = next((item for item in groups if item[0] <= index <= item[1]), None)
        if group_match is not None:
            group_start, group_end, group = group_match
            if index == group_start:
                rows.append(StructureRow(
                    "group", str(group.get("id") or ""), str(group.get("title") or ""),
                    1, bool(group.get("collapsed")), group_start, group_end,
                ))
            if group.get("collapsed"):
                continue
            depth = 2
        else:
            depth = 1
        rows.append(StructureRow("node", str(node.get("id") or ""), "", depth, False, index, index, index))
    return rows


def retarget_structure_ids(story: dict, mapping: dict[str, str]) -> int:
    changed = 0
    for section in get_sections(story):
        for item in [section] + list(section.get("groups") or []):
            for key in ("start", "end"):
                if item.get(key) in mapping:
                    item[key] = mapping[item[key]]
                    changed += 1
    return changed


def repair_after_delete(story: dict, removed_id: str, old_nodes: list[dict]) -> None:
    old_ids = [str(node.get("id") or "") for node in old_nodes]
    if removed_id not in old_ids:
        return
    removed_at = old_ids.index(removed_id)
    for section in list(get_sections(story)):
        for item in [section] + list(section.get("groups") or []):
            start_at = old_ids.index(item["start"]) if item.get("start") in old_ids else -1
            end_at = old_ids.index(item["end"]) if item.get("end") in old_ids else -1
            lo, hi = sorted((start_at, end_at))
            if item.get("start") == removed_id:
                replacement = lo + 1 if lo < hi else -1
                if replacement >= 0:
                    item["start"] = old_ids[replacement]
            if item.get("end") == removed_id:
                replacement = hi - 1 if lo < hi else -1
                if replacement >= 0:
                    item["end"] = old_ids[replacement]
            if lo == hi == removed_at:
                remove_structure(story, str(item.get("id") or ""))


class StorySectionsDialog(QDialog):
    def __init__(self, story: dict, parent=None):
        super().__init__(parent)
        self.story = copy.deepcopy(story)
        self.setWindowTitle(t("sections.title"))
        self.resize(680, 570)
        root = QVBoxLayout(self)
        root.addWidget(QLabel(t("sections.intro")))
        form = QFormLayout()
        self.title_edit = QLineEdit()
        self.kind_combo = QComboBox()
        self.kind_combo.addItem("Section", "section")
        self.kind_combo.addItem("Group", "group")
        self.parent_combo = QComboBox()
        upper = max(1, len(self.story.get("nodes") or []))
        self.start_spin = QSpinBox(); self.start_spin.setRange(1, upper)
        self.end_spin = QSpinBox(); self.end_spin.setRange(1, upper)
        form.addRow(t("sections.name"), self.title_edit)
        form.addRow(t("sections.kind"), self.kind_combo)
        form.addRow(t("sections.parent"), self.parent_combo)
        form.addRow(t("sections.start"), self.start_spin)
        form.addRow(t("sections.end"), self.end_spin)
        root.addLayout(form)
        add_button = QPushButton(t("sections.add")); add_button.clicked.connect(self._add)
        add_button.setEnabled(bool(self.story.get("nodes")))
        root.addWidget(add_button)
        self.items = QListWidget(); root.addWidget(self.items, 1)
        actions = QHBoxLayout()
        for label, callback in (
            (t("sections.toggle"), self._toggle),
            (t("sections.expand_all"), lambda: self._set_all(False)),
            (t("sections.collapse_all"), lambda: self._set_all(True)),
            (t("sections.delete"), self._delete),
        ):
            button = QPushButton(label); button.clicked.connect(callback); actions.addWidget(button)
        root.addLayout(actions)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept); buttons.rejected.connect(self.reject); root.addWidget(buttons)
        self.kind_combo.currentIndexChanged.connect(self._update_parent_state)
        self._reload()

    def _reload(self) -> None:
        self.parent_combo.clear()
        for section in get_sections(self.story):
            self.parent_combo.addItem(str(section.get("title") or section.get("id")), section.get("id"))
        self.items.clear()
        for section in get_sections(self.story):
            self._add_item(section, 0, "Section")
            for group in section.get("groups") or []:
                self._add_item(group, 1, "Group")
        self._update_parent_state()

    def _add_item(self, entry: dict, depth: int, kind: str) -> None:
        marker = "▸" if entry.get("collapsed") else "▾"
        item = QListWidgetItem("  " * depth + "%s %s · %s [%s…%s]" % (
            marker, kind, entry.get("title", ""), entry.get("start", "?"), entry.get("end", "?")))
        item.setData(Qt.ItemDataRole.UserRole, entry.get("id")); self.items.addItem(item)

    def _update_parent_state(self, *_args) -> None:
        is_group = self.kind_combo.currentData() == "group"
        self.parent_combo.setEnabled(is_group)

    def _add(self) -> None:
        try:
            add_structure(
                self.story, self.title_edit.text(), self.start_spin.value() - 1,
                self.end_spin.value() - 1, str(self.kind_combo.currentData()),
                self.parent_combo.currentData(),
            )
        except ValueError as exc:
            QMessageBox.warning(self, t("sections.title"), str(exc)); return
        self.title_edit.clear(); self._reload()

    def _toggle(self) -> None:
        item = self.items.currentItem()
        if item is None: return
        item_id = str(item.data(Qt.ItemDataRole.UserRole) or "")
        current = next((row.collapsed for row in structure_rows(self.story) if row.item_id == item_id), False)
        set_collapsed(self.story, item_id, not current); self._reload()

    def _set_all(self, collapsed: bool) -> None:
        set_all_collapsed(self.story, collapsed); self._reload()

    def _delete(self) -> None:
        item = self.items.currentItem()
        if item is None: return
        remove_structure(self.story, str(item.data(Qt.ItemDataRole.UserRole) or "")); self._reload()
