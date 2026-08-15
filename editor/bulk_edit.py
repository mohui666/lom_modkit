# -*- coding: utf-8 -*-
"""保守的批量编辑：只开放所选节点 schema 中 kind 完全一致的字段。"""

from __future__ import annotations

from dataclasses import dataclass

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QCheckBox, QComboBox, QDialog, QDialogButtonBox, QDoubleSpinBox,
    QFormLayout, QLabel, QLineEdit, QListWidget, QListWidgetItem,
    QSpinBox, QVBoxLayout, QWidget,
)

import models
from i18n import t


# 结构字段、长文本与复合表不允许批改；其余标量只有 kind 相同才兼容。
_SAFE_FIELDS = {
    "character", "portrait", "voice", "position", "from", "to", "facing",
    "duration", "fadeDuration", "moveDuration", "seconds", "fade", "scale",
    "opacity", "x", "y", "angle", "active", "dimmed", "play", "remove",
    "waitDisplay", "display", "update", "count", "level", "value", "mode",
    "layer", "action", "kind", "op", "source",
}


@dataclass(frozen=True)
class BulkField:
    key: str
    label: str
    kind: str


def _schema_fields(node: dict) -> dict[str, BulkField]:
    schema = models.NODE_SCHEMAS.get(str(node.get("type") or ""), {})
    return {
        key: BulkField(key, label, kind)
        for key, label, kind, _optional in schema.get("fields", [])
        if key in _SAFE_FIELDS and not kind in ("options", "cases", "vars", "code", "multiline")
    }


def compatible_fields(nodes: list[dict]) -> list[BulkField]:
    if not nodes:
        return []
    common = _schema_fields(nodes[0])
    for node in nodes[1:]:
        other = _schema_fields(node)
        common = {
            key: field for key, field in common.items()
            if key in other and other[key].kind == field.kind
        }
    return sorted(common.values(), key=lambda field: field.label)


def _valid_value(kind: str, value) -> bool:
    if kind == "bool":
        return isinstance(value, bool)
    if kind == "int":
        return isinstance(value, int) and not isinstance(value, bool)
    if kind in ("float", "number", "percent_cg_scale", "percent_opacity", "percent_position"):
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if kind.startswith("enum:"):
        options = {raw for raw, _label in models.ENUM_SETS.get(kind[5:], [])}
        return isinstance(value, str) and value in options
    return isinstance(value, str)


def apply_bulk_edit(story: dict, indices: list[int], field: str, value) -> int:
    nodes = story.get("nodes") or []
    valid_indices = sorted({index for index in indices if 0 <= index < len(nodes)})
    selected = [nodes[index] for index in valid_indices]
    if len(selected) < 2:
        raise ValueError("批量编辑至少需要两个节点")
    compatible = {item.key: item for item in compatible_fields(selected)}
    if field not in compatible:
        raise ValueError("字段 %s 在所选节点之间不兼容" % field)
    if not _valid_value(compatible[field].kind, value):
        raise ValueError("字段 %s 的值类型不符合 %s" % (field, compatible[field].kind))
    for node in selected:
        node[field] = value
    return len(selected)


