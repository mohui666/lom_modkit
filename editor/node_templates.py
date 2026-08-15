# -*- coding: utf-8 -*-
"""Reusable node templates with collision-free IDs and local-only storage."""

from __future__ import annotations

import copy
import json
import os
import re
from pathlib import Path, PurePosixPath

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QDialog, QDialogButtonBox, QFormLayout, QHBoxLayout, QLabel,
    QLineEdit, QListWidget, QListWidgetItem, QMessageBox, QPushButton, QSpinBox,
    QVBoxLayout,
)

import models
from i18n import t


TEMPLATE_SCHEMA = 1
_WINDOWS_ABSOLUTE = re.compile(r"^[a-zA-Z]:[\\/]")
_ASSET_KEYS = {
    "asset", "audio", "background", "file", "image", "music", "path",
    "portrait", "sound", "voice",
}


def template_store_path() -> Path:
    appdata = os.environ.get("APPDATA")
    base = Path(appdata) if appdata else Path.home() / "AppData" / "Roaming"
    return base / "lom_modkit" / "node_templates.json"


def _is_absolute_asset(value: str) -> bool:
    value = value.strip()
    return bool(
        _WINDOWS_ABSOLUTE.match(value)
        or value.startswith(("\\\\", "file://"))
        or PurePosixPath(value).is_absolute()
    )


def _check_no_absolute_assets(value, key: str = "", asset_context: bool = False) -> None:
    asset_context = asset_context or key in _ASSET_KEYS or key.endswith(("_path", "_file"))
    if isinstance(value, dict):
        for child_key, child in value.items():
            _check_no_absolute_assets(child, str(child_key).lower(), asset_context)
    elif isinstance(value, list):
        for child in value:
            _check_no_absolute_assets(child, key, asset_context)
    elif isinstance(value, str) and asset_context and _is_absolute_asset(value):
        raise ValueError("节点模板不能保存本机绝对资源路径: %s" % value)


def create_template(name: str, nodes: list[dict]) -> dict:
    name = str(name or "").strip()
    if not name:
        raise ValueError("模板名称不能为空")
    if len(name) > 80 or any(ord(ch) < 32 for ch in name):
        raise ValueError("模板名称最长 80 个字符且不能包含控制字符")
    if not nodes:
        raise ValueError("模板至少需要一个节点")
    copied = copy.deepcopy(nodes)
    ids: set[str] = set()
    for node in copied:
        if not isinstance(node, dict):
            raise ValueError("模板节点结构非法")
        node_id = str(node.get("id") or "")
        if not models.ID_PATTERN.fullmatch(node_id) or node_id in ids:
            raise ValueError("模板节点编号缺失、非法或重复: %s" % node_id)
        ids.add(node_id)
    _check_no_absolute_assets(copied)
    return {"schema": TEMPLATE_SCHEMA, "name": name, "nodes": copied}


def validate_template(template: dict) -> dict:
    if not isinstance(template, dict) or template.get("schema") != TEMPLATE_SCHEMA:
        raise ValueError("不支持的节点模板格式")
    return create_template(template.get("name", ""), template.get("nodes") or [])


