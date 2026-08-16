# -*- coding: utf-8 -*-
import sys
import unittest
from pathlib import Path

COMPILER = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(COMPILER))

from lomc import LomcError, compile_story


def story(first):
    return {
        "id": "main", "title": "战斗测试", "start": "fight",
        "nodes": [
            first,
            {"id": "win", "type": "message", "text": "胜利", "goto": "end"},
            {"id": "lose", "type": "message", "text": "失败", "goto": "end"},
            {"id": "end", "type": "end"},
        ],
    }


class CombatNodeTest(unittest.TestCase):
    def test_character_only_selects_animation_and_stats_remain_explicit(self):
        lua = compile_story(story({
            "id": "fight", "type": "combat", "character": "brother4",
            "max_health": 800, "health": 650, "strength": 20,
            "talents": [{"key": "skill_1", "level": 3}],
            "attack_rate": 0.65, "win": "win", "lose": "lose",
        }))
        self.assertIn(
            'mod_gameplay_configure("combat", "character=brother4;max_health=800;'
            'health=650;strength=20;attack_rate=0.65;talents=skill_1:3")', lua
        )
        self.assertIn('ChangeScene("Combat", "0001_01", "Story")', lua)
        self.assertNotIn('ChangeScene("Combat", "brother4"', lua)

    def test_custom_character_is_accepted_without_filling_stats(self):
        lua = compile_story(story({
            "id": "fight", "type": "combat", "character": "user:author.hero",
            "win": "win", "lose": "lose",
        }))
        self.assertIn("character=user:author.hero", lua)

    def test_rejects_missing_target_bad_stats_and_old_key(self):
        base = {
            "id": "fight", "type": "combat", "character": "brother4",
            "win": "win", "lose": "lose",
        }
        missing = dict(base)
        del missing["lose"]
        with self.assertRaises(LomcError):
            compile_story(story(missing))
        with self.assertRaisesRegex(LomcError, "nowhere"):
            compile_story(story(dict(base, win="nowhere")))
        with self.assertRaises(LomcError):
            compile_story(story(dict(base, strength=1.5)))
        with self.assertRaisesRegex(LomcError, "未知字段.*key"):
            compile_story(story(dict(base, key="5102_01")))


class BattleNodeTest(unittest.TestCase):
    def _battle(self, **changes):
        node = {
            "id": "fight", "type": "battle",
            "friend_faction": "500", "friend_people": 8,
            "friend_characters": ["brother4"],
            "enemy_faction": "400", "enemy_people": 12,
            "enemy_characters": ["special3"],
            "win": "win", "lose": "lose",
        }
        node.update(changes)
        return node

    def test_factions_totals_and_named_characters_are_compiled(self):
        lua = compile_story(story(self._battle()))
        self.assertIn(
            'mod_gameplay_configure("battle", "friend_faction=500;friend_people=8;'
            'enemy_faction=400;enemy_people=12;friend_characters=brother4;'
            'enemy_characters=special3")', lua
        )
        self.assertIn('ChangeScene("Battle", "0000", "Story")', lua)
        self.assertNotIn("roster", lua)
        self.assertNotIn("ResetBattleSkill", lua)

    def test_totals_include_named_and_only_verified_official_are_allowed(self):
        with self.assertRaisesRegex(LomcError, "至少 1"):
            compile_story(story(self._battle(friend_people=0)))
        with self.assertRaisesRegex(LomcError, "已验证的官方 Battle 人物"):
            compile_story(story(self._battle(friend_characters=["player"])))
        with self.assertRaisesRegex(LomcError, "不得重复"):
            compile_story(story(self._battle(
                friend_characters=["brother4", "brother4"]
            )))

    def test_old_preset_fields_are_rejected(self):
        with self.assertRaisesRegex(LomcError, "未知字段.*key"):
            compile_story(story(dict(self._battle(), key="0000")))
        with self.assertRaisesRegex(LomcError, "未知字段.*friend_roster"):
            compile_story(story(dict(self._battle(), friend_roster="0000")))


class BattleResultNodeTest(unittest.TestCase):
    def test_emits_host_verified_win_lose_branch(self):
        document = story({
            "id": "fight", "type": "combat", "character": "brother4",
            "win": "result", "lose": "result",
        })
        document["nodes"].insert(1, {
            "id": "result", "type": "battle_result", "kind": "combat",
            "win": "win", "lose": "lose",
        })
        lua = compile_story(document)
        self.assertIn('mod_gameplay_last_result("main", "combat")', lua)
        self.assertNotIn("draw", lua)
        self.assertNotIn("escape", lua)


if __name__ == "__main__":
    unittest.main(verbosity=2)
