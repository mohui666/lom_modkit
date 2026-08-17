# -*- coding: utf-8 -*-
from __future__ import annotations

import unittest
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from lomc import LomcError, compile_story, validate_manifest, validate_story
from lomc.schema_versions import CONTENT_SCHEMA, PACKAGE_FORMAT, STORY_SCHEMA


def manifest(**changes):
    value = {
        "format": PACKAGE_FORMAT,
        "package_format": PACKAGE_FORMAT,
        "story_schema": STORY_SCHEMA,
        "content_schema": CONTENT_SCHEMA,
        "id": "package-id",
        "campaign_id": "stable-campaign",
        "name": "Regression package",
        "version": "1.0.1",
        "author": "tests",
        "description": "Campaign v3 contract",
        "entry": "main",
        "campaign": {"new_game": True},
    }
    value.update(changes)
    return value


def story(node):
    return {
        "story_schema": STORY_SCHEMA,
        "id": "main",
        "start": node["id"],
        "nodes": [
            node,
            {"id": "win", "type": "end"},
            {"id": "lose", "type": "end"},
        ],
    }


class CampaignV3ManifestTest(unittest.TestCase):
    def test_campaign_id_is_required_and_legacy_packages_are_rejected(self):
        validate_manifest(manifest())

        missing = manifest()
        missing.pop("campaign_id")
        with self.assertRaisesRegex(LomcError, "campaign_id"):
            validate_manifest(missing)

        for legacy in (1, 2):
            old = manifest(format=legacy, package_format=legacy)
            with self.subTest(package_format=legacy), self.assertRaisesRegex(
                LomcError, "重新导出"
            ):
                validate_manifest(old)

    def test_campaign_id_is_not_inferred_from_package_id(self):
        missing = manifest(id="same-name")
        missing.pop("campaign_id")
        with self.assertRaises(LomcError):
            validate_manifest(missing)


class GameplayV3ContractTest(unittest.TestCase):
    def test_combat_character_is_animation_input_not_scene_template(self):
        source = story(
            {
                "id": "fight",
                "type": "combat",
                "character": "user:author.hero",
                "max_health": 900,
                "strength": 77,
                "win": "win",
                "lose": "lose",
            }
        )
        lua = compile_story(source)
        self.assertIn("character=user:author.hero", lua)
        self.assertIn("max_health=900", lua)
        self.assertIn("strength=77", lua)
        self.assertIn('mod_gameplay_start_scene("combat")', lua)
        self.assertNotIn('ChangeScene("Combat", "user:author.hero"', lua)

    def test_battle_total_includes_named_official_characters(self):
        source = story(
            {
                "id": "war",
                "type": "battle",
                "friend_faction": "001",
                "friend_people": 2,
                "friend_characters": ["brother1", "girl4"],
                "enemy_faction": "002",
                "enemy_people": 1,
                "enemy_characters": ["special3"],
                "win": "win",
                "lose": "lose",
            }
        )
        validate_story(source)
        lua = compile_story(source)
        self.assertIn("friend_people=2", lua)
        self.assertIn("friend_characters=brother1,girl4", lua)
        self.assertIn("enemy_people=1", lua)
        self.assertIn('mod_gameplay_start_scene("battle")', lua)

        too_many = story({**source["nodes"][0], "friend_people": 1})
        with self.assertRaisesRegex(LomcError, "超过.*总人数"):
            validate_story(too_many)

        duplicate = story(
            {**source["nodes"][0], "friend_characters": ["brother1", "brother1"]}
        )
        with self.assertRaisesRegex(LomcError, "不得重复"):
            validate_story(duplicate)

        unofficial = story(
            {**source["nodes"][0], "friend_characters": ["user:author.hero"]}
        )
        with self.assertRaisesRegex(LomcError, "官方.*人物"):
            validate_story(unofficial)


if __name__ == "__main__":
    unittest.main()
