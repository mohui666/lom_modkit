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
            "character", "max_health", "health", "max_stamina", "stamina", "strength",
            "internal", "dexterity", "talking", "defence", "sword", "fist",
            "martial_weapon", "mental", "talents", "ultimate_one", "ultimate_two",
            "ultimate_three", "talk_rate", "attack_rate", "weapon_rate",
            "ultimate_rate", "block_rate", "win", "lose",
        })

    def test_battle_node_exposes_only_factions_totals_and_named_characters(self):
        fields = {key for key, *_ in models.NODE_SCHEMAS["battle"]["fields"]}
        self.assertNotIn("preset", fields)
        self.assertEqual(fields, {
            "friend_faction", "friend_people", "friend_characters",
            "enemy_faction", "enemy_people", "enemy_characters", "win", "lose",
        })

    def test_form_shows_readable_template_and_direct_numeric_controls(self):
        data = dict(models.FALLBACK_EDITOR_DATA)
        data["characters"] = [{"id": "special3", "name": "叶云舟"}]
        form = NodeForm()
        form.set_context(data, ["fight", "win", "lose"], ["main"])
        form.set_node({
            "id": "fight", "type": "combat", "character": "special3",
            "max_health": 800, "win": "win", "lose": "lose",
        })
        template = next(c for c in form.findChildren(QComboBox) if c.findData("special3") >= 0)
        self.assertIn("叶云舟", template.itemText(template.findData("special3")))
        self.assertTrue(form.findChildren(QSpinBox))


if __name__ == "__main__":
    unittest.main(verbosity=2)
