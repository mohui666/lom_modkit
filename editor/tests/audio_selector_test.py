# -*- coding: utf-8 -*-
from __future__ import annotations

import json
import os
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
ROOT = EDITOR.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication, QComboBox  # noqa: E402

from node_form import NodeForm  # noqa: E402


class AudioSelectorTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    @staticmethod
    def _editor_data() -> dict:
        return {
            "characters": [],
            "modes": ["character", "think", "narrative", "center"],
            "sounds": [{"id": "巴掌_001", "name": "巴掌_001"}],
            "env_sounds": [{"id": "雨天_001", "name": "雨天_001"}],
        }

    def test_sound_picker_contains_official_and_user_audio(self):
        form = NodeForm()
        form.set_context(self._editor_data(), ["sfx"])
        user = SimpleNamespace(ref="user:test.hit", name="自制打击声")
        with patch("node_form.content_registry.list_contents", return_value=[user]):
            form.set_node(
                {"id": "sfx", "type": "sound", "kind": "sound", "name": "巴掌_001"}
            )

        combo = next(
            item for item in form.findChildren(QComboBox)
            if item.findData("巴掌_001") >= 0
        )
        self.assertGreaterEqual(combo.findData("user:test.hit"), 0)
        self.assertIn("官方", combo.itemText(combo.findData("巴掌_001")))
        self.assertIn("用户", combo.itemText(combo.findData("user:test.hit")))

    def test_switching_to_env_rebuilds_with_official_environment_sounds(self):
        form = NodeForm()
        form.set_context(self._editor_data(), ["sfx"])
        node = {"id": "sfx", "type": "sound", "kind": "sound", "name": ""}
        with patch("node_form.content_registry.list_contents", return_value=[]):
            form.set_node(node)
            kind_combo = next(
                item for item in form.findChildren(QComboBox)
                if item.findData("sound") >= 0 and item.findData("env") >= 0
            )
            kind_combo.setCurrentIndex(kind_combo.findData("env"))
            self.app.processEvents()

        self.assertEqual(node["kind"], "env")
        combos = form.findChildren(QComboBox)
        self.assertTrue(any(item.findData("雨天_001") >= 0 for item in combos))
        self.assertFalse(any(item.findData("巴掌_001") >= 0 for item in combos))

    def test_generated_data_contains_verified_official_sound_catalogs(self):
        data = json.loads((ROOT / "data" / "editor_data.json").read_text(encoding="utf-8"))
        sounds = {item["id"] for item in data["sounds"]}
        env_sounds = {item["id"] for item in data["env_sounds"]}
        self.assertGreaterEqual(len(sounds), 90)
        self.assertGreaterEqual(len(env_sounds), 8)
        self.assertIn("巴掌_001", sounds)
        self.assertIn("雨天_001", env_sounds)


if __name__ == "__main__":
    unittest.main(verbosity=2)
