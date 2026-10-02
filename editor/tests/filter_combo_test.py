# -*- coding: utf-8 -*-
"""长清单下拉框（筛选框）的取值回归测试。

用法（在 editor/ 目录下）：
    .venv/Scripts/python tests/filter_combo_test.py

背景：人物 400+ 条、背景 150+ 条，超过阈值（COMBO_VISIBLE_ITEMS）的下拉框会被
强制变成可输入筛选框。用户为了在长清单里找人必然要打字，而 QComboBox 会把输入框
里的文字当作 currentText 抛出来——如果直接取它当值，节点里的人物就会变成「武」，
导出时报「人物必须保存内部 ID，不能使用下拉显示文字」。

所以这里守住两条：
1. 输入框里的文字只是筛选词，绝不写进节点；
2. 点选条目、或手输的内容能对上清单时，仍然要正确写回。
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from PySide6.QtWidgets import (  # noqa: E402  # type: ignore[reportMissingImports]
    QApplication,
    QComboBox,
)

import main  # noqa: E402
import models  # noqa: E402
import node_form  # noqa: E402


def type_into(combo: QComboBox, text: str) -> None:
    """模拟用户真的在输入框里打字。

    只 setText 不会触发 textEdited（那是「用户编辑」才有的信号），所以三个动作
    都要做：改编辑框内容、抛用户编辑信号、抛 QComboBox 自己会抛的 currentTextChanged。
    最后那个正是原来出问题的那条路径。
    """
    assert combo.lineEdit() is not None, "该下拉框不是可编辑的，无法输入"
    combo.lineEdit().setText(text)
    combo.lineEdit().textEdited.emit(text)
    combo.currentTextChanged.emit(text)


def longest_combo_for(form, value) -> QComboBox | None:
    """找到包含该取值的下拉框（取条目最多的那个，即真正在用的长清单）。"""
    best: QComboBox | None = None
    for combo in form.findChildren(QComboBox):
        if combo.count() < 2:
            continue
        if any(combo.itemData(i) == value for i in range(combo.count())):
            if best is None or combo.count() > best.count():
                best = combo
    return best


class FilterComboTest:
    def __init__(self, win):
        self.win = win
        self.ed = win.editor_data

    def _form_for(self, node_type: str, node_id: str = "x1", preset=None) -> dict:
        node = models.new_node(node_type, node_id, self.ed)
        if preset:
            node.update(preset)
        self.win.form.set_node(node)
        QApplication.processEvents()
        return node

    def test_filter_typing_never_writes(self):
        """在长清单里打字筛选，节点取值必须原地不动。"""
        # 先给每个字段填一个确定在清单里的值：新建节点时 view 是空的，
        # 按空值去找下拉框会误匹配到带空选项的别的控件。
        cases = [
            ("show", "character", "artist1", "武"),
            ("say", "character", "artist1", "武"),
            ("scene", "view", "alchemy", "炼"),
        ]
        checked = 0
        for node_type, key, seed, typed in cases:
            node = self._form_for(node_type, f"c_{node_type}", {key: seed})
            combo = longest_combo_for(self.win.form, seed)
            assert combo is not None, f"{node_type}.{key} 找不到长清单下拉框"
            before = node.get(key)
            type_into(combo, typed)
            QApplication.processEvents()
            assert node.get(key) == before, (
                f"{node_type}.{key}: 输入筛选词 {typed!r} 后取值被污染成 "
                f"{node.get(key)!r}（原本 {before!r}）"
            )
            combo.finish_typing()
            QApplication.processEvents()
            assert node.get(key) == before, (
                f"{node_type}.{key}: 结束输入后取值仍是 {node.get(key)!r}，应为 {before!r}"
            )
            checked += 1
        assert checked >= 3, f"只验证到 {checked} 个下拉框，用例可能失效了"
        print(f"[筛选输入] {checked} 个长清单下拉框打字后取值均未被污染")

    def test_picking_item_still_writes_id(self):
        """修复不能把「点选条目」也一起弄坏。"""
        for node_type, key, seed in (
            ("show", "character", "artist1"),
            ("scene", "view", "alchemy"),
        ):
            node = self._form_for(node_type, f"p_{node_type}", {key: seed})
            combo = longest_combo_for(self.win.form, seed)
            assert combo is not None, f"{node_type}.{key} 找不到下拉框"
            target = None
            for i in range(combo.count()):
                if combo.itemData(i) and combo.itemData(i) != seed:
                    target = i
                    break
            assert target is not None
            combo.setCurrentIndex(target)
            QApplication.processEvents()
            assert node.get(key) == combo.itemData(target), (
                f"{node_type}.{key}: 点选后应写入 id {combo.itemData(target)!r}，"
                f"实际 {node.get(key)!r}"
            )
        print("[点选条目] 仍然正确写入内部 id")

    def test_typed_exact_id_commits_on_finish(self):
        """手输的内容正好就是某个 id 时，结束输入应当认下来。"""
        node = self._form_for("show", "t_show")
        combo = longest_combo_for(self.win.form, node.get("character"))
        assert combo is not None
        # 找一个和数据不同的目标
        target_index = None
        for i in range(combo.count()):
            data = combo.itemData(i)
            if data and data != node.get("character"):
                target_index = i
                break
        assert target_index is not None
        wanted = str(combo.itemData(target_index))
        type_into(combo, wanted)
        QApplication.processEvents()
        combo.finish_typing()
        QApplication.processEvents()
        assert node.get("character") == wanted, (
            f"手输 id {wanted!r} 结束输入后应被采用，实际 {node.get('character')!r}"
        )
        print(f"[手输 id] 结束输入后正确采用 {wanted!r}")

    def test_completer_backfill_commits(self):
        """completer 回填完整显示名（文字变了、index 没变）也应写回正确 id。

        这是「下拉框选人物不刷新、要删一个字才刷新」的根因回归：自动补全把完整
        显示名填进输入框但不设 index，若把这段文字当筛选词返回旧值，人物就不会更新。
        """
        node = self._form_for("show", "cb_show", {"character": "artist1"})
        combo = longest_combo_for(self.win.form, "artist1")
        assert combo is not None
        target_index = None
        for i in range(combo.count()):
            data = combo.itemData(i)
            if data and data != node.get("character"):
                target_index = i
                break
        assert target_index is not None
        full = combo.itemText(target_index)
        wanted = str(combo.itemData(target_index))
        # 只改文字、不动 index，模拟 completer 回填
        combo.lineEdit().setText(full)
        QApplication.processEvents()
        assert node.get("character") == wanted, (
            f"completer 回填 {full!r} 后应写回 {wanted!r}，实际 {node.get('character')!r}"
        )
        print(f"[completer 回填] 文字对得上条目时正确写回 {wanted!r}")

    def test_filter_text_is_restored_after_finish(self):
        """筛选词对不上任何条目时，输入框要还原成真正的选中项，不能留着误导。"""
        node = self._form_for("show", "r_show")
        combo = longest_combo_for(self.win.form, node.get("character"))
        assert combo is not None
        before_text = combo.currentText()
        type_into(combo, "绝不可能匹配的词zzz")
        QApplication.processEvents()
        combo.finish_typing()
        QApplication.processEvents()
        assert combo.currentText() == before_text, (
            f"筛选词应被还原为 {before_text!r}，实际 {combo.currentText()!r}"
        )
        print("[还原显示] 对不上的筛选词已还原为选中项")

    def test_goto_combo_still_accepts_hand_written_id(self):
        """跳转框允许填「尚未创建的编号」，这条既有用法不能被误伤。"""
        story = {"nodes": [{"id": f"n{i}", "type": "end"} for i in range(1, 21)]}
        story["nodes"][0] = {"id": "n1", "type": "say", "text": "x"}
        self.win.story = story
        node = {"id": "manual", "type": "say", "text": "x", "character": "artist1"}
        self.win.story["nodes"].append(node)
        self.win.form.set_context(
            self.ed,
            [n["id"] for n in story["nodes"]],
            [self.win._current_id or "main"],
        )
        self.win.form.set_node(node)
        QApplication.processEvents()
        combo = None
        for candidate in self.win.form.findChildren(QComboBox):
            if isinstance(candidate, node_form._GotoCombo):
                combo = candidate
                break
        if combo is None:
            print("[跳转框] 该节点没有独立 goto 下拉框，跳过")
            return
        type_into(combo, "will_create_later")
        QApplication.processEvents()
        combo.finish_typing()
        QApplication.processEvents()
        assert node.get("goto") == "will_create_later", (
            f"跳转框应接受手写编号，实际 {node.get('goto')!r}"
        )
        print("[跳转框] 手写编号仍可提交")

    def test_short_combo_keeps_direct_typing(self):
        """短清单没有被强制改成筛选框，手输仍然立即取值（不改变既有习惯）。"""
        form = node_form.NodeForm()
        form.set_context(self.ed, ["n1"], ["main", "extra"])
        node = models.new_node("end", "e1", self.ed)
        form._node = node
        widget = form._make_widget(node, "next_script", "story_ref")
        combo = None
        if isinstance(widget, QComboBox):
            combo = widget
        elif widget is not None:
            found = widget.findChildren(QComboBox)
            combo = found[0] if found else None
        assert combo is not None, "story_ref 应生成下拉框"
        assert not isinstance(combo, node_form._FilterCombo), (
            "短清单不该被当成筛选框，否则会改变既有手输行为"
        )
        combo.setCurrentText("brand_new_story")
        QApplication.processEvents()
        assert node.get("next_script") == "brand_new_story", (
            f"短清单手输应直接取值，实际 {node.get('next_script')!r}"
        )
        print("[短清单] 手输仍直接取值（未被筛选框规则误伤）")


def main_fn() -> int:
    app = QApplication([])
    editor_data, is_fallback = models.load_editor_data(main.PROJECT_ROOT)
    win = main.MainWindow(editor_data, is_fallback)
    win._prompt_on_discard = False
    win.show()
    app.processEvents()

    suite = FilterComboTest(win)
    suite.test_filter_typing_never_writes()
    suite.test_picking_item_still_writes_id()
    suite.test_typed_exact_id_commits_on_finish()
    suite.test_completer_backfill_commits()
    suite.test_filter_text_is_restored_after_finish()
    suite.test_goto_combo_still_accepts_hand_written_id()
    suite.test_short_combo_keeps_direct_typing()
    win.close()
    print("\nfilter_combo_test 全部通过")
    return 0


if __name__ == "__main__":
    sys.exit(main_fn())
