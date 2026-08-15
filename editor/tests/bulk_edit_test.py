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

from bulk_edit import BulkEditDialog, apply_bulk_edit, compatible_fields  # noqa: E402


class BulkEditTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_only_exact_common_schema_fields_are_enabled(self):
        show = {"id": "s1", "type": "show", "character": "player", "position": "M"}
        say = {"id": "q1", "type": "say", "character": "player", "portrait": "normal", "text": "x"}
        common = {field.key: field.kind for field in compatible_fields([show, say])}
        self.assertEqual(common, {"character": "character", "portrait": "portrait"})
        move = {"id": "m1", "type": "move", "character": "player", "from": "L1", "to": "M", "duration": 1}
        offset = {"id": "o1", "type": "offset", "character": "player", "x": 0, "y": 0, "duration": 1}
        common = {field.key for field in compatible_fields([move, offset])}
        self.assertEqual(common, {"character", "duration"})

    def test_apply_changes_all_or_rejects_incompatible_field_and_type(self):
        story = {"nodes": [
            {"id": "s1", "type": "show", "character": "player", "position": "M"},
            {"id": "q1", "type": "say", "character": "player", "portrait": "normal", "text": "x"},
        ]}
        self.assertEqual(apply_bulk_edit(story, [0, 1], "character", "brother4"), 2)
        self.assertEqual([n["character"] for n in story["nodes"]], ["brother4", "brother4"])
        with self.assertRaises(ValueError):
            apply_bulk_edit(story, [0, 1], "position", "L1")
        with self.assertRaises(ValueError):
            apply_bulk_edit(story, [0, 1], "character", 123)
        with self.assertRaises(ValueError):
            apply_bulk_edit(story, [0], "character", "player")

    def test_dialog_requires_two_nodes_and_exposes_common_field(self):
        story = {"nodes": [
            {"id": "s1", "type": "show", "character": "player", "position": "M"},
            {"id": "q1", "type": "say", "character": "player", "portrait": "normal", "text": "x"},
        ]}
        dialog = BulkEditDialog(story)
        self.assertFalse(dialog._ok.isEnabled())
        dialog.nodes.item(0).setSelected(True)
        dialog.nodes.item(1).setSelected(True)
        self.assertTrue(dialog._ok.isEnabled())
        fields = {dialog.field_combo.itemData(i).key for i in range(dialog.field_combo.count())}
        self.assertEqual(fields, {"character", "portrait"})
        dialog.close()


if __name__ == "__main__":
    unittest.main()
