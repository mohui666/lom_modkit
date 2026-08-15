# -*- coding: utf-8 -*-
"""舞台推演对人物纯演出状态的回归测试。"""

from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

import preview  # noqa: E402


class PreviewStageActionTest(unittest.TestCase):
    def test_custom_character_actions_update_visual_state(self):
        raw = "user:mohui.luoxue"
        story = {
            "id": "main",
            "title": "preview",
            "start": "n1",
            "nodes": [
                {"id": "n1", "type": "show", "character": raw, "position": "M"},
                {
                    "id": "n2",
                    "type": "offset",
                    "character": raw,
                    "x": 12,
                    "y": -8,
                    "duration": 0.2,
                },
                {"id": "n3", "type": "dim", "character": raw, "dimmed": True},
                {
                    "id": "n4",
                    "type": "rotate",
                    "character": raw,
                    "angle": 30,
                    "duration": 0.3,
                },
                {"id": "n5", "type": "shock", "character": raw, "duration": 0.5},
                {"id": "n6", "type": "end"},
            ],
        }

        state = preview.simulate_stage(story, "n5")
        actor = state["actors"][raw]
        self.assertEqual((actor["offset_x"], actor["offset_y"]), (12, -8))
        self.assertTrue(actor["dimmed"])
        self.assertEqual(actor["rotation"], 30)
        self.assertTrue(actor["shocked"])
        self.assertIsNone(state["hint"])

        after = preview.simulate_stage(story, "n6")["actors"][raw]
        self.assertFalse(after["shocked"], "震动是瞬时效果，不应污染后续节点")


if __name__ == "__main__":
    unittest.main()
