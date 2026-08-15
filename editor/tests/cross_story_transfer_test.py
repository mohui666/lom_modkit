# -*- coding: utf-8 -*-

from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parents[1]
if str(EDITOR) not in sys.path:
    sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication
from cross_story_transfer import CrossStoryTransferDialog, copy_nodes_between_stories


def _stories():
    return {
        "source": {
            "id": "source", "start": "say1", "nodes": [
                {"id": "say1", "type": "say", "character": "player", "text": "one", "voice": "user:voice.hero", "goto": "choice1"},
                {"id": "choice1", "type": "choice", "options": [
                    {"text": "loop", "goto": "say1"},
                    {"text": "outside", "goto": "end1"},
                ]},
                {"id": "end1", "type": "end"},
            ],
        },
        "target": {
            "id": "target", "start": "say1", "nodes": [
                {"id": "say1", "type": "say", "character": "player", "text": "existing"},
                {"id": "end1", "type": "end"},
            ],
        },
    }


class CrossStoryTransferTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_collision_resolution_internal_remap_and_content_ref_preserved(self):
        stories = _stories()
        result = copy_nodes_between_stories(stories, "source", 0, 1, "target", 1)
        self.assertEqual(result.first_index, 1)
        self.assertEqual(result.count, 2)
        self.assertEqual(result.id_mapping, {"say1": "say2", "choice1": "choice1"})
        copied = stories["target"]["nodes"][1:3]
        self.assertEqual(copied[0]["id"], "say2")
        self.assertEqual(copied[0]["goto"], "choice1")
        self.assertEqual(copied[0]["voice"], "user:voice.hero")
        self.assertEqual(copied[1]["options"][0]["goto"], "say2")
        self.assertEqual(copied[1]["options"][1]["goto"], "end1")
        self.assertTrue(any("复制范围外" in warning for warning in result.warnings))
        self.assertEqual(stories["source"]["nodes"][0]["id"], "say1")

    def test_external_fallthrough_and_missing_story_refs_warn_without_rewrite(self):
        stories = _stories()
        stories["source"]["nodes"][0].pop("goto")
        stories["source"]["nodes"][0]["next_script"] = "missing_chapter"
        result = copy_nodes_between_stories(stories, "source", 0, 0, "target", 0)
        copied = stories["target"]["nodes"][0]
        self.assertEqual(copied["next_script"], "missing_chapter")
        self.assertTrue(any("不存在的章节" in warning for warning in result.warnings))
        self.assertTrue(any("顺序进入复制范围外" in warning for warning in result.warnings))

    def test_same_story_unknown_story_and_bad_ranges_are_rejected_transactionally(self):
        stories = _stories()
        before = [dict(node) for node in stories["target"]["nodes"]]
        for args in (
            ("source", 0, 0, "source", 0),
            ("missing", 0, 0, "target", 0),
            ("source", 0, 99, "target", 0),
        ):
            with self.assertRaises(ValueError):
                copy_nodes_between_stories(stories, *args)
        self.assertEqual(stories["target"]["nodes"], before)

    def test_all_copied_ids_are_fresh_even_without_existing_collision(self):
        stories = _stories()
        result = copy_nodes_between_stories(stories, "source", 2, 2, "target", 2)
        self.assertNotEqual(result.id_mapping["end1"], "end1")
        self.assertEqual(len({node["id"] for node in stories["target"]["nodes"]}), 3)

    def test_dialog_requires_different_stories(self):
        stories = _stories()
        dialog = CrossStoryTransferDialog(stories, "source")
        self.assertTrue(dialog._ok.isEnabled())
        dialog.target_combo.setCurrentIndex(dialog.target_combo.findData("source"))
        self.assertFalse(dialog._ok.isEnabled())
        dialog.close()


if __name__ == "__main__":
    unittest.main()