def load_templates(path: Path | None = None) -> list[dict]:
    target = Path(path or template_store_path())
    if not target.is_file():
        return []
    try:
        raw = json.loads(target.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise ValueError("无法读取节点模板库: %s" % exc) from exc
    if not isinstance(raw, dict) or raw.get("schema") != TEMPLATE_SCHEMA:
        raise ValueError("不支持的节点模板库格式")
    templates = [validate_template(item) for item in raw.get("templates") or []]
    names = [item["name"].casefold() for item in templates]
    if len(names) != len(set(names)):
        raise ValueError("节点模板库包含重名模板")
    return templates


def save_templates(templates: list[dict], path: Path | None = None) -> None:
    target = Path(path or template_store_path())
    checked = [validate_template(item) for item in templates]
    names = [item["name"].casefold() for item in checked]
    if len(names) != len(set(names)):
        raise ValueError("模板名称已存在，未覆盖原模板")
    target.parent.mkdir(parents=True, exist_ok=True)
    pending = target.with_suffix(target.suffix + ".tmp")
    pending.write_text(
        json.dumps({"schema": TEMPLATE_SCHEMA, "templates": checked}, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    pending.replace(target)


def add_template(template: dict, path: Path | None = None) -> list[dict]:
    checked = validate_template(template)
    templates = load_templates(path)
    if any(item["name"].casefold() == checked["name"].casefold() for item in templates):
        raise ValueError("模板名称已存在，未覆盖原模板")
    templates.append(checked)
    save_templates(templates, path)
    return templates


def instantiate_template(story: dict, template: dict, insert_at: int) -> tuple[int, int, dict[str, str]]:
    """Insert a template and return ``(first_index, count, old_to_new_id)``."""
    checked = validate_template(template)
    existing = story.setdefault("nodes", [])
    if not isinstance(existing, list):
        raise ValueError("剧情缺少 nodes 数组")
    at = max(0, min(int(insert_at), len(existing)))
    working = {"nodes": copy.deepcopy(existing)}
    clones = copy.deepcopy(checked["nodes"])
    mapping: dict[str, str] = {}
    for node in clones:
        old_id = node["id"]
        new_id = models.make_node_id(working, str(node.get("type") or "n"))
        mapping[old_id] = new_id
        # Reserve the generated ID without mutating the clone yet. A generated
        # ID may equal another source ID (say1→say2 while say2 also exists), so
        # pre-mutating would make a later generic retarget apply twice.
        working["nodes"].append({"id": new_id})
    # Only references whose targets are part of the saved block are remapped;
    # cross-block targets stay external by design.
    models.retarget_node_ids({"nodes": clones}, mapping)
    existing[at:at] = clones
    return at, len(clones), mapping


class NodeTemplateDialog(QDialog):
    """Manage templates and choose one to insert after the current node."""

    def __init__(self, story: dict, current_index: int, editor_data: dict | None = None, parent=None):
        super().__init__(parent)
        self._story = story
        self._editor_data = editor_data or {}
        self._path = template_store_path()
        self._chosen: dict | None = None
        self.setWindowTitle(t("template.title"))
        self.resize(700, 560)
        root = QVBoxLayout(self)
        root.addWidget(QLabel(t("template.intro")))

        nodes = story.get("nodes") or []
        range_form = QFormLayout()
        self.name_edit = QLineEdit()
        self.start_spin = QSpinBox()
        self.end_spin = QSpinBox()
        upper = max(1, len(nodes))
        for spin in (self.start_spin, self.end_spin):
            spin.setRange(1, upper)
        selected = min(max(current_index, 0), max(0, len(nodes) - 1)) + 1
        self.start_spin.setValue(selected)
        self.end_spin.setValue(selected)
        range_form.addRow(t("template.name"), self.name_edit)
        range_form.addRow(t("template.start"), self.start_spin)
        range_form.addRow(t("template.end"), self.end_spin)
        root.addLayout(range_form)
        save_button = QPushButton(t("template.save_range"))
        save_button.setEnabled(bool(nodes))
        save_button.clicked.connect(self._save_range)
        root.addWidget(save_button)

        self.items = QListWidget()
        root.addWidget(self.items, 1)
        actions = QHBoxLayout()
        delete_button = QPushButton(t("template.delete"))
        delete_button.clicked.connect(self._delete)
        actions.addWidget(delete_button)
        actions.addStretch(1)
        root.addLayout(actions)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Cancel)
        self.insert_button = buttons.addButton(t("template.insert"), QDialogButtonBox.ButtonRole.AcceptRole)
        self.insert_button.clicked.connect(self._choose)
        buttons.rejected.connect(self.reject)
        root.addWidget(buttons)
        self.items.itemSelectionChanged.connect(self._update_buttons)
        self._reload()

    def chosen_template(self) -> dict | None:
        return copy.deepcopy(self._chosen)

    def _reload(self) -> None:
        self.items.clear()
        try:
            templates = load_templates(self._path)
        except ValueError as exc:
            QMessageBox.warning(self, t("template.title"), str(exc))
            templates = []
        for template in templates:
            label = t("template.item", name=template["name"], count=len(template["nodes"]))
            item = QListWidgetItem(label)
            item.setData(Qt.ItemDataRole.UserRole, template)
            self.items.addItem(item)
        self._update_buttons()

    def _save_range(self) -> None:
        start, end = sorted((self.start_spin.value() - 1, self.end_spin.value() - 1))
        try:
            template = create_template(self.name_edit.text(), (self._story.get("nodes") or [])[start:end + 1])
            add_template(template, self._path)
        except (OSError, ValueError) as exc:
            QMessageBox.warning(self, t("template.title"), str(exc))
            return
        self.name_edit.clear()
        self._reload()

    def _delete(self) -> None:
        item = self.items.currentItem()
        if item is None:
            return
        selected = item.data(Qt.ItemDataRole.UserRole)
        try:
            templates = [entry for entry in load_templates(self._path) if entry["name"] != selected["name"]]
            save_templates(templates, self._path)
        except (OSError, ValueError) as exc:
            QMessageBox.warning(self, t("template.title"), str(exc))
            return
        self._reload()

    def _update_buttons(self) -> None:
        self.insert_button.setEnabled(self.items.currentItem() is not None)

    def _choose(self) -> None:
        item = self.items.currentItem()
        if item is None:
            return
        self._chosen = item.data(Qt.ItemDataRole.UserRole)
        self.accept()
