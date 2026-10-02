# -*- coding: utf-8 -*-
"""结束剧情清空重置 + 自由模式触发旗标条件下拉的回归测试。

用法（在 editor/ 目录下）：
    .venv/Scripts/python tests/form_field_test.py

背景：
1. end 节点的 next_script 清空后变成空字符串 ""，校验会报「必须是脚本 id」；
   应提供「返回自由模式」选项，且清空时移除字段。
2. free_trigger 的 when_flag_set / when_flag_clear 应是下拉框，列出剧情里
   flag 步骤设过的旗标，可留空（不限）。
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from PySide6.QtWidgets import QApplication, QComboBox  # noqa: E402  # type: ignore[reportMissingImports]

import main  # noqa: E402
import models  # noqa: E402
import story_api  # noqa: E402


def _combo_of(form, first_text):
    for c in form.findChildren(QComboBox):
        if c.count() and c.itemText(0) == first_text:
            return c
    return None


def _fresh(ed):
    win = main.MainWindow(ed, False)
    win._prompt_on_discard = False
    win.show()
    QApplication.processEvents()
    return win


def test_end_next_script_clearing(ed) -> None:
    win = _fresh(ed)
    node = models.new_node("end", "e1", ed)
    win.form.set_node(node)
    QApplication.processEvents()
    combo = _combo_of(win.form, "（返回自由模式）")
    assert combo is not None, "end.next_script 应有「返回自由模式」下拉选项"

    # 选「返回自由模式」→ 移除字段
    combo.setCurrentIndex(0)
    QApplication.processEvents()
    assert "next_script" not in node, "选返回自由模式应移除 next_script 字段"

    # 手填一个脚本 id → 写入
    combo.setCurrentText("main")
    QApplication.processEvents()
    assert node.get("next_script") == "main"

    # 清空 → 移除字段，且校验不报错
    combo.setCurrentText("")
    QApplication.processEvents()
    assert "next_script" not in node, "清空应移除 next_script 字段"
    story = {"story_schema": models.STORY_SCHEMA, "id": "t", "start": "e1",
             "nodes": [node, {"id": "e2", "type": "end"}]}
    errors, _ = story_api.check_story(story)
    assert not errors, f"清空后不应报错：{errors}"
    win.close()
    print("[结束剧情] 清空自动移除字段，校验通过")


def test_free_trigger_flag_dropdown(ed) -> None:
    win = _fresh(ed)
    # 剧情里放两个 flag 步骤
    win.story["nodes"].append({"id": "f1", "type": "flag", "flag": "MOD_DONE"})
    win.story["nodes"].append({"id": "f2", "type": "flag", "flag": "OPEN_SECRET"})
    node = models.new_node("free_trigger", "ft1", ed)
    win.form.set_node(node)
    QApplication.processEvents()

    combos = [c for c in win.form.findChildren(QComboBox) if c.count() and c.itemText(0) == "（不限）"]
    assert len(combos) >= 2, "when_flag_set / when_flag_clear 应是「（不限）」开头的下拉"
    flags = [combos[0].itemText(i) for i in range(combos[0].count())]
    assert "MOD_DONE" in flags and "OPEN_SECRET" in flags, f"下拉应列出剧情 flag：{flags}"

    # 选择某个旗标 → 写入节点
    idx = flags.index("MOD_DONE")
    combos[0].setCurrentIndex(idx)
    QApplication.processEvents()
    assert node.get("when_flag_set") == "MOD_DONE", (
        f"选择旗标应写入 MOD_DONE，实际 {node.get('when_flag_set')!r}"
    )
    # 选「不限」→ 移除字段
    combos[0].setCurrentIndex(0)
    QApplication.processEvents()
    assert "when_flag_set" not in node, "选不限应移除 when_flag_set 字段"
    win.close()
    print("[旗标条件] 下拉列出剧情 flag，选不限可移除")


def main_fn() -> int:
    app = QApplication([])
    ed, fb = models.load_editor_data(main.PROJECT_ROOT)
    test_end_next_script_clearing(ed)
    test_free_trigger_flag_dropdown(ed)
    print("\nform_field_test 全部通过")
    return 0


if __name__ == "__main__":
    sys.exit(main_fn())
