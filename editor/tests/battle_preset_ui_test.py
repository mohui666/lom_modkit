# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication, QComboBox, QSpinBox  # noqa: E402

import models  # noqa: E402
from node_form import NodeForm  # noqa: E402


class DirectCombatBattleUiTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_combat_node_exposes_all_verified_parameters(self):
        fields = {key for key, *_ in models.NODE_SCHEMAS["combat"]["fields"]}
        self.assertNotIn("preset", fields)
        self.assertEqual(fields, {
            "key", "max_health", "health", "max_stamina", "stamina", "strength",
            "internal", "dexterity", "talking", "defence", "sword", "fist",
            "martial_weapon", "mental", "talents", "ultimate_one", "ultimate_two",
            "ultimate_three", "talk_rate", "attack_rate", "weapon_rate",
            "ultimate_rate", "block_rate", "win", "lose",
        })

    def test_battle_node_exposes_all_three_sides_and_skills(self):
        fields = {key for key, *_ in models.NODE_SCHEMAS["battle"]["fields"]}
        self.assertNotIn("preset", fields)
        self.assertEqual(fields, {
            "key", "friend_roster", "enemy_roster", "neutral_roster",
            "friend_people", "enemy_people", "neutral_people",
            "friend_health", "enemy_health", "neutral_health",
            "reset_skills", "skills", "win", "lose",
        })

    def test_form_shows_readable_template_and_direct_numeric_controls(self):
        data = dict(models.FALLBACK_EDITOR_DATA)
        data["combat_ids"] = [{"id": "5102_01", "name": "山贼决斗"}]
        form = NodeForm()
        form.set_context(data, ["fight", "win", "lose"], ["main"])
        form.set_node({
            "id": "fight", "type": "combat", "key": "5102_01",
            "max_health": 800, "win": "win", "lose": "lose",
        })
        template = next(c for c in form.findChildren(QComboBox) if c.findData("5102_01") >= 0)
        self.assertIn("山贼决斗", template.itemText(template.findData("5102_01")))
        self.assertTrue(form.findChildren(QSpinBox))


if __name__ == "__main__":
    unittest.main(verbosity=2)