class BulkEditDialog(QDialog):
    def __init__(self, story: dict, editor_data: dict | None = None, parent=None):
        super().__init__(parent)
        self._story = story
        self._editor_data = editor_data or {}
        self._value_widget: QWidget | None = None
        self.setWindowTitle(t("bulk.title"))
        self.resize(620, 560)
        layout = QVBoxLayout(self)
        layout.addWidget(QLabel(t("bulk.intro")))
        self.nodes = QListWidget()
        self.nodes.setSelectionMode(QListWidget.SelectionMode.ExtendedSelection)
        for index, node in enumerate(story.get("nodes") or []):
            item = QListWidgetItem("%s · %s" % (node.get("id", "?"), models.node_summary(node, self._editor_data)))
            item.setData(Qt.ItemDataRole.UserRole, index)
            self.nodes.addItem(item)
        layout.addWidget(self.nodes, 1)
        self.form = QFormLayout()
        self.field_combo = QComboBox()
        self.form.addRow(t("bulk.field"), self.field_combo)
        self.value_host = QVBoxLayout()
        self.form.addRow(t("bulk.value"), self.value_host)
        layout.addLayout(self.form)
        self.message = QLabel(t("bulk.select_hint"))
        self.message.setWordWrap(True)
        layout.addWidget(self.message)
        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)
        self.nodes.itemSelectionChanged.connect(self._refresh_fields)
        self.field_combo.currentIndexChanged.connect(self._rebuild_value)
        self._ok = buttons.button(QDialogButtonBox.StandardButton.Ok)
        self._refresh_fields()

    def selected_indices(self) -> list[int]:
        return sorted(int(item.data(Qt.ItemDataRole.UserRole)) for item in self.nodes.selectedItems())

    def _selected_nodes(self) -> list[dict]:
        nodes = self._story.get("nodes") or []
        return [nodes[index] for index in self.selected_indices()]

    def _refresh_fields(self) -> None:
        fields = compatible_fields(self._selected_nodes())
        self.field_combo.blockSignals(True)
        self.field_combo.clear()
        for field in fields:
            self.field_combo.addItem("%s (%s)" % (field.label, field.key), field)
        self.field_combo.blockSignals(False)
        self.message.setText(
            t("bulk.ready", nodes=len(self.selected_indices()), fields=len(fields))
            if len(self.selected_indices()) >= 2 else t("bulk.select_hint")
        )
        self._ok.setEnabled(len(self.selected_indices()) >= 2 and bool(fields))
        self._rebuild_value()

    def _clear_value(self) -> None:
        while self.value_host.count():
            item = self.value_host.takeAt(0)
            widget = item.widget()
            if widget is not None:
                widget.deleteLater()
        self._value_widget = None

    def _rebuild_value(self, *_args) -> None:
        self._clear_value()
        field = self.field_combo.currentData()
        if not isinstance(field, BulkField):
            return
        kind = field.kind
        current = self._selected_nodes()[0].get(field.key) if self._selected_nodes() else None
        if kind == "bool":
            widget = QCheckBox(t("bulk.boolean_value"))
            widget.setChecked(bool(current))
        elif kind == "int":
            widget = QSpinBox(); widget.setRange(-1000000, 1000000); widget.setValue(int(current or 0))
        elif kind in ("float", "number"):
            widget = QDoubleSpinBox(); widget.setRange(-1000000, 1000000); widget.setDecimals(3); widget.setValue(float(current or 0))
        elif kind in ("percent_cg_scale", "percent_opacity", "percent_position"):
            widget = QSpinBox()
            low, high = ((10, 300) if kind == "percent_cg_scale" else
                         (0, 100) if kind == "percent_opacity" else (-100, 100))
            widget.setRange(low, high); widget.setSuffix(" %"); widget.setValue(int(current or 0))
        elif kind.startswith("enum:"):
            widget = QComboBox()
            for raw, label in models.ENUM_SETS.get(kind[5:], []):
                widget.addItem(label, raw)
            index = widget.findData(current)
            widget.setCurrentIndex(index if index >= 0 else 0)
        else:
            widget = QLineEdit(str(current or ""))
        self._value_widget = widget
        self.value_host.addWidget(widget)

    def selected_field(self) -> str:
        field = self.field_combo.currentData()
        return field.key if isinstance(field, BulkField) else ""

    def selected_value(self):
        widget = self._value_widget
        if isinstance(widget, QCheckBox): return widget.isChecked()
        if isinstance(widget, QSpinBox): return widget.value()
        if isinstance(widget, QDoubleSpinBox): return widget.value()
        if isinstance(widget, QComboBox): return widget.currentData()
        if isinstance(widget, QLineEdit): return widget.text()
        return None
