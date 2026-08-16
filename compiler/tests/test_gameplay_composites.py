# -*- coding: utf-8 -*-
import sys
import unittest
from pathlib import Path

COMPILER = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(COMPILER))

from lomc import LomcError, compile_story


def story(node):
    return {
        "id": "main", "start": node["id"],
        "nodes": [node, {"id": "end", "type": "end"}],
    }


class BattleSetupTest(unittest.TestCase):
    def test_emits_only_verified_enemy_and_skill_calls(self):
        lua = compile_story(story({
            "id": "setup", "type": "battle_setup", "enemy": "Bandit",
            "team": 2, "level": 10, "people": 3, "display": 1,
            "reset_skills": True,
            "skills": [
                {"key": "Skill_A", "index": 2, "active": 1},
                {"key": "Skill_B", "index": 3, "active": 0},
            ],
        }))
        self.assertIn('ModifyEnemyId("Bandit")', lua)
        self.assertIn('ModifyEnemyTeam("Bandit", 2, 1)', lua)
        self.assertIn('ModifyEnemyLevel("Bandit", 10, 1)', lua)
        self.assertIn('ModifyEnemyPeople("Bandit", 3, 1)', lua)
        self.assertIn("luamanager.ResetBattleSkill()", lua)
        self.assertIn('SetPlayerBattleSkill("Skill_A", 2)', lua)
        self.assertIn('SetBattleSkillActive("Skill_B", 0)', lua)

    def test_rejects_empty_and_malformed_setup(self):
        bad_nodes = [
            {"id": "setup", "type": "battle_setup"},
            {"id": "setup", "type": "battle_setup", "level": 2},
            {"id": "setup", "type": "battle_setup", "skills": [{}]},
            {"id": "setup", "type": "battle_setup", "skills": [{"key": "A", "active": 2}]},
        ]
        for node in bad_nodes:
            with self.subTest(node=node), self.assertRaises(LomcError):
                compile_story(story(node))


class RewardTest(unittest.TestCase):
    def test_expands_to_existing_atomic_reward_apis(self):
        lua = compile_story(story({
            "id": "reward", "type": "reward", "entries": [
                {"kind": "stat", "key": "Money", "amount": 500},
                {"kind": "affinity", "key": "chicken1", "amount": 1},
                {"kind": "talent", "key": "Talent_A", "amount": 1},
                {"kind": "item", "category": "book", "key": "Book_A", "amount": 1},
                {"kind": "flag", "key": "bandit_rewarded"},
            ],
        }))
        self.assertIn('Player("Money", 500, "", 1)', lua)
        self.assertIn('Character("chicken1", 1, 1)', lua)
        self.assertIn('AddTalent("Talent_A", 1)', lua)
        self.assertIn('AddBook("Book_A", 1)', lua)
        self.assertIn('AddStory("bandit_rewarded")', lua)
        self.assertIn('modflags["bandit_rewarded"] = true', lua)

    def test_rejects_malformed_rewards(self):
        bad_entries = [
            [],
            [{"kind": "money", "key": "Money", "amount": 1}],
            [{"kind": "talent", "key": "T", "amount": 2}],
            [{"kind": "item", "category": "book", "key": "B", "amount": 0}],
            [{"kind": "item", "category": "weapon", "key": "B", "amount": 1}],
            [{"kind": "flag", "key": "F", "amount": 1}],
        ]
        for entries in bad_entries:
            with self.subTest(entries=entries), self.assertRaises(LomcError):
                compile_story(story({"id": "reward", "type": "reward", "entries": entries}))

    def test_result_screen_reuses_message_and_reward_apis(self):
        lua = compile_story(story({
            "id": "result", "type": "result_screen", "title": "胜利",
            "text": "获得以下奖励",
            "entries": [
                {"kind": "stat", "key": "Money", "amount": 500},
                {"kind": "flag", "key": "bandit_rewarded"},
            ],
        }))
        self.assertIn('mainui.DisplayMessageText("胜利\\n获得以下奖励")', lua)
        self.assertIn('Player("Money", 500, "", 1)', lua)
        self.assertIn('AddStory("bandit_rewarded")', lua)
        self.assertLess(lua.index("DisplayMessageText"), lua.index("statmodifymanager.Player"))

    def test_result_screen_rejects_blank_title_and_malformed_rewards(self):
        for node in (
            {"id": "result", "type": "result_screen", "title": "  ", "entries": [
                {"kind": "stat", "key": "Money", "amount": 1},
            ]},
            {"id": "result", "type": "result_screen", "title": "胜利", "entries": []},
        ):
            with self.subTest(node=node), self.assertRaises(LomcError):
                compile_story(story(node))


class CustomShopTest(unittest.TestCase):
    def test_emits_scoped_original_shop_inventory_and_conditions(self):
        lua = compile_story(story({
            "id": "shop", "type": "custom_shop", "discount": 1,
            "items": [
                {"category": "book", "item": "Book_A", "count": 2},
                {
                    "category": "misc", "item": "Misc_B", "count": 3,
                    "condition": {"source": "mod", "key": "SHOP_OPEN"},
                },
                {
                    "category": "special", "item": "Special_C", "count": 1,
                    "condition": {
                        "source": "condition", "key": "CP_SECRET", "invert": True,
                    },
                },
            ],
        }))
        self.assertIn("mod_custom_shop_begin()", lua)
        self.assertIn('mod_custom_shop_add("book", "Book_A", 2)', lua)
        self.assertIn('if modflags["SHOP_OPEN"] then', lua)
        self.assertIn('if not (checkpointmanager.Condition("CP_SECRET")) then', lua)
        self.assertIn("runwait(shoppanel.Open(1))", lua)
        self.assertIn("mod_custom_shop_end()", lua)
        self.assertLess(lua.index("mod_custom_shop_begin"), lua.index("shoppanel.Open"))
        self.assertLess(lua.index("shoppanel.Open"), lua.index("mod_custom_shop_end"))

    def test_rejects_unverified_price_and_malformed_inventory(self):
        bad_items = [
            [],
            [{"category": "consume", "item": "C", "count": 1}],
            [{"category": "book", "item": "", "count": 1}],
            [{"category": "book", "item": "B", "count": 0}],
            [{"category": "book", "item": "B", "count": 1, "price": 10}],
            [
                {"category": "book", "item": "B", "count": 1},
                {"category": "book", "item": "B", "count": 2},
            ],
            [{
                "category": "misc", "item": "M", "count": 1,
                "condition": {"source": "stat", "key": "Money"},
            }],
        ]
        for items in bad_items:
            with self.subTest(items=items), self.assertRaises(LomcError):
                compile_story(story({"id": "shop", "type": "custom_shop", "items": items}))
        for discount in (-1, 2, 0.5, True):
            with self.subTest(discount=discount), self.assertRaises(LomcError):
                compile_story(story({
                    "id": "shop", "type": "custom_shop", "discount": discount,
                    "items": [{"category": "book", "item": "B", "count": 1}],
                }))


if __name__ == "__main__":
    unittest.main(verbosity=2)
