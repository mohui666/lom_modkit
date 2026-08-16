# -*- coding: utf-8 -*-
"""舞台推演对人物纯演出状态的回归测试。"""

from __future__ import annotations

import base64
import os
import sys
import tempfile
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

import preview  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402


class PreviewStageActionTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_pixmap_loader_accepts_path_returned_by_content_registry(self):
        # 1x1 PNG；复现自定义背景解析器把 pathlib.Path 交给预览加载器的路径。
        png = base64.b64decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        )
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "background.png"
            source.write_bytes(png)
            stage = preview.StagePreview()
            pixmap = stage._load_pixmap(source)
            self.assertFalse(pixmap.isNull())
            self.assertIn(str(source).replace("\\", "/"), stage._pix_cache)

    def test_custom_background_state_cleanup_and_playtest_prelude(self):
        story = {
            "id": "main",
            "title": "background",
            "start": "n1",
            "nodes": [
                {"id": "n1", "type": "scene", "view": "center"},
                {
                    "id": "n2",
                    "type": "background",
                    "action": "show",
                    "image": "user:mohui.moon_bg",
                    "fade": 0.5,
                },
                {"id": "n3", "type": "say", "mode": "narrative", "text": "月下"},
                {"id": "n4", "type": "background", "action": "clear"},
                {"id": "n5", "type": "end"},
            ],
        }
        state = preview.simulate_stage(story, "n3")
        self.assertEqual(state["background"], "user:mohui.moon_bg")
        prelude = preview.build_playtest_prelude(story, "n3")
        self.assertEqual([n["type"] for n in prelude], ["scene", "background"])
        self.assertEqual(prelude[1]["action"], "set")
        cleared = preview.simulate_stage(story, "n5")
        self.assertIsNone(cleared["background"])

    def test_custom_cg_state_and_playtest_prelude(self):
        story = {
            "id": "main",
            "title": "cg",
            "start": "n1",
            "nodes": [
                {
                    "id": "n1",
                    "type": "custom_cg",
                    "action": "show",
                    "image": "user:mohui.memory_cg",
                    "fade": 0.5,
                    "scale": 120,
                    "x": -10,
                    "y": 15,
                },
                {"id": "n2", "type": "say", "mode": "narrative", "text": "回忆"},
                {"id": "n3", "type": "custom_cg", "action": "hide", "fade": 0.2},
                {"id": "n4", "type": "end"},
            ],
        }
        state = preview.simulate_stage(story, "n2")
        self.assertEqual(state["custom_cg"]["image"], "user:mohui.memory_cg")
        self.assertEqual(state["custom_cg"]["scale"], 120)
        prelude = preview.build_playtest_prelude(story, "n2")
        self.assertEqual(len(prelude), 1)
        self.assertEqual(prelude[0]["type"], "custom_cg")
        self.assertEqual(prelude[0]["fade"], 0)
        cleared = preview.simulate_stage(story, "n4")
        self.assertIsNone(cleared["custom_cg"])

    def test_overlay_slots_layers_hide_and_playtest_prelude(self):
        story = {
            "id": "main", "title": "overlay", "start": "n1",
            "nodes": [
                {"id": "n1", "type": "overlay", "action": "show", "slot": "prop",
                 "image": "user:mohui.lantern", "position": "left", "scale": 80,
                 "opacity": 70, "layer": "back", "fade": 0.2},
                {"id": "n2", "type": "overlay", "action": "show", "slot": "mask",
                 "image": "user:mohui.vignette", "position": "center", "scale": 100,
                 "opacity": 40, "layer": "front", "fade": 0},
                {"id": "n3", "type": "say", "mode": "narrative", "text": "test"},
                {"id": "n4", "type": "overlay", "action": "hide", "slot": "prop", "fade": 0.1},
                {"id": "n5", "type": "end"},
            ],
        }
        state = preview.simulate_stage(story, "n3")
        self.assertEqual(set(state["overlays"]), {"prop", "mask"})
        self.assertEqual(state["overlays"]["prop"]["layer"], "back")
        prelude = preview.build_playtest_prelude(story, "n3")
        self.assertEqual([n["slot"] for n in prelude], ["mask", "prop"])
        self.assertTrue(all(n["fade"] == 0 for n in prelude))
        hidden = preview.simulate_stage(story, "n5")
        self.assertNotIn("prop", hidden["overlays"])
        self.assertIn("mask", hidden["overlays"])

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
