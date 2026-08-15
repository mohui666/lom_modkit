# -*- coding: utf-8 -*-
import sys
import unittest
from pathlib import Path

COMPILER = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(COMPILER))

from lomc import LomcError, compile_story


def story(combat):
    return {
        "id": "main",
        "title": "战斗测试",
        "start": "fight",
        "nodes": [
            combat,
            {"id": "win", "type": "message", "text": "胜利", "goto": "end"},
            {"id": "lose", "type": "message", "text": "失败", "goto": "end"},
            {"id": "end", "type": "end"},
        ],
    }


class CombatNodeTest(unittest.TestCase):
    def test_emits_original_combat_and_resume_dispatch(self):
        lua = compile_story(story({
            "id": "fight", "type": "combat", "key": "5102_01",
            "enemy": "Bandit", "level": 20, "people": 3,
            "display": 1, "win": "win", "lose": "lose",
        }))
        self.assertIn('statmodifymanager.ModifyEnemyId("Bandit")', lua)
        self.assertIn('statmodifymanager.ModifyEnemyLevel("Bandit", 20, 1)', lua)
        self.assertIn('statmodifymanager.ModifyEnemyPeople("Bandit", 3, 1)', lua)
        self.assertIn('mod_gameplay_prepare("combat", "main", "fight", "win", "lose")', lua)
        self.assertIn('luamanager.ChangeScene("Combat", "5102_01", "Story")', lua)
        self.assertIn('local mod_resume_target = mod_gameplay_consume_resume("main")', lua)
        self.assertIn('mod_resume_target == "win" then return node_win()', lua)
        self.assertIn('mod_resume_target == "lose" then return node_lose()', lua)
        self.assertNotIn("return node_win()\nend\n\n-- [win]", lua)

    def test_rejects_missing_or_unknown_result_target(self):
        base = {"id": "fight", "type": "combat", "key": "5102_01", "win": "win", "lose": "lose"}
        missing = dict(base)
        del missing["lose"]
        with self.assertRaises(LomcError):
            compile_story(story(missing))
        unknown = dict(base, win="nowhere")
        with self.assertRaises(LomcError) as caught:
            compile_story(story(unknown))
        self.assertIn("nowhere", str(caught.exception))

    def test_rejects_fractional_enemy_setup_and_explicit_goto(self):
        combat = {
            "id": "fight", "type": "combat", "key": "5102_01",
            "level": 1.5, "win": "win", "lose": "lose",
        }
        with self.assertRaises(LomcError):
            compile_story(story(combat))
        combat["level"] = 1
        combat["goto"] = "end"
        with self.assertRaises(LomcError):
            compile_story(story(combat))


class BattleNodeTest(unittest.TestCase):
    def test_emits_verified_original_battle_resume(self):
        document = story({
            "id": "fight", "type": "battle", "key": "1001",
            "win": "win", "lose": "lose",
        })
        lua = compile_story(document)
        self.assertIn('mod_gameplay_prepare("battle", "main", "fight", "win", "lose")', lua)
        self.assertIn('luamanager.ChangeScene("Battle", "1001", "Story")', lua)
        self.assertIn('mod_gameplay_consume_resume("main")', lua)

    def test_rejects_missing_target_and_extra_goto(self):
        document = story({
            "id": "fight", "type": "battle", "key": "1001",
            "win": "win", "lose": "missing",
        })
        with self.assertRaises(LomcError):
            compile_story(document)
        document["nodes"][0]["lose"] = "lose"
        document["nodes"][0]["goto"] = "end"
        with self.assertRaises(LomcError):
            compile_story(document)


class BattlePresetTest(unittest.TestCase):
    def test_combat_preset_expands_to_verified_original_calls(self):
        document = story({
            "id": "fight", "type": "combat", "preset": "bandit_ambush",
            "win": "win", "lose": "lose",
        })
        document["battle_presets"] = {
            "bandit_ambush": {
                "kind": "combat", "key": "5102_01", "enemy": "Bandit",
                "level": 20, "people": 3, "display": 1,
            }
        }
        lua = compile_story(document)
        self.assertIn('ModifyEnemyId("Bandit")', lua)
        self.assertIn('ModifyEnemyLevel("Bandit", 20, 1)', lua)
        self.assertIn('ChangeScene("Combat", "5102_01", "Story")', lua)

    def test_battle_preset_expands_to_verified_original_battle(self):
        document = story({
            "id": "fight", "type": "battle", "preset": "final_war",
            "win": "win", "lose": "lose",
        })
        document["battle_presets"] = {
            "final_war": {"kind": "battle", "key": "1001"}
        }
        lua = compile_story(document)
        self.assertIn('ChangeScene("Battle", "1001", "Story")', lua)

    def test_rejects_missing_wrong_kind_and_mixed_configuration(self):
        document = story({
            "id": "fight", "type": "combat", "preset": "missing",
            "win": "win", "lose": "lose",
        })
        with self.assertRaisesRegex(LomcError, "不存在"):
            compile_story(document)

        document["battle_presets"] = {
            "missing": {"kind": "battle", "key": "1001"}
        }
        with self.assertRaisesRegex(LomcError, "不是 combat"):
            compile_story(document)

        document["battle_presets"]["missing"] = {
            "kind": "combat", "key": "5102_01"
        }
        document["nodes"][0]["key"] = "5102_02"
        with self.assertRaisesRegex(LomcError, "只能填写"):
            compile_story(document)

    def test_rejects_malformed_preset_contract(self):
        document = story({
            "id": "fight", "type": "combat", "preset": "bad",
            "win": "win", "lose": "lose",
        })
        bad_values = [
            [],
            {"bad id": {"kind": "combat", "key": "5102_01"}},
            {"bad": {"kind": "combat", "key": ""}},
            {"bad": {"kind": "combat", "key": "5102_01", "level": 1.5}},
            {"bad": {"kind": "battle", "key": "1001", "enemy": "x"}},
        ]
        for value in bad_values:
            document["battle_presets"] = value
            with self.subTest(value=value), self.assertRaises(LomcError):
                compile_story(document)


class BattleResultNodeTest(unittest.TestCase):
    def test_emits_host_verified_win_lose_branch(self):
        document = story({
            "id": "fight", "type": "combat", "key": "5102_01",
            "win": "result", "lose": "result",
        })
        document["nodes"].insert(1, {
            "id": "result", "type": "battle_result", "kind": "combat",
            "win": "win", "lose": "lose",
        })
        lua = compile_story(document)
        self.assertIn('mod_gameplay_last_result("main", "combat")', lua)
        self.assertIn('gameplay_result == "win" then return node_win()', lua)
        self.assertIn('gameplay_result == "lose" then return node_lose()', lua)
        self.assertNotIn("draw", lua)
        self.assertNotIn("escape", lua)

    def test_rejects_unverified_result_kinds_and_missing_targets(self):
        document = story({
            "id": "fight", "type": "battle_result", "kind": "draw",
            "win": "win", "lose": "lose",
        })
        with self.assertRaises(LomcError):
            compile_story(document)
        document["nodes"][0]["kind"] = "any"
        document["nodes"][0]["win"] = "missing"
        with self.assertRaisesRegex(LomcError, "missing"):
            compile_story(document)


if __name__ == "__main__":
    unittest.main(verbosity=2)
