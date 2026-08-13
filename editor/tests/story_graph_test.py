# -*- coding: utf-8 -*-
from __future__ import annotations

import sys
import unittest
from pathlib import Path

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from story_graph import analyze_story  # noqa: E402


class StoryGraphTest(unittest.TestCase):
    def test_branches_fallthrough_and_unreachable(self):
        story = {
            "start": "n1",
            "nodes": [
                {"id": "n1", "type": "choice", "options": [{"text": "去", "goto": "n3"}]},
                {"id": "n2", "type": "say"},
                {"id": "n3", "type": "say"},
                {"id": "n4", "type": "end"},
            ],
        }
        graph = analyze_story(story)
        self.assertEqual(graph.reachable, {"n1", "n3", "n4"})
        self.assertEqual(graph.unreachable, {"n2"})
        self.assertFalse(graph.dead_ends)

    def test_missing_target_and_dead_end(self):
        story = {
            "start": "n1",
            "nodes": [
                {"id": "n1", "type": "choice", "options": [{"goto": "lost"}]},
                {"id": "n2", "type": "say"},
            ],
        }
        graph = analyze_story(story)
        self.assertEqual(graph.missing_targets, {"n1"})
        self.assertEqual(graph.dead_ends, {"n1"})
        self.assertEqual(graph.unreachable, {"n2"})

    def test_only_marks_cycle_that_cannot_reach_ending(self):
        trapped = {
            "start": "n1",
            "nodes": [
                {"id": "n1", "type": "say", "goto": "n2"},
                {"id": "n2", "type": "say", "goto": "n1"},
                {"id": "end", "type": "end"},
            ],
        }
        self.assertEqual(analyze_story(trapped).infinite_loops, {"n1", "n2"})

        escapable = {
            "start": "n1",
            "nodes": [
                {
                    "id": "n1",
                    "type": "branch",
                    "cases": [{"value": 1, "goto": "n1"}, {"value": 2, "goto": "end"}],
                },
                {"id": "end", "type": "end"},
            ],
        }
        self.assertFalse(analyze_story(escapable).infinite_loops)


if __name__ == "__main__":
    unittest.main(verbosity=2)
