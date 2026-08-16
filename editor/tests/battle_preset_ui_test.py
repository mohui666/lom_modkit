# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication, QComboBox, QPushButton  # noqa: E402

import models  # noqa: E402
from main import BattlePresetDialog, GameplayPresetConfigDialog  # noqa: E402
from node_form import NodeForm  # noqa: E402


class BattlePresetUiTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_dialog_round_trips_combat_and_battle_presets(self):
        editor_data = dict(models.FALLBACK_EDITOR_DATA)
        editor_data["combat_ids"] = [{"id": "5102_01", "name": "山贼伏击"}]
        editor_data["battle_ids"] = [{"id": "1001", "name": "最终战"}]
        editor_data["enemy_teams"] = [{"id": "201", "name": "山贼"}]
        source = {
            "bandit": {
                "name": "山贼伏击", "kind": "combat", "key": "5102_01",
                "max_health": 800, "strength": 20,
            },
            "final": {
                "kind": "battle", "key": "1001", "enemy_roster": "1001",
                "friend_people": 8, "enemy_people": 12,
            },
        }
        dialog = BattlePresetDialog(source, editor_data)
        dialog._accept_presets()
        self.assertEqual(dialog.presets, source)

        dialog = BattlePresetDialog(source, editor_data)
        kind_combo = dialog.table.cellWidget(0, 2)
        config_button = dialog.table.cellWidget(0, 4)
        self.assertIsInstance(config_button, QPushButton)
        self.assertIn("决斗", config_button.text())
        kind_combo.setCurrentIndex(kind_combo.findData("battle"))
        self.assertIn("战役", config_button.text())

    def test_config_dialog_reuses_node_controls_without_flow_fields(self):
        data = dict(models.FALLBACK_EDITOR_DATA)
        data["combat_ids"] = [{"id": "5102_01", "name": "山贼伏击"}]
        dialog = GameplayPresetConfigDialog(
            "combat", {"key": "5102_01", "max_health": 800}, data
        )
        self.assertEqual(dialog.node["max_health"], 800)
        self.assertFalse(any(combo.findData("win") >= 0 for combo in dialog.findChildren(QComboBox)))
        result = dialog.result_config()
        self.assertEqual(result["key"], "5102_01")
        self.assertNotIn("win", result)

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

    def test_enemy_and_skill_catalogs_show_names_but_store_ids(self):
        data = dict(models.FALLBACK_EDITOR_DATA)
        data["enemy_teams"] = [{"id": "201", "name": "山贼"}]
        data["battle_skills"] = [{"id": "special3", "name": "点破云关"}]
        form = NodeForm()
        form.set_context(data, ["setup"], ["main"], {})

        enemy = {"id": "setup", "type": "enemy", "op": "id", "enemy": "201"}
        form.set_node(enemy)
        enemy_combo = next(c for c in form.findChildren(QComboBox) if c.findData("201") >= 0)
        self.assertIn("山贼", enemy_combo.itemText(enemy_combo.findData("201")))
        self.assertEqual(enemy_combo.currentData(), "201")

        skill = {
            "id": "setup", "type": "battle_skill", "op": "set",
            "key": "special3", "index": 2,
        }
        form.set_node(skill)
        skill_combo = next(
            c for c in form.findChildren(QComboBox) if c.findData("special3") >= 0
        )
        self.assertIn("点破云关", skill_combo.itemText(skill_combo.findData("special3")))
        self.assertEqual(skill_combo.currentData(), "special3")


if __name__ == "__main__":
    unittest.main(verbosity=2)
