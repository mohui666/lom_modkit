# -*- coding: utf-8 -*-
from __future__ import annotations

import sys
import unittest
from pathlib import Path

EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

import models  # noqa: E402
from preview import simulate_stage  # noqa: E402
from story_graph import analyze_story  # noqa: E402


class CombatEditorTest(unittest.TestCase):
    def test_schema_default_summary_and_picker_contract(self):
        self.assertIn("combat", models.NODE_SCHEMAS)
        fields = {key: kind for key, _label, kind, _optional in models.NODE_SCHEMAS["combat"]["fields"]}
        self.assertEqual(fields["key"], "combat_id")
        self.assertEqual(fields["win"], "node_ref")
        node = models.new_node("combat", "fight", {"combat_ids": ["5102_01"]})
        node.update({"key": "5102_01", "win": "win", "lose": "lose"})
        self.assertIn("5102_01", models.node_summary(node))
        self.assertIn("胜利→win", models.node_summary(node))
        self.assertEqual(models.node_bullet("combat"), "■")

    def test_graph_has_two_labeled_edges_and_no_fallthrough(self):
        story = {
            "id": "main", "start": "fight",
            "nodes": [
                {"id": "fight", "type": "combat", "key": "5102_01", "win": "win", "lose": "lose"},
                {"id": "unused", "type": "say", "text": "不应顺序到达"},
                {"id": "win", "type": "end"},
                {"id": "lose", "type": "end"},
            ],
        }
        graph = analyze_story(story)
        edges = {(edge.source, edge.target, edge.label) for edge in graph.edges}
        self.assertIn(("fight", "win", "胜利"), edges)
        self.assertIn(("fight", "lose", "失败"), edges)
        self.assertNotIn(("fight", "unused", "下一步"), edges)
        self.assertEqual(graph.unreachable, {"unused"})
        self.assertFalse(simulate_stage(story, "unused")["reached"])

    def test_rename_retargets_win_and_lose(self):
        story = {
            "start": "fight",
            "nodes": [
                {"id": "fight", "type": "combat", "win": "win", "lose": "lose"},
                {"id": "win", "type": "end"},
                {"id": "lose", "type": "end"},
            ],
        }
        models.rename_node(story, "win", "victory")
        self.assertEqual(story["nodes"][0]["win"], "victory")

    def test_battle_schema_and_graph_use_verified_result_names(self):
        fields = {
            key: kind for key, _label, kind, _optional
            in models.NODE_SCHEMAS["battle"]["fields"]
        }
        self.assertEqual(fields["key"], "battle_id")
        story = {
            "start": "war",
            "nodes": [
                {"id": "war", "type": "battle", "key": "1001", "win": "friend", "lose": "enemy"},
                {"id": "friend", "type": "end"},
                {"id": "enemy", "type": "end"},
            ],
        }
        labels = {edge.label for edge in analyze_story(story).edges if edge.source == "war"}
        self.assertEqual(labels, {"友军胜利", "敌军胜利"})
        self.assertIn("友军胜→friend", models.node_summary(story["nodes"][0]))


if __name__ == "__main__":
    unittest.main(verbosity=2)
