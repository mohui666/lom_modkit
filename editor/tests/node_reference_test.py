# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication, QTabWidget, QTextBrowser  # noqa: E402

from i18n import set_language  # noqa: E402
import models  # noqa: E402
from node_reference import (  # noqa: E402
    DocumentationDialog,
    KIND_DOCS,
    RUNTIME_API,
    NodeReferenceWidget,
    SoftwareDocumentationWidget,
    documentation_home_html,
    node_reference_html,
    reference_node_types,
)


class NodeReferenceTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def tearDown(self):
        set_language("chs")
        models.refresh_labels()

    def test_reference_covers_every_node_and_every_field(self):
        self.assertEqual(set(reference_node_types()), set(models.NODE_TYPES))
        self.assertEqual(set(RUNTIME_API), set(models.NODE_TYPES))
        for node_type in models.NODE_TYPES:
            with self.subTest(node=node_type):
                page = node_reference_html(node_type)
                self.assertIn(node_type, page)
                self.assertIn("运行时接口", page)
                self.assertIn("参数作用", page)
                self.assertIn("<code>id</code>", page)
                self.assertIn("<code>type</code>", page)
                for key, _label, kind, _optional in models.NODE_SCHEMAS[node_type]["fields"]:
                    self.assertIn(f"<code>{key}</code>", page)
                    if not kind.startswith("enum:"):
                        self.assertIn(kind, KIND_DOCS)

    def test_search_finds_node_by_field_and_runtime_api(self):
        widget = NodeReferenceWidget()
        widget.search.setText("portrait")
        shown = set(widget.visible_node_types())
        self.assertIn("say", shown)
        self.assertIn("show", shown)
        widget.search.setText("ItemDatabase")
        shown = set(widget.visible_node_types())
        self.assertIn("item_check", shown)
        widget.close()

    def test_search_finds_localized_field_label_and_kind_description(self):
        set_language("chs")
        models.refresh_labels()
        widget = NodeReferenceWidget()
        widget.search.setText("表情")
        shown = set(widget.visible_node_types())
        self.assertIn("say", shown)
        self.assertIn("show", shown)
        widget.search.setText("用户内容引用")
        self.assertGreater(len(widget.visible_node_types()), 0)
        widget.close()

    def test_reference_chrome_is_localized(self):
        expectations = {
            "chs": ("作者文档", "运行时接口", "参数作用"),
            "cht": ("作者文件", "執行階段介面", "參數作用"),
            "ja": ("作者向けドキュメント", "ランタイム API", "パラメーターの効果"),
            "ko": ("제작자 문서", "런타임 API", "매개변수 효과"),
        }
        for locale, (home_title, api_title, effect_title) in expectations.items():
            with self.subTest(locale=locale):
                set_language(locale)
                models.refresh_labels()
                self.assertIn(home_title, documentation_home_html())
                page = node_reference_html("say")
                self.assertIn(api_title, page)
                self.assertIn(effect_title, page)

    def test_combat_battle_and_dice_parameter_effects_are_specific(self):
        combat = node_reference_html("combat")
        self.assertIn("原版一对一决斗的角色与场景底板", combat)
        self.assertIn("原版 AI", combat)
        battle = node_reference_html("battle")
        self.assertIn("三方阵容", battle)
        self.assertIn("上一场配置残留", battle)
        dice = node_reference_html("dice")
        self.assertIn("0 到该值之间", dice)
        self.assertIn("最后一档接收剩余点数", dice)

    def test_documentation_is_separate_from_quick_help(self):
        from main import HelpDialog

        help_dialog = HelpDialog()
        self.assertIsNone(help_dialog.findChild(QTabWidget))
        self.assertIsNotNone(help_dialog.findChild(QTextBrowser))
        docs = DocumentationDialog()
        self.assertFalse(docs.isModal())
        self.assertIsInstance(docs.software, SoftwareDocumentationWidget)
        self.assertIsInstance(docs.reference, NodeReferenceWidget)
        self.assertEqual(docs.tabs.count(), 2)
        self.assertEqual(docs.tabs.tabText(0), "软件使用文档")
        self.assertEqual(docs.tabs.tabText(1), "脚本 / API 文档")
        self.assertIn("作者文档", documentation_home_html())
        help_dialog.close()
        docs.close()

    def test_documentation_has_tree_search_and_browser_history(self):
        widget = NodeReferenceWidget()
        self.assertEqual(widget.current_node_type(), "")
        widget._navigate("say")
        self.assertEqual(widget.current_node_type(), "say")
        self.assertTrue(widget.back_button.isEnabled())
        widget.go_back()
        self.assertEqual(widget.current_node_type(), "")
        self.assertTrue(widget.forward_button.isEnabled())
        widget.go_forward()
        self.assertEqual(widget.current_node_type(), "say")
        widget.close()

    def test_software_documentation_has_separate_articles_and_search(self):
        widget = SoftwareDocumentationWidget()
        self.assertIn("overview", widget.visible_page_ids())
        self.assertGreater(len(widget.visible_page_ids()), 4)
        widget.search.setText("F5")
        self.assertGreater(len(widget.visible_page_ids()), 0)
        widget.close()


if __name__ == "__main__":
    unittest.main()
