# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from PySide6.QtWidgets import QApplication  # noqa: E402

from global_search import index_project  # noqa: E402
from reference_inspector import (  # noqa: E402
    ReferenceDialog,
    ReferenceTarget,
    find_symbol_references,
    target_for_search_hit,
)
from global_search_test import STORIES  # noqa: E402


class ReferenceInspectorTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_content_character_variable_flag_and_node_references(self):
        cases = [
            (ReferenceTarget("content", "user:mohui.hello"), [("main", "say1", "voice")]),
            (ReferenceTarget("character", "user:mohui.hero"), [("main", "say1", "character")]),
            (ReferenceTarget("variable", "chapter_score"), [("side", "vars1", "vars[0].name")]),
            (ReferenceTarget("flag", "ROUTE_A"), [("main", "flag1", "flag")]),
            (ReferenceTarget("node", "flag1", "main"), [("main", "choice1", "options[0].goto")]),
        ]
        for target, expected in cases:
            refs = find_symbol_references(STORIES, target)
            self.assertEqual([(r.story_id, r.node_id, r.field) for r in refs], expected)

    def test_story_references_include_manifest_and_cross_story_jump(self):
        manifest = {
            "entry": "side",
            "campaign": {"triggers": [{"script": "side"}]},
        }
        refs = find_symbol_references(
            STORIES, ReferenceTarget("story", "side"), manifest
        )
        self.assertEqual(
            [r.field for r in refs],
            ["manifest.entry", "next_script", "manifest.campaign.triggers[0].script"],
        )

    def test_manifest_flag_and_character_conditions_are_references(self):
        manifest = {
            "campaign": {"triggers": [{
                "script": "side", "when_flag_set": "ROUTE_A",
                "when_affinity": {"character": "user:mohui.hero", "min": 2},
            }]}
        }
        flag_refs = find_symbol_references(
            STORIES, ReferenceTarget("flag", "ROUTE_A"), manifest
        )
        self.assertEqual(
            [r.field for r in flag_refs],
            ["flag", "manifest.campaign.triggers[0].when_flag_set"],
        )
        character_refs = find_symbol_references(
            STORIES, ReferenceTarget("character", "user:mohui.hero"), manifest
        )
        self.assertEqual(
            [r.field for r in character_refs],
            ["character", "manifest.campaign.triggers[0].when_affinity.character"],
        )

    def test_search_hit_target_inference_and_dialog_location(self):
        hit = next(
            h for h in index_project(STORIES)
            if h.category == "node" and h.story_id == "main" and h.value == "flag1"
        )
        self.assertEqual(target_for_search_hit(hit), ReferenceTarget("node", "flag1", "main"))
        located = []
        dialog = ReferenceDialog(
            STORIES, ReferenceTarget("node", "flag1", "main"),
            lambda sid, nid: located.append((sid, nid)),
        )
        self.assertEqual(dialog.table.rowCount(), 1)
        dialog._jump_selected()
        self.assertEqual(located, [("main", "choice1")])


if __name__ == "__main__":
    unittest.main()
