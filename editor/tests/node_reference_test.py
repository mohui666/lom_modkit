# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtCore import Qt  # noqa: E402
from PySide6.QtWidgets import QApplication, QTabWidget  # noqa: E402

from i18n import set_language  # noqa: E402
import models  # noqa: E402
from node_reference import (  # noqa: E402
    KIND_DOCS,
    RUNTIME_API,
    NodeReferenceWidget,
    node_reference_html,
    reference_node_types,
)


class NodeReferenceTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def tearDown(self):
        set_language("zh_CN")
        models.refresh_labels()

    def test_reference_covers_every_node_and_every_field(self):
        self.assertEqual(set(reference_node_types()), set(models.NODE_TYPES))
        self.assertEqual(set(RUNTIME_API), set(models.NODE_TYPES))
        for node_type in models.NODE_TYPES:
            with self.subTest(node=node_type):
                page = node_reference_html(node_type)
                self.assertIn(node_type, page)
                self.assertIn("运行时接口", page)
                self.assertIn("<code>id</code>", page)
                self.assertIn("<code>type</code>", page)
                for key, _label, kind, _optional in models.NODE_SCHEMAS[node_type]["fields"]:
                    self.assertIn(f"<code>{key}</code>", page)
                    if not kind.startswith("enum:"):
                        self.assertIn(kind, KIND_DOCS)

    def test_search_finds_node_by_field_and_runtime_api(self):
        widget = NodeReferenceWidget()
        widget.search.setText("portrait")
        shown = {
            str(widget.node_list.item(i).data(Qt.ItemDataRole.UserRole))
            for i in range(widget.node_list.count())
        }
        self.assertIn("say", shown)
        self.assertIn("show", shown)
        widget.search.setText("ItemDatabase")
        shown = {
            str(widget.node_list.item(i).data(Qt.ItemDataRole.UserRole))
            for i in range(widget.node_list.count())
        }
        self.assertIn("item_check", shown)
        widget.close()

    def test_search_finds_localized_field_label_and_kind_description(self):
        set_language("zh_CN")
        models.refresh_labels()
        widget = NodeReferenceWidget()
        widget.search.setText("表情")
        shown = {
            str(widget.node_list.item(i).data(Qt.ItemDataRole.UserRole))
            for i in range(widget.node_list.count())
        }
        self.assertIn("say", shown)
        self.assertIn("show", shown)
        widget.search.setText("用户内容引用")
        self.assertGreater(widget.node_list.count(), 0)
        widget.close()

    def test_reference_chrome_is_localized(self):
        set_language("ja")
        models.refresh_labels()
        page = node_reference_html("say")
        self.assertIn("ランタイム API", page)
        self.assertIn("フィールド仕様", page)
        self.assertNotIn("运行时接口", page)

    def test_help_dialog_contains_guide_and_reference_tabs(self):
        from main import HelpDialog

        dialog = HelpDialog()
        tabs = dialog.findChild(QTabWidget)
        self.assertIsNotNone(tabs)
        self.assertEqual(tabs.count(), 2)
        self.assertIsInstance(tabs.widget(1), NodeReferenceWidget)
        dialog.close()


if __name__ == "__main__":
    unittest.main()
