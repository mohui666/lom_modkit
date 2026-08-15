# -*- coding: utf-8 -*-
from pathlib import Path
import sys
import unittest

EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from voice_coverage import NARRATOR_ID, calculate_voice_coverage


STORIES = {
    "main": {
        "id": "main", "title": "主线", "nodes": [
            {"id": "a", "type": "say", "character": "player", "text": "有声",
             "voice": "user:demo.a"},
            {"id": "b", "type": "say", "character": "player", "text": "无声"},
            {"id": "c", "type": "say", "mode": "narrative", "text": "旁白"},
        ]
    },
    "side": {
        "id": "side", "title": "支线", "nodes": [
            {"id": "d", "type": "say", "mode": "center", "text": "居中旁白",
             "voice": "user:demo.d"},
            {"id": "e", "type": "say", "character": "user:demo.hero",
             "text": "自定义人物无声"},
            {"id": "end", "type": "end"},
        ]
    },
}


class VoiceCoverageTest(unittest.TestCase):
    def test_total_story_character_and_missing_nodes(self):
        report = calculate_voice_coverage(STORIES)
        self.assertEqual((report.total.voiced, report.total.unvoiced), (2, 3))
        self.assertEqual(report.total.total, 5)
        self.assertEqual(report.total.percent, 40.0)
        self.assertEqual(
            [(row.key, row.voiced, row.unvoiced) for row in report.stories],
            [("main", 1, 2), ("side", 1, 1)],
        )
        characters = {row.key: row for row in report.characters}
        self.assertEqual((characters["player"].voiced, characters["player"].unvoiced), (1, 1))
        self.assertEqual((characters[NARRATOR_ID].voiced, characters[NARRATOR_ID].unvoiced), (1, 1))
        self.assertEqual(characters["user:demo.hero"].unvoiced, 1)
        self.assertEqual(
            [(item.story_id, item.node_id) for item in report.unvoiced_dialogues],
            [("main", "b"), ("main", "c"), ("side", "e")],
        )

    def test_empty_project_is_zero_not_division_error(self):
        report = calculate_voice_coverage({"main": {"nodes": []}})
        self.assertEqual(report.total.total, 0)
        self.assertEqual(report.total.percent, 0.0)
        self.assertEqual(report.unvoiced_dialogues, ())


if __name__ == "__main__":
    unittest.main()
