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
            "background": "battlefield",
            "max_health": 800, "health": 650, "strength": 20,
            "talents": [{"key": "0001", "level": 3}],
            "attack_rate": 0.65, "win": "win", "lose": "lose",
        }))
        self.assertIn(
            'mod_gameplay_configure("combat", "character=brother4;background=battlefield;max_health=800;'
            'health=650;strength=20;attack_rate=0.65;talents=0001:3")', lua
        )
        self.assertIn('mod_gameplay_start_scene("combat")', lua)
        self.assertNotIn('ChangeScene("Combat", "0001_01", "Story")', lua)
        self.assertNotIn('ChangeScene("Combat", "brother4"', lua)

    def test_custom_character_is_accepted_without_filling_stats(self):
        lua = compile_story(story({
            "id": "fight", "type": "combat", "character": "user:author.hero",
            "win": "win", "lose": "lose",
        }))
        self.assertIn("character=user:author.hero;background=center", lua)

    def test_every_live_combat_field_is_serialized_without_dead_ultimate_slots(self):
        lua = compile_story(story({
            "id": "fight", "type": "combat", "character": "brother4",
            "background": "center", "max_health": 900, "health": 800,
            "max_stamina": 130, "stamina": 120, "stamina_power": 17,
            "strength": 21, "internal": 22, "dexterity": 23, "talking": 24,
            "defence": 25, "sword": 26, "fist": 27, "martial_weapon": 28,
            "mental": 29, "confucianism": 1, "buddhism": 2, "taoism": 3,
            "xingyi": 4, "strategy_level": 5,
            "weapon_poison_value": 30, "weapon_paralyzed_value": 31,
            "poison_resist": 32, "paralyzed_resist": 33, "disposition": 34,
            "behaviour": 35, "karma": 36, "training": 37,
            "attack_damage_addition": -2, "defence_addition": 3,
            "ultimate_damage_rate": 1.25, "attack_dice_addition": 2,
            "weapon_damage_addition": -4, "weapon_dice_addition": 1,
            "weapon_hit_addition": 5, "attack_parry_addition": 0.2,
            "block_dodge_addition": 0.3, "block_parry_addition": 0.4,
            "talk_rate": 0.1, "attack_rate": 0.2,
            "weapon_rate": 0.3, "ultimate_rate": 0.4, "block_rate": 0.5,
            "talents": [{"key": "0001", "level": 2}],
            "win": "win", "lose": "lose",
        }))
        self.assertIn(
            'character=brother4;background=center;max_health=900;health=800;'
            'max_stamina=130;stamina=120;stamina_power=17;strength=21;internal=22;'
            'dexterity=23;talking=24;defence=25;sword=26;fist=27;'
            'martial_weapon=28;mental=29;confucianism=1;buddhism=2;taoism=3;xingyi=4;'
            'strategy_level=5;weapon_poison_value=30;weapon_paralyzed_value=31;'
            'poison_resist=32;paralyzed_resist=33;disposition=34;behaviour=35;karma=36;'
            'training=37;attack_damage_addition=-2;defence_addition=3;'
            'ultimate_damage_rate=1.25;attack_dice_addition=2;weapon_damage_addition=-4;'
            'weapon_dice_addition=1;weapon_hit_addition=5;attack_parry_addition=0.2;'
            'block_dodge_addition=0.3;block_parry_addition=0.4;talk_rate=0.1;attack_rate=0.2;'
            'weapon_rate=0.3;ultimate_rate=0.4;block_rate=0.5;talents=0001:2',
            lua,
        )

    def test_background_is_an_independent_verified_official_view(self):
        lua = compile_story(story({
            "id": "fight", "type": "combat", "character": "brother4",
            "background": "center_night", "win": "win", "lose": "lose",
        }))
        self.assertIn("character=brother4;background=center_night", lua)
        with self.assertRaisesRegex(LomcError, "background"):
            compile_story(story({
                "id": "fight", "type": "combat", "character": "brother4",
                "background": "center;character=other", "win": "win", "lose": "lose",
            }))
        with self.assertRaisesRegex(LomcError, "editor_data.views"):
            compile_story(story({
                "id": "fight", "type": "combat", "character": "brother4",
                "background": "no_such_official_view", "win": "win", "lose": "lose",
            }))

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
        with self.assertRaisesRegex(LomcError, "display_name"):
            compile_story(story(dict(base, display_name="伪装名称")))
        with self.assertRaisesRegex(LomcError, "未知字段.*ultimate_one"):
            compile_story(story(dict(base, ultimate_one="special3")))
        with self.assertRaisesRegex(LomcError, "不是原版 CombatSkill"):
            compile_story(story(dict(base, talents=[{"key": "fake", "level": 1}])))
        with self.assertRaisesRegex(LomcError, "1~3"):
            compile_story(story(dict(base, talents=[{"key": "0001", "level": 4}])))
        with self.assertRaisesRegex(LomcError, "max_health.*1~10000000"):
            compile_story(story(dict(base, max_health=0)))
        with self.assertRaisesRegex(LomcError, "strategy_level.*0~10"):
            compile_story(story(dict(base, strategy_level=11)))
        with self.assertRaisesRegex(LomcError, "weapon_hit_addition.*0~10000"):
            compile_story(story(dict(base, weapon_hit_addition=-1)))
        with self.assertRaisesRegex(LomcError, "block_dodge_addition.*-1~1"):
            compile_story(story(dict(base, block_dodge_addition=1.1)))
        with self.assertRaisesRegex(LomcError, "ultimate_damage_rate.*0~100"):
            compile_story(story(dict(base, ultimate_damage_rate=101)))


class BattleNodeTest(unittest.TestCase):
    def _battle(self, **changes):
        node = {
            "id": "fight", "type": "battle",
            "friend_faction": "500", "friend_people": 8,
            "friend_characters": ["special4"],
            "enemy_faction": "400", "enemy_people": 12,
            "enemy_characters": ["special102"],
            "win": "win", "lose": "lose",
        }
        node.update(changes)
        return node

    def test_factions_totals_and_named_characters_are_compiled(self):
        lua = compile_story(story(self._battle()))
        self.assertIn(
            'mod_gameplay_configure("battle", "friend_faction=500;friend_people=8;'
            'enemy_faction=400;enemy_people=12;friend_characters=special4;'
            'enemy_characters=special102")', lua
        )
        self.assertIn('mod_gameplay_start_scene("battle")', lua)
        self.assertNotIn('ChangeScene("Battle", "0000", "Story")', lua)
        self.assertNotIn("roster", lua)
        self.assertNotIn("ResetBattleSkill", lua)

    def test_totals_include_named_and_only_verified_official_are_allowed(self):
        with self.assertRaisesRegex(LomcError, "至少 1"):
            compile_story(story(self._battle(friend_people=0)))
        with self.assertRaisesRegex(LomcError, "已验证的官方 Battle 人物"):
            compile_story(story(self._battle(friend_characters=["player"])))
        with self.assertRaisesRegex(LomcError, "不得重复"):
            compile_story(story(self._battle(
                friend_characters=["special4", "special4"]
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
