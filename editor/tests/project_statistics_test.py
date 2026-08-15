# -*- coding: utf-8 -*-
from pathlib import Path
import sys
import unittest

EDITOR = Path(__file__).resolve().parent.parent
COMPILER = EDITOR.parent / "compiler"
sys.path[:0] = [str(EDITOR), str(COMPILER)]

from project_statistics import calculate_project_statistics


STORIES = {
    "main": {
        "id": "main", "start": "show1",
        "nodes": [
            {"id": "show1", "type": "show", "character": "player", "position": "M"},
            {"id": "say1", "type": "say", "character": "player", "text": "一",
             "voice": "user:demo.voice"},
            {"id": "choice1", "type": "choice", "options": [
                {"text": "左", "goto": "left"}, {"text": "结束", "goto": "end1"}
            ]},
            {"id": "left", "type": "say", "mode": "narrative", "text": "二",
             "goto": "end1"},
            {"id": "unused", "type": "say", "mode": "narrative", "text": "不可达"},
            {"id": "end1", "type": "end"},
        ],
    },
    "side": {
        "id": "side", "start": "bg1",
        "nodes": [
            {"id": "bg1", "type": "background", "action": "show",
             "image": "user:demo.background"},
            {"id": "music1", "type": "music", "name": "user:demo.music"},
            {"id": "end2", "type": "end"},
        ],
    },
}


class ProjectStatisticsTest(unittest.TestCase):
    def test_required_statistics_and_asset_inventory(self):
        bundled = [
            "assets/user/audio/demo.voice/content.json",
            "assets/user/audio/demo.voice/voice.wav",
            "assets/user/audio/demo.music/content.json",
            "assets/user/audio/demo.music/music.ogg",
            "assets/user/image/demo.background/content.json",
            "assets/user/image/demo.background/bg.png",
            "assets/unused.png",
        ]
        stats = calculate_project_statistics(STORIES, bundled)
        self.assertEqual(stats.stories, 2)
        self.assertEqual(stats.nodes, 9)
        self.assertEqual(stats.dialogue_count, 3)
        self.assertEqual((stats.choice_nodes, stats.choice_options), (1, 2))
        self.assertEqual(stats.endings, 2)
        self.assertEqual(stats.characters, 1)
        self.assertEqual(stats.images, 1)
        self.assertEqual(stats.audio, 2)
        self.assertEqual((stats.voiced_dialogue, stats.unvoiced_dialogue), (1, 2))
        self.assertAlmostEqual(stats.voice_coverage, 100 / 3)
        self.assertEqual(stats.unreachable_nodes, 1)
        self.assertEqual(stats.unused_assets, 1)
        self.assertEqual(len(stats.rows()), 11)

    def test_unsaved_project_reports_asset_inventory_unavailable(self):
        stats = calculate_project_statistics(STORIES)
        self.assertIsNone(stats.unused_assets)
        self.assertIn("不可用", stats.rows()[-1][1])


if __name__ == "__main__":
    unittest.main()
