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
            "character", "background", "max_health", "health", "max_stamina", "stamina", "strength",
            "stamina_power", "internal", "dexterity", "talking", "defence", "sword", "fist",
            "martial_weapon", "mental",
            "talents",
            "player_max_health", "player_health", "player_max_stamina", "player_stamina",
            "player_stamina_power", "player_strength", "player_internal",
            "player_dexterity", "player_talking", "player_defence",
            "player_sword", "player_fist", "player_martial_weapon", "player_mental",
            "player_poison_resist", "player_paralyzed_resist",
            "player_disposition", "player_behaviour", "player_karma", "player_training",
            "player_talents",
            "talk_rate", "attack_rate", "weapon_rate",
            "ultimate_rate", "block_rate", "weapon_poison_value", "weapon_paralyzed_value",
            "poison_resist", "paralyzed_resist", "disposition", "behaviour", "karma",
            "training", "attack_damage_addition", "defence_addition", "ultimate_damage_rate",
            "attack_dice_addition", "weapon_damage_addition", "weapon_dice_addition",
            "weapon_hit_addition", "attack_parry_addition", "block_dodge_addition",
            "block_parry_addition", "win", "lose",
        })
        kinds = {key: kind for key, _label, kind, _optional in models.NODE_SCHEMAS["combat"]["fields"]}
        self.assertNotIn("ultimate_one", kinds)
        self.assertEqual(kinds["stamina_power"], "int")

    def test_full_combat_skill_catalog_is_independent_from_story_talents(self):
        data, _fallback = models.load_editor_data(Path(__file__).resolve().parents[2])
        self.assertEqual(len(data["combat_talents"]), 115)
        self.assertGreater(len(data["combat_talents"]), len(data["talents"]))
        skill = next(item for item in data["combat_talents"] if item["id"] == "0001")
        self.assertEqual(skill["max_level"], 3)
        self.assertEqual([x["key"] for x in skill["effects"]], ["B006_1", "B006_2", "B006_3"])

    def test_battle_form_hides_factions_without_a_battle_level(self):
        data = dict(models.FALLBACK_EDITOR_DATA)
        form = NodeForm()
        form.set_context(data, ["war", "win", "lose"], ["main"])
        form.set_node({
            "id": "war", "type": "battle",
            "friend_factions": [{"id": "500", "people": 2}],
            "enemy_factions": [{"id": "001", "people": 2}],
            "win": "win", "lose": "lose",
        })
        kinds = {
            key: kind for key, _label, kind, _optional
            in models.NODE_SCHEMAS["battle"]["fields"]
        }
        self.assertEqual(kinds["friend_factions"], "battle_faction_list")
        self.assertNotIn("400", dict(models.battle_faction_items(data)))

    def test_battle_picker_only_promises_spawnable_named_prefabs(self):
        self.assertEqual(
            models.VERIFIED_BATTLE_CHARACTER_IDS,
            ("special4", "special102", "special103", "special401", "special811"),
        )

    def test_battle_node_exposes_only_factions_totals_and_named_characters(self):
        fields = {key for key, *_ in models.NODE_SCHEMAS["battle"]["fields"]}
        self.assertNotIn("preset", fields)
        self.assertEqual(fields, {
            "title", "friend_health", "friend_factions", "friend_characters",
            "enemy_health", "enemy_factions", "enemy_characters", "win", "lose",
        })
        self.assertNotIn("friend_people", fields)
        self.assertNotIn("enemy_people", fields)

    def test_battle_form_edits_per_faction_people_and_sums_total(self):
        data = dict(models.FALLBACK_EDITOR_DATA)
        form = NodeForm()
        form.set_context(data, ["war", "win", "lose"], ["main"])
        node = {
            "id": "war", "type": "battle",
            "friend_factions": [{"id": "500", "people": 2}, {"id": "001", "people": 3}],
            "friend_characters": ["special4"],
            "enemy_factions": [{"id": "002", "people": 4}],
            "win": "win", "lose": "lose",
        }
        form.set_node(node)
        people_boxes = [
            box for box in form.findChildren(QSpinBox)
            if box.minimum() == 1 and box.maximum() == 10000
        ]
        self.assertGreaterEqual(len(people_boxes), 3)
        self.assertEqual(models.battle_side_total(node, "friend"), 6)
        people_boxes[0].setValue(5)
        self.assertEqual(node["friend_factions"][0]["people"], 5)
        self.assertEqual(models.battle_side_total(node, "friend"), 9)

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
        self.assertEqual(template.lineEdit().cursorPosition(), 0)
        self.assertFalse(template.currentText().startswith(" "))
        self.assertNotIn(" · ", template.currentText())


if __name__ == "__main__":
    unittest.main(verbosity=2)
