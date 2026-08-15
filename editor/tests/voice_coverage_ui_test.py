# -*- coding: utf-8 -*-
import os
from pathlib import Path
import sys
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication  # noqa: E402

import main  # noqa: E402
from voice_coverage import calculate_voice_coverage  # noqa: E402


class VoiceCoverageUiTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_tables_and_jump_to_unvoiced_node(self):
        stories = {
            "main": {"id": "main", "title": "主线", "nodes": [
                {"id": "a", "type": "say", "character": "player", "text": "无声"},
                {"id": "b", "type": "say", "mode": "narrative", "text": "有声",
                 "voice": "user:demo.b"},
            ]}
        }
        located = []
        dialog = main.VoiceCoverageDialog(
            calculate_voice_coverage(stories),
            lambda story_id, node_id: located.append((story_id, node_id)),
        )
        self.assertEqual(dialog.coverage_table.rowCount(), 4)  # total/story/2 characters
        self.assertEqual(dialog.unvoiced_table.rowCount(), 1)
        dialog._locate_unvoiced()
        self.assertEqual(located, [("main", "a")])


if __name__ == "__main__":
    unittest.main()
