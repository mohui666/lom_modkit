# -*- coding: utf-8 -*-
"""起始步骤（start）跟随列表第 0 位的回归测试。

用法（在 editor/ 目录下）：
    .venv/Scripts/python tests/start_sync_test.py

背景：把某一步拖/移到最前（列表第 0 位）时，若不同步 story["start"]，右侧流程图
仍从旧 start 推演，会把新的第 0 位标成「无法到达」。这让「把切换场景/设置时间/
自动存档/自由模式触发设成第一步」这类合法剧情（例如从自由模式触发进入的 story）
根本做不出来。
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from PySide6.QtWidgets import QApplication  # noqa: E402  # type: ignore[reportMissingImports]

import main  # noqa: E402
import models  # noqa: E402
from story_graph import analyze_story  # noqa: E402


def _fresh(ed):
    win = main.MainWindow(ed, False)
    win._prompt_on_discard = False
    win.show()
    QApplication.processEvents()
    return win


def test_drag_to_front_follows_start(ed) -> None:
    """把 scene 拖到最前，start 应指向它，且没有不可达节点。"""
    for node_type, label in (
        ("scene", "场景"), ("time", "时间"), ("autosave", "存档"), ("free_trigger", "触发"),
    ):
        win = _fresh(ed)
        win._select_node_index(0)
        win._add_node(node_type)
        QApplication.processEvents()
        # 新节点落在 show 之后（index 1），拖到最前
        win._on_steps_moved(1, 0)
        QApplication.processEvents()
        first = win.story["nodes"][0]
        assert win.story["start"] == first["id"], (
            f"{label}: start 应跟随新的第 0 位 {first['id']}，实际 {win.story['start']}"
        )
        assert not analyze_story(win.story).unreachable, f"{label}: 不应有不可达节点"
        win.close()
    print("[拖到最前] 4 种节点设为首步后 start 跟随、无不可达")


def test_move_up_follows_start(ed) -> None:
    """上移按钮把 scene 提到最前，start 也应跟随。"""
    win = _fresh(ed)
    win._select_node_index(0)
    win._add_node("scene")
    QApplication.processEvents()
    win._select_node_index(1)  # scene
    win._move_node(-1)
    QApplication.processEvents()
    assert win.story["start"] == "scene1", f"start 应为 scene1，实际 {win.story['start']}"
    assert not analyze_story(win.story).unreachable
    win.close()
    print("[上移] start 跟随新的第 0 位")


def test_branched_start_not_disturbed(ed) -> None:
    """start 指向别处（分支入口）时，往最前拖节点不应改动 start。"""
    win = _fresh(ed)
    # 构造：music(0) → show(1) → say(2)，start 指向 say（入口在中间）
    nodes = [
        {"id": "music1", "type": "music", "name": "a"},
        {"id": "show1", "type": "show", "character": "artist1", "position": "M"},
        {"id": "say1", "type": "say", "character": "artist1", "text": "x"},
        {"id": "end1", "type": "end"},
    ]
    win._install_project(
        {"main": {"story_schema": models.STORY_SCHEMA, "id": "main", "start": "say1",
                  "nodes": nodes}},
        {"main": None}, "main", "untitled", None,
    )
    QApplication.processEvents()
    # 把 end 拖到最前，start 仍应是 say1（不在第 0 位）
    win._on_steps_moved(3, 0)
    QApplication.processEvents()
    assert win.story["start"] == "say1", (
        f"分支入口不应被改动，start 仍应 say1，实际 {win.story['start']}"
    )
    win.close()
    print("[分支入口] start 指向别处时不被拖拽改动")


def main_fn() -> int:
    app = QApplication([])
    ed, fb = models.load_editor_data(main.PROJECT_ROOT)
    test_drag_to_front_follows_start(ed)
    test_move_up_follows_start(ed)
    test_branched_start_not_disturbed(ed)
    print("\nstart_sync_test 全部通过")
    return 0


if __name__ == "__main__":
    sys.exit(main_fn())
