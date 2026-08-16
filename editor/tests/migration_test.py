# -*- coding: utf-8 -*-
from __future__ import annotations

import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

EDITOR = Path(__file__).resolve().parents[1]
COMPILER = EDITOR.parent / "compiler"
sys.path[:0] = [str(EDITOR), str(COMPILER)]

import migration  # noqa: E402
import models  # noqa: E402
from schema_versions import CONTENT_SCHEMA, PACKAGE_FORMAT, STORY_SCHEMA  # noqa: E402


def legacy_story() -> dict:
    return {
        "id": "main",
        "start": "end1",
        "unknown_top": {"future": [1, 2, 3]},
        "nodes": [
            {"id": "end1", "type": "end", "unknown_node": {"keep": True}}
        ],
    }


def migratable_story() -> dict:
    return {
        "story_schema": STORY_SCHEMA,
        "id": "main", "start": "roll",
        "nodes": [
            {"id": "roll", "type": "dice", "check": "S0205_01_001",
             "options": [{"goto_大成功": "end1", "goto_成功": "end1",
                          "goto_失败": "end1"}]},
            {"id": "end1", "type": "end"},
        ],
    }


class MigrationPureTest(unittest.TestCase):
    def test_pure_migrations_preserve_unknown_fields_and_input(self):
        source_story = {**legacy_story(), "story_schema": STORY_SCHEMA}
        untouched = copy.deepcopy(source_story)
        story = migration.migrate_story(source_story)
        self.assertFalse(story.changed)
        self.assertEqual(story.document["story_schema"], STORY_SCHEMA)
        self.assertEqual(story.document["unknown_top"], {"future": [1, 2, 3]})
        self.assertTrue(story.document["nodes"][0]["unknown_node"]["keep"])
        self.assertEqual(source_story, untouched)

        with self.assertRaisesRegex(migration.MigrationError, "不兼容"):
            migration.migrate_manifest(
                {"format": 1, "id": "old", "entry": "main"}
            )

        content = migration.migrate_content(
            {
                "schema": 1,
                "id": "mohui.test",
                "type": "image",
                "vendor_extension": ["keep"],
            }
        )
        self.assertEqual(content.document["content_schema"], CONTENT_SCHEMA)
        self.assertEqual(content.document["vendor_extension"], ["keep"])

    def test_unknown_null_bool_and_conflicting_versions_are_rejected(self):
        for document, migrator in (
            ({**legacy_story(), "story_schema": None}, migration.migrate_story),
            ({**legacy_story(), "story_schema": True}, migration.migrate_story),
            ({"format": 1, "package_format": 2}, migration.migrate_manifest),
            ({"format": 1, "content_schema": 2}, migration.migrate_manifest),
            ({"schema": 1, "content_schema": 2}, migration.migrate_content),
        ):
            with self.subTest(document=document), self.assertRaises(migration.MigrationError):
                migrator(document)

    def test_legacy_dice_is_expanded_and_localized_paths_follow_result_bands(self):
        source = {
            "story_schema": STORY_SCHEMA,
            "id": "main", "start": "roll", "nodes": [
                {
                    "id": "roll", "type": "dice", "check": "S0205_01_001",
                    "options": [{
                        "goto_大成功": "ok", "goto_成功": "ok", "goto_失败": "bad",
                        "band_texts": ["失手", "成功"],
                    }],
                },
                {"id": "ok", "type": "end"}, {"id": "bad", "type": "end"},
            ],
            "localization": {
                "default_locale": "chs", "fallback_locale": "chs",
                "translations": {"ja": {
                    "roll.options.0.band_texts.0": "失敗",
                    "roll.options.0.band_texts.1": "成功",
                }},
            },
        }
        migrated = migration.migrate_story(source).document
        roll = migrated["nodes"][0]
        self.assertNotIn("check", roll)
        self.assertEqual([band["text"] for band in roll["bands"]], ["失手", "成功"])
        catalog = migrated["localization"]["translations"]["ja"]
        self.assertEqual(catalog, {
            "roll.bands.0.text": "失敗", "roll.bands.1.text": "成功",
        })

    def test_legacy_dice_with_unreachable_official_band_requires_manual_conversion(self):
        node = {
            "id": "roll", "type": "dice", "check": "dynamic_bonus",
            "options": [{
                "goto_大成功": "great", "goto_成功": "ok", "goto_失败": "bad",
            }],
        }
        metadata = {"dynamic_bonus": {
            "max": 60,
            "bands": [
                {"text": "失败", "cond": "<20"},
                {"text": "成功", "cond": "<80"},
                {"text": "大成功", "cond": ">=80"},
            ],
        }}
        with self.assertRaisesRegex(migration.MigrationError, "动态加值.*无法无损转换"):
            migration._inline_legacy_dice(node, metadata)


class MigrationFileTest(unittest.TestCase):
    def test_file_migration_creates_exact_backup_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "main.json"
            original = json.dumps(migratable_story(), ensure_ascii=False).encode("utf-8")
            source.write_bytes(original)
            result, backup = migration.migrate_json_file(source, "story")
            self.assertTrue(result.changed)
            self.assertIsNotNone(backup)
            self.assertEqual(backup.read_bytes(), original)
            current = json.loads(source.read_text(encoding="utf-8"))
            self.assertEqual(current["story_schema"], STORY_SCHEMA)
            second, second_backup = migration.migrate_json_file(source, "story")
            self.assertFalse(second.changed)
            self.assertIsNone(second_backup)

    def test_validation_or_atomic_replace_failure_preserves_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "main.json"
            original = json.dumps(migratable_story()).encode("utf-8")
            source.write_bytes(original)

            def reject(_document):
                raise ValueError("validator failed")

            with self.assertRaises(ValueError):
                migration.migrate_json_file(source, "story", validator=reject)
            self.assertEqual(source.read_bytes(), original)
            self.assertEqual(list(source.parent.glob("*.bak*")), [])

            with mock.patch.object(
                migration.os, "replace", side_effect=OSError("replace failed")
            ):
                with self.assertRaises(migration.MigrationError):
                    migration.migrate_json_file(source, "story")
            self.assertEqual(source.read_bytes(), original)
            backups = list(source.parent.glob("main.json.pre-migration-v2.bak*"))
            self.assertEqual(len(backups), 1)
            self.assertEqual(backups[0].read_bytes(), original)
            self.assertEqual(list(source.parent.glob("*.migration.tmp")), [])

    def test_recovery_restores_original_and_keeps_replaced_current_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "main.json"
            source.write_text(json.dumps(migratable_story()), encoding="utf-8")
            _result, backup = migration.migrate_json_file(source, "story")
            migrated_bytes = source.read_bytes()
            recovery = migration.restore_migration_backup(source, backup)
            restored = json.loads(source.read_text(encoding="utf-8"))
            self.assertIn("check", restored["nodes"][0])
            self.assertEqual(recovery.read_bytes(), migrated_bytes)

    def test_models_load_migrates_legacy_story_on_disk(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "main.json"
            original = json.dumps(migratable_story()).encode("utf-8")
            source.write_bytes(original)
            loaded = models.load_story(source)
            self.assertEqual(loaded["story_schema"], STORY_SCHEMA)
            self.assertNotIn("check", loaded["nodes"][0])
            self.assertEqual(
                json.loads(source.read_text(encoding="utf-8"))["story_schema"],
                STORY_SCHEMA,
            )
            backup = source.with_name(source.name + ".pre-migration-v2.bak")
            self.assertEqual(backup.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
