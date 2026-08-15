# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication, QComboBox  # noqa: E402

import models  # noqa: E402
from main import BattlePresetDialog  # noqa: E402
from node_form import NodeForm  # noqa: E402


class BattlePresetUiTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_dialog_round_trips_combat_and_battle_presets(self):
        editor_data = dict(models.FALLBACK_EDITOR_DATA)
        editor_data["combat_ids"] = [{"id": "5102_01", "name": "山贼伏击"}]
        editor_data["battle_ids"] = [{"id": "1001", "name": "最终战"}]
        source = {
            "bandit": {
                "kind": "combat", "key": "5102_01", "enemy": "Bandit",
                "level": 20, "people": 3,
            },
            "final": {"kind": "battle", "key": "1001"},
        }
        dialog = BattlePresetDialog(source, editor_data)
        dialog._accept_presets()
        self.assertEqual(dialog.presets, source)

    def test_node_form_filters_presets_by_gameplay_kind(self):
        form = NodeForm()
        form.set_context(
            models.FALLBACK_EDITOR_DATA, ["fight", "win", "lose"], ["main"],
            {
                "ambush": {"kind": "combat", "key": "5102_01"},
                "war": {"kind": "battle", "key": "1001"},
            },
        )
        node = {
            "id": "fight", "type": "combat", "preset": "ambush",
            "win": "win", "lose": "lose",
        }
        form.set_node(node)
        combos = form.findChildren(QComboBox)
        preset_combo = next(combo for combo in combos if combo.findData("ambush") >= 0)
        self.assertGreaterEqual(preset_combo.findData("ambush"), 0)
        self.assertEqual(preset_combo.findData("war"), -1)
        self.assertFalse(any(combo.findData("5102_01") >= 0 for combo in combos))


if __name__ == "__main__":
    unittest.main(verbosity=2)
