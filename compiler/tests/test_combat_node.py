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
            "max_health": 800, "health": 650, "strength": 20,
            "talents": [{"key": "skill_1", "level": 3}],
            "attack_rate": 0.65, "win": "win", "lose": "lose",
        }))
        self.assertIn('mod_gameplay_configure("combat", "max_health=800;health=650;strength=20;attack_rate=0.65;talents=skill_1:3")', lua)
        self.assertNotIn("ModifyEnemy", lua)
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
            "strength": 1.5, "win": "win", "lose": "lose",
        }
        with self.assertRaises(LomcError):
            compile_story(story(combat))
        combat["strength"] = 1
        combat["goto"] = "end"
        with self.assertRaises(LomcError):
            compile_story(story(combat))


class BattleNodeTest(unittest.TestCase):
    def test_emits_verified_original_battle_resume(self):
        document = story({
            "id": "fight", "type": "battle", "key": "1001",
            "friend_roster": "1002", "enemy_roster": "1003",
            "friend_people": 8, "enemy_people": 12,
            "friend_health": 300, "enemy_health": 450,
            "reset_skills": True,
            "skills": [{"key": "special3", "index": 2, "active": 1}],
            "win": "win", "lose": "lose",
        })
        lua = compile_story(document)
        self.assertIn("luamanager.ResetBattleSkill()", lua)
        self.assertIn('luamanager.SetPlayerBattleSkill("special3", 2)', lua)
        self.assertIn('mod_gameplay_configure("battle", "friend_roster=1002;enemy_roster=1003;friend_people=8;enemy_people=12;friend_health=300;enemy_health=450")', lua)
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


class DirectBattleConfigurationTest(unittest.TestCase):
    def test_old_preset_layer_is_rejected(self):
        document = story({
            "id": "fight", "type": "combat", "key": "5102_01",
            "win": "win", "lose": "lose",
        })
        document["battle_presets"] = {
            "old": {"kind": "combat", "key": "5102_01"}
        }
        with self.assertRaisesRegex(LomcError, "battle_presets.*已废弃"):
            compile_story(document)

    def test_nodes_reject_preset_and_require_direct_key(self):
        for kind in ("combat", "battle"):
            node = {"id": "fight", "type": kind, "preset": "old", "win": "win", "lose": "lose"}
            with self.subTest(kind=kind), self.assertRaises(LomcError):
                compile_story(story(node))
            node.pop("preset")
            with self.subTest(kind=kind), self.assertRaises(LomcError):
                compile_story(story(node))


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
