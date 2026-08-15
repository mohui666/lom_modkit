# -*- coding: utf-8 -*-
"""Safe cross-story node range transfer with explicit unresolved warnings."""

from __future__ import annotations

import copy
from dataclasses import dataclass

from PySide6.QtWidgets import (
    QComboBox, QDialog, QDialogButtonBox, QFormLayout, QLabel, QSpinBox,
    QVBoxLayout,
)

import models
from i18n import t


_GOTO_KEYS = ("goto", "goto_大成功", "goto_成功", "goto_失败")
_TERMINAL_TYPES = {"end", "choice", "branch", "dice", "goto_scene", "raw", "death"}


@dataclass(frozen=True)
class TransferResult:
    source_story: str
    target_story: str
    first_index: int
    count: int
    id_mapping: dict[str, str]
    warnings: tuple[str, ...]


def _local_targets(node: dict):
    target = node.get("goto")
    if isinstance(target, str) and target:
        yield "goto", target
    for option_index, option in enumerate(node.get("options") or []):
        if not isinstance(option, dict):
            continue
        for key in _GOTO_KEYS:
            target = option.get(key)
            if isinstance(target, str) and target:
                yield "options[%d].%s" % (option_index, key), target
    for case_index, case in enumerate(node.get("cases") or []):
        if isinstance(case, dict) and isinstance(case.get("goto"), str) and case["goto"]:
            yield "cases[%d].goto" % case_index, case["goto"]


def _transfer_warnings(
    stories: dict[str, dict], source: dict, selected: list[dict], start: int, end: int
) -> list[str]:
    selected_ids = {str(node.get("id") or "") for node in selected}
    warnings: list[str] = []
    for node in selected:
        node_id = str(node.get("id") or "?")
        for field, target in _local_targets(node):
            if target not in selected_ids:
                warnings.append(
                    "%s.%s 指向复制范围外的本章节点 %s；已原样保留，需在目标章节手工确认"
                    % (node_id, field, target)
                )
        next_script = node.get("next_script")
        if isinstance(next_script, str) and next_script and next_script not in stories:
            warnings.append(
                "%s.next_script 指向不存在的章节 %s；已原样保留" % (node_id, next_script)
            )
    last = selected[-1]
    if (
        end + 1 < len(source.get("nodes") or [])
        and last.get("type") not in _TERMINAL_TYPES
        and not last.get("goto")
    ):
        warnings.append(
            "%s 原本会顺序进入复制范围外的下一节点；粘贴后将顺序进入目标插入点后的节点"
            % str(last.get("id") or "?")
        )
    # Stable order and no duplicated noise when several schema paths coincide.
    return list(dict.fromkeys(warnings))


def copy_nodes_between_stories(
    stories: dict[str, dict],
    source_story: str,
    start_index: int,
    end_index: int,
    target_story: str,
    insert_at: int,
) -> TransferResult:
    if source_story == target_story:
        raise ValueError("跨章节复制必须选择不同的来源与目标章节")
    if source_story not in stories or target_story not in stories:
        raise ValueError("来源或目标章节不存在")
    source = stories[source_story]
    target = stories[target_story]
    source_nodes = source.get("nodes") or []
    target_nodes = target.get("nodes") or []
    lo, hi = sorted((int(start_index), int(end_index)))
    if not (0 <= lo <= hi < len(source_nodes)):
        raise ValueError("复制节点范围越界")
    at = max(0, min(int(insert_at), len(target_nodes)))
    selected = copy.deepcopy(source_nodes[lo:hi + 1])
    for node in selected:
        node_id = str(node.get("id") or "")
        if not models.ID_PATTERN.fullmatch(node_id):
            raise ValueError("来源节点编号非法: %s" % node_id)
    ids = [str(node["id"]) for node in selected]
    if len(ids) != len(set(ids)):
        raise ValueError("复制范围包含重复节点编号")
    warnings = _transfer_warnings(stories, source, selected, lo, hi)

    # Allocate every copied node a fresh target-local ID. This resolves both
    # present and future collisions and makes internal remapping unambiguous.
    working = {"nodes": copy.deepcopy(target_nodes)}
    mapping: dict[str, str] = {}
    for node in selected:
        new_id = models.make_node_id(working, str(node.get("type") or "n"))
        mapping[str(node["id"])] = new_id
        working["nodes"].append({"id": new_id})
    models.retarget_node_ids({"nodes": selected}, mapping)
    target_nodes[at:at] = selected
    target["nodes"] = target_nodes
    return TransferResult(
        source_story, target_story, at, len(selected), mapping, tuple(warnings)
    )


class CrossStoryTransferDialog(QDialog):
    def __init__(self, stories: dict[str, dict], current_story: str, parent=None):
        super().__init__(parent)
        self._stories = stories
        self.setWindowTitle(t("transfer.title"))
        self.resize(560, 390)
        root = QVBoxLayout(self)
        root.addWidget(QLabel(t("transfer.intro")))
        form = QFormLayout()
        self.source_combo = QComboBox()
        self.target_combo = QComboBox()
        for story_id in sorted(stories):
            title = str(stories[story_id].get("title") or story_id)
            label = "%s — %s" % (story_id, title) if title != story_id else story_id
            self.source_combo.addItem(label, story_id)
            self.target_combo.addItem(label, story_id)
        self.source_combo.setCurrentIndex(max(0, self.source_combo.findData(current_story)))
        target_index = next((i for i in range(self.target_combo.count()) if self.target_combo.itemData(i) != current_story), 0)
        self.target_combo.setCurrentIndex(target_index)
        self.start_spin = QSpinBox(); self.end_spin = QSpinBox(); self.insert_spin = QSpinBox()
        form.addRow(t("transfer.source"), self.source_combo)
        form.addRow(t("transfer.start"), self.start_spin)
        form.addRow(t("transfer.end"), self.end_spin)
        form.addRow(t("transfer.target"), self.target_combo)
        form.addRow(t("transfer.insert_after"), self.insert_spin)
        root.addLayout(form)
        self.hint = QLabel(); self.hint.setWordWrap(True); root.addWidget(self.hint)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept); buttons.rejected.connect(self.reject); root.addWidget(buttons)
        self._ok = buttons.button(QDialogButtonBox.StandardButton.Ok)
        self.source_combo.currentIndexChanged.connect(self._refresh_ranges)
        self.target_combo.currentIndexChanged.connect(self._refresh_ranges)
        self._refresh_ranges()

    def _refresh_ranges(self, *_args) -> None:
        source = self._stories.get(str(self.source_combo.currentData()), {})
        target = self._stories.get(str(self.target_combo.currentData()), {})
        source_count = len(source.get("nodes") or [])
        target_count = len(target.get("nodes") or [])
        for spin in (self.start_spin, self.end_spin):
            spin.setRange(1, max(1, source_count))
        self.insert_spin.setRange(0, target_count)
        same = self.source_combo.currentData() == self.target_combo.currentData()
        self._ok.setEnabled(source_count > 0 and not same)
        self.hint.setText(t("transfer.same_story") if same else t("transfer.ready"))

    def parameters(self) -> tuple[str, int, int, str, int]:
        return (
            str(self.source_combo.currentData()), self.start_spin.value() - 1,
            self.end_spin.value() - 1, str(self.target_combo.currentData()),
            self.insert_spin.value(),
        )
