# -*- coding: utf-8 -*-
"""步骤编号重命名、拖动排序、切语言保持选中。"""

from __future__ import annotations

import os
import sys
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

EDITOR = Path(__file__).resolve().parent.parent
if str(EDITOR) not in sys.path:
    sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication  # noqa: E402

from i18n import set_language  # noqa: E402
import main  # noqa: E402
import models  # noqa: E402
import story_api  # noqa: E402


def _story() -> dict:
    return {
        "id": "main",
        "title": "t",
        "start": "n1",
        "mood": False,
        "nodes": [
            {"id": "n1", "type": "show", "character": "player", "position": "M"},
            {
                "id": "n2",
                "type": "choice",
                "options": [
                    {"text": "a", "goto": "n3"},
                    {"text": "b", "goto": "n4"},
                ],
            },
            {"id": "n3", "type": "say", "text": "ok", "goto": "n4"},
            {
                "id": "n4",
                "type": "dice",
                "check": "C1",
                "options": [
                    {"goto_失败": "n3", "goto_成功": "n5", "goto_大成功": "n5"}
                ],
            },
            {
                "id": "n5",
                "type": "branch",
                "source": "mod",
                "cases": [{"value": 1, "goto": "n3"}],
            },
        ],
    }


def test_rename_updates_all_refs():
    story = _story()
    changed = models.rename_node(story, "n3", "ending")
    assert story["nodes"][2]["id"] == "ending"
    assert story["nodes"][1]["options"][0]["goto"] == "ending"
    assert story["nodes"][3]["options"][0]["goto_失败"] == "ending"
    assert story["nodes"][4]["cases"][0]["goto"] == "ending"
    assert story["start"] == "n1"
    assert changed >= 4
    models.rename_node(story, "n1", "open")
    assert story["start"] == "open"
    try:
        models.rename_node(story, "open", "n2")
        raise AssertionError("占用编号应失败")
    except ValueError:
        pass
    try:
        models.rename_node(story, "open", "坏 id")
        raise AssertionError("非法编号应失败")
    except ValueError:
        pass
    try:
        models.rename_node(story, "open", "node-with-dash")
        raise AssertionError("节点编号含短横线应失败")
    except ValueError:
        pass
    assert models.make_node_id(story, prefix="bad-prefix") == "n1"
    print("[node] 重命名同步 start/goto/选项/骰子/分支 OK")


def test_reorder_node():
    story = _story()
    dest = models.reorder_node(story, 0, 3)
    ids = [n["id"] for n in story["nodes"]]
    assert ids == ["n2", "n3", "n1", "n4", "n5"], ids
    assert dest == 2
    dest = models.reorder_node(story, 2, 0)
    ids = [n["id"] for n in story["nodes"]]
    assert ids[0] == "n1"
    assert dest == 0
    print("[node] 拖动重排下标 OK")


def test_story_api_rename():
    story = _story()
    node = story_api.rename_node(story, "n4", "roll")
    assert node["id"] == "roll"
    assert story["nodes"][2]["goto"] == "roll"
    print("[node] story_api.rename_node OK")


def test_language_keeps_selection():
    app = QApplication.instance() or QApplication([])
    editor_data, _ = models.load_editor_data(main.PROJECT_ROOT)
    win = main.MainWindow(editor_data, False)
    win._prompt_on_discard = False
    win._select_node_index(1)
    assert win._selected_node_index() == 1
    before = win.story["nodes"][1]["id"]
    set_language("zh_CN")
    win._change_language("zh_TW")
    assert win._selected_node_index() == 1, win._selected_node_index()
    assert win.story["nodes"][1]["id"] == before
    win._change_language("zh_CN")
    assert win._selected_node_index() == 1
    print("[node] 切语言保持当前步骤 OK")


def test_window_rename_and_drag():
    app = QApplication.instance() or QApplication([])
    editor_data, _ = models.load_editor_data(main.PROJECT_ROOT)
    win = main.MainWindow(editor_data, False)
    win._prompt_on_discard = False
    first = win.story["nodes"][0]["id"]
    second = win.story["nodes"][1]["id"]
    win.story["nodes"][0]["goto"] = second
    win._select_node_index(1)
    win._apply_node_rename(second, "talk")
    assert win.story["nodes"][1]["id"] == "talk"
    assert win.story["nodes"][0]["goto"] == "talk"
    assert win._selected_node_index() == 1
    win._on_steps_moved(1, 0)
    ids = [n["id"] for n in win.story["nodes"]]
    assert ids[0] == "talk"
    assert ids[1] == first
    assert win.node_list.count() == 1 + len(win.story["nodes"])
    listed = [win.node_list.item(i).text() for i in range(1, win.node_list.count())]
    assert any("talk" in line for line in listed), listed
    # 原地放下也必须把列表从数据重建，不能让 Qt MoveAction 删掉那一行
    before_count = win.node_list.count()
    win._on_steps_moved(0, 0)
    assert win.node_list.count() == before_count
    assert win.story["nodes"][0]["id"] == "talk"
    print("[node] 窗口重命名与拖动 OK")


if __name__ == "__main__":
    test_rename_updates_all_refs()
    test_reorder_node()
    test_story_api_rename()
    test_language_keeps_selection()
    test_window_rename_and_drag()
    print("node_ops tests OK")
