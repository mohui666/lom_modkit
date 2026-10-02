# -*- coding: utf-8 -*-
"""步骤多选 + 复制/粘贴 的回归测试。

用法（在 editor/ 目录下）：
    .venv/Scripts/python tests/plus_multiselect_test.py

重点验证「只重映射副本」这条约束：粘贴时旧 id 会变新 id，如果改到了原剧情里
指向原节点的跳转，原步骤的去向就被改乱了——那是静默的数据损坏，必须测。
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from PySide6.QtCore import QItemSelectionModel  # noqa: E402  # type: ignore[reportMissingImports]
from PySide6.QtWidgets import QApplication  # noqa: E402  # type: ignore[reportMissingImports]

import main  # noqa: E402
import models  # noqa: E402
from i18n import t  # noqa: E402


def _sample_story(editor_data: dict) -> dict:
    """一段带内部跳转的剧情：choice1 一个去向在集合内、一个在集合外。"""
    return {
        "story_schema": models.STORY_SCHEMA,
        "id": "multi",
        "title": "多选复制",
        "mood": False,
        "start": "show1",
        "nodes": [
            {"id": "show1", "type": "show", "character": "girl4", "position": "M"},
            {"id": "say1", "type": "say", "character": "girl4", "text": "第一句"},
            {"id": "say2", "type": "say", "character": "girl4", "text": "第二句"},
            {
                "id": "choice1",
                "type": "choice",
                "text": "选一个",
                "options": [
                    {"text": "回到第一句", "goto": "say1"},   # 集合内 → 要跟着改
                    {"text": "直接结束", "goto": "end1"},     # 集合外 → 不能动
                ],
            },
            {"id": "end1", "type": "end"},
        ],
    }


def _select(win, node_indexes: list[int]) -> None:
    win.node_list.clearSelection()
    for index in node_indexes:
        row = win._list_row_for_node_index(index)
        assert row >= 0, f"节点下标 {index} 在列表里找不到"
        win.node_list.item(row).setSelected(True)
    win.node_list.setCurrentRow(win._list_row_for_node_index(node_indexes[-1]))


def test_chapter_row_not_selectable(win) -> None:
    """章节设置行不能被 Ctrl/Shift 选进多选集合。"""
    win.node_list.clearSelection()
    win.node_list.setCurrentRow(0)
    model = win.node_list.model()
    idx = model.index(0, 0)
    flag = win.node_list.selectionCommand(idx, None)
    assert flag == QItemSelectionModel.SelectionFlag.NoUpdate, (
        f"章节设置行不允许选中，实际返回 {flag}"
    )
    assert win._selected_node_indexes() == [], "章节行不该出现在选中步骤里"
    print("[章节行] 不参与多选")


def test_multi_copy_paste(win) -> None:
    nodes = win.story["nodes"]
    # 选中 say1 / say2 / choice1 三个，外加章节行（应被忽略）
    win.node_list.item(0).setSelected(True)
    _select(win, [1, 2, 3])
    assert len(win._selected_node_indexes()) == 3, (
        f"应选中 3 个步骤，实际 {win._selected_node_indexes()}"
    )
    win._copy_selected_nodes()
    assert len(win._node_clipboard) == 3, "剪贴板应有 3 个步骤"

    before_ids = [n["id"] for n in nodes]
    original_gotos = [o["goto"] for o in nodes[3]["options"]]

    # 以 choice1 为插入锚点（粘贴到它后面）
    _select(win, [3])
    win._paste_nodes()

    after_ids = [n["id"] for n in win.story["nodes"]]
    assert len(after_ids) == len(before_ids) + 3, f"节点数不对：{after_ids}"
    assert after_ids[:3] == before_ids[:3], "原节点顺序不应被改动"
    pasted = win.story["nodes"][4:7]
    pasted_ids = [n["id"] for n in pasted]
    assert pasted_ids == ["say3", "say4", "choice2"], f"新编号不对：{pasted_ids}"
    assert len(set(after_ids)) == len(after_ids), f"出现重复编号：{after_ids}"

    # 副本内部的跳转要指向副本自己
    clone_choice = pasted[2]
    assert clone_choice["options"][0]["goto"] == "say3", (
        f"副本内部跳转应重映射到 say3，实际 {clone_choice['options'][0]['goto']}"
    )
    # 指向集合外的跳转必须保持原样
    assert clone_choice["options"][1]["goto"] == "end1", (
        f"集合外跳转不应被改动，实际 {clone_choice['options'][1]['goto']}"
    )
    # 原节点完全不受影响
    assert win.story["nodes"][3]["id"] == "choice1"
    assert [o["goto"] for o in win.story["nodes"][3]["options"]] == original_gotos, (
        f"原 choice1 的去向被改乱了：{[o['goto'] for o in win.story['nodes'][3]['options']]}"
    )
    print(f"[粘贴] 生成 {pasted_ids}，副本内部跳转已重映射，原剧情不受影响")


def test_paste_batch_ids_unique(win) -> None:
    """同一批里多个同类型步骤不能拿到同一个编号。"""
    nodes = win.story.setdefault("nodes", [])
    nodes.extend([
        {"id": "say9", "type": "say", "text": "a"},
        {"id": "say10", "type": "say", "text": "b"},
        {"id": "say11", "type": "say", "text": "c"},
    ])
    win._refresh_all(select_row=len(nodes) - 1)
    _select(win, [len(nodes) - 3, len(nodes) - 2, len(nodes) - 1])
    win._copy_selected_nodes()
    before = len(nodes)  # nodes 是活引用，粘贴后长度会变，必须先记下来
    win._paste_nodes()
    ids = [n["id"] for n in win.story["nodes"]]
    assert len(set(ids)) == len(ids), f"批量粘贴出现重复编号：{ids}"
    assert len(ids) == before + 3, f"节点数不对：期望 {before + 3}，实际 {len(ids)}"
    print(f"[批量] 3 个同类步骤一次粘贴，编号唯一：{ids[-3:]}")


def test_empty_clipboard(win) -> None:
    win._node_clipboard = []
    before = len(win.story["nodes"])
    win._paste_nodes()
    assert len(win.story["nodes"]) == before, "空剪贴板不该改动剧情"
    print("[空剪贴板] 粘贴被正确忽略")


def test_copy_then_switch_story(win) -> None:
    """跨章节粘贴：复制后切到另一个章节，仍能粘贴。"""
    win._stories["other"] = {
        "story_schema": models.STORY_SCHEMA,
        "id": "other",
        "title": "另一章",
        "mood": False,
        "start": "end1",
        "nodes": [{"id": "end1", "type": "end"}],
    }
    win._story_paths["other"] = None
    win._current_id = "other"
    win._refresh_all()
    assert win.story["id"] == "other"

    _select(win, [0])
    win._copy_selected_nodes()
    win._paste_nodes()
    ids = [n["id"] for n in win.story["nodes"]]
    assert len(ids) == 2, f"跨章节粘贴失败：{ids}"
    assert ids[0] == "end1" and ids[1].startswith("end"), f"编号异常：{ids}"
    print(f"[跨章节] 从其他章节复制的步骤粘贴成功：{ids}")


def main_fn() -> int:
    app = QApplication([])
    editor_data, is_fallback = models.load_editor_data(main.PROJECT_ROOT)
    win = main.MainWindow(editor_data, is_fallback)
    win._prompt_on_discard = False
    win.show()
    app.processEvents()

    win.story = _sample_story(editor_data)
    win._refresh_all()

    test_chapter_row_not_selectable(win)
    test_multi_copy_paste(win)
    test_paste_batch_ids_unique(win)
    test_empty_clipboard(win)
    test_copy_then_switch_story(win)

    win.close()
    print("\nplus_multiselect_test 全部通过")
    return 0


if __name__ == "__main__":
    sys.exit(main_fn())
