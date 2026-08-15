# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication, QTableWidget  # noqa: E402

import models  # noqa: E402
from node_form import NodeForm  # noqa: E402
from preview import _hint_text  # noqa: E402
from story_graph import analyze_story  # noqa: E402


class GameplayCompositeEditorTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_battle_setup_schema_table_summary_and_flow(self):
        node = models.new_node("battle_setup", "setup", models.FALLBACK_EDITOR_DATA)
        node.update({
            "enemy": "Bandit", "skills": [{"key": "Skill_A", "index": 2, "active": 1}],
            "goto": "end",
        })
        self.assertIn("battle_setup_skills", {
            kind for _key, _label, kind, _optional
            in models.NODE_SCHEMAS["battle_setup"]["fields"]
        })
        self.assertIn("Bandit", models.node_summary(node))
        self.assertIn("我方技能 1 项", _hint_text(node, models.FALLBACK_EDITOR_DATA))
        graph = analyze_story({
            "start": "setup", "nodes": [node, {"id": "end", "type": "end"}],
        })
        self.assertIn(("setup", "end"), {(edge.source, edge.target) for edge in graph.edges})
        form = NodeForm()
        form.set_context(models.FALLBACK_EDITOR_DATA, ["setup", "end"])
        form.set_node(node)
        self.assertTrue(any(table.columnCount() == 3 for table in form.findChildren(QTableWidget)))

    def test_reward_schema_table_and_summary(self):
        node = {
            "id": "reward", "type": "reward", "entries": [
                {"kind": "stat", "key": "Money", "amount": 500},
                {"kind": "flag", "key": "won"},
            ], "goto": "end",
        }
        fields = {key: kind for key, _label, kind, _optional in models.NODE_SCHEMAS["reward"]["fields"]}
        self.assertEqual(fields["entries"], "reward_entries")
        self.assertIn("2 项", models.node_summary(node))
        self.assertIn("2 项", _hint_text(node, models.FALLBACK_EDITOR_DATA))
        form = NodeForm()
        form.set_context(models.FALLBACK_EDITOR_DATA, ["reward", "end"])
        form.set_node(node)
        self.assertTrue(any(table.columnCount() == 4 for table in form.findChildren(QTableWidget)))

    def test_custom_shop_schema_table_summary_and_ai_list_kind(self):
        node = models.new_node("custom_shop", "shop", models.FALLBACK_EDITOR_DATA)
        node.update({
            "discount": 1,
            "items": [
                {"category": "book", "item": "Book_A", "count": 2},
                {
                    "category": "misc", "item": "Misc_B", "count": 1,
                    "condition": {"source": "mod", "key": "OPEN"},
                },
            ],
            "goto": "end",
        })
        fields = {
            key: kind for key, _label, kind, _optional
            in models.NODE_SCHEMAS["custom_shop"]["fields"]
        }
        self.assertEqual(fields["items"], "custom_shop_items")
        self.assertIn("2 件", models.node_summary(node))
        form = NodeForm()
        form.set_context(models.FALLBACK_EDITOR_DATA, ["shop", "end"])
        form.set_node(node)
        self.assertTrue(any(table.columnCount() == 6 for table in form.findChildren(QTableWidget)))

    def test_five_gameplay_checks_have_two_real_graph_edges(self):
        check_types = (
            "stat_check", "affinity_check", "item_check", "talent_check", "flag_check",
        )
        for node_type in check_types:
            with self.subTest(node_type=node_type):
                node = models.new_node(node_type, "check", models.FALLBACK_EDITOR_DATA)
                node["success"] = "ok"
                node["failure"] = "bad"
                graph = analyze_story({
                    "start": "check",
                    "nodes": [
                        node, {"id": "ok", "type": "end"}, {"id": "bad", "type": "end"},
                    ],
                })
                edges = {(edge.source, edge.target, edge.label) for edge in graph.edges}
                self.assertIn(("check", "ok", "成功"), edges)
                self.assertIn(("check", "bad", "失败"), edges)
                self.assertIn("成功→ok", models.node_summary(node))


if __name__ == "__main__":
    unittest.main(verbosity=2)
