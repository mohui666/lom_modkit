# -*- coding: utf-8 -*-
from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path
import zipfile

EDITOR = Path(__file__).resolve().parents[1]
COMPILER = EDITOR.parent / "compiler"
sys.path[:0] = [str(EDITOR), str(COMPILER)]

import package_io  # noqa: E402
from schema_versions import (  # noqa: E402
    CONTENT_SCHEMA,
    PACKAGE_FORMAT,
    STORY_SCHEMA,
    manifest_versions,
)
from lomc import schema_versions as compiler_versions  # noqa: E402


class SchemaVersionsTest(unittest.TestCase):
    def test_editor_compiler_constants_match(self):
        self.assertEqual(PACKAGE_FORMAT, compiler_versions.PACKAGE_FORMAT)
        self.assertEqual(STORY_SCHEMA, compiler_versions.STORY_SCHEMA)
        self.assertEqual(CONTENT_SCHEMA, compiler_versions.CONTENT_SCHEMA)
        self.assertEqual(
            manifest_versions(),
            {
                "format": 1,
                "package_format": 1,
                "story_schema": 1,
                "content_schema": 1,
            },
        )
        runtime = (
            EDITOR.parent / "runtime" / "MortalModHost" / "src" / "ModLoader.cs"
        ).read_text(encoding="utf-8")
        self.assertIn("const int PackageFormat = %d" % PACKAGE_FORMAT, runtime)
        self.assertIn("const int StorySchema = %d" % STORY_SCHEMA, runtime)
        self.assertIn("const int ContentSchema = %d" % CONTENT_SCHEMA, runtime)

    @staticmethod
    def _package(path: Path, manifest: dict, story: dict) -> None:
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("manifest.json", json.dumps(manifest))
            archive.writestr("story/main.json", json.dumps(story))

    def test_import_accepts_legacy_v1_and_rejects_unknown_explicit_versions(self):
        story = {
            "id": "main",
            "start": "end1",
            "nodes": [{"id": "end1", "type": "end"}],
        }
        legacy = {"format": 1, "id": "legacy", "entry": "main"}
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            old_package = root / "old.lommod"
            self._package(old_package, legacy, story)
            manifest, stories = package_io.import_lommod(old_package)
            self.assertEqual(manifest["id"], "legacy")
            self.assertEqual(stories["main"]["id"], "main")

            cases = (
                ({**legacy, "package_format": 2}, story),
                ({**legacy, "story_schema": 2}, story),
                ({**legacy, "content_schema": 2}, story),
                ({**legacy, "content_schema": None}, story),
                (legacy, {**story, "story_schema": 2}),
                (legacy, {**story, "story_schema": None}),
            )
            for index, (bad_manifest, bad_story) in enumerate(cases):
                package = root / ("bad-%d.lommod" % index)
                self._package(package, bad_manifest, bad_story)
                with self.subTest(index=index), self.assertRaises(package_io.PackError):
                    package_io.import_lommod(package)

    def test_export_does_not_silently_downgrade_future_versions(self):
        story = {
            "story_schema": STORY_SCHEMA,
            "id": "main",
            "start": "end1",
            "nodes": [{"id": "end1", "type": "end"}],
        }
        manifest = {
            **manifest_versions(),
            "id": "future",
            "name": "future",
            "version": "1",
            "author": "test",
            "description": "test",
            "entry": "main",
        }
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "future.lommod"
            with self.assertRaises(package_io.PackError):
                package_io.export_lommod(
                    output,
                    {**manifest, "package_format": 2},
                    {"main": story},
                )
            with self.assertRaises(package_io.PackError):
                package_io.export_lommod(
                    output,
                    manifest,
                    {"main": {**story, "story_schema": 2}},
                )


if __name__ == "__main__":
    unittest.main()
