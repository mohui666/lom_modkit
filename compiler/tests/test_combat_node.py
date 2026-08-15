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


if __name__ == "__main__":
    unittest.main(verbosity=2)
