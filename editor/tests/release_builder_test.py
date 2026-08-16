# -*- coding: utf-8 -*-
from pathlib import Path
import hashlib
import sys
import tempfile
import unittest

EDITOR = Path(__file__).resolve().parent.parent
COMPILER = EDITOR.parent / "compiler"
sys.path[:0] = [str(EDITOR), str(COMPILER)]

import models
from release_builder import ReleaseBuildBlocked, build_release
from schema_versions import STORY_SCHEMA


class ReleaseBuilderTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.editor_data, _ = models.load_editor_data(EDITOR.parent)

    @staticmethod
    def _story(text="完成对白"):
        return {
            "story_schema": STORY_SCHEMA,
            "id": "main", "title": "发布剧情", "start": "say1", "mood": False,
            "nodes": [
                {"id": "say1", "type": "say", "character": "player", "text": text},
                {"id": "end1", "type": "end"},
            ],
        }

    @staticmethod
    def _manifest(version="1.2.3"):
        return {
            "id": "release_demo", "name": "Release Demo", "version": version,
            "author": "Author", "description": "Offline release", "entry": "main",
            "campaign_id": "campaign_release_demo",
            "campaign": {"new_game": True},
        }

    def test_builds_package_checksum_and_summary_without_installing(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            result = build_release(
                root / "release_demo",
                self._manifest(),
                {"main": self._story()},
                self.editor_data,
                "0.7.0",
                content_root=root / "content",
                bundled_assets=[],
            )
            self.assertEqual(result.package_path.suffix, ".lommod")
            self.assertTrue(result.package_path.is_file())
            self.assertTrue(result.checksum_path.is_file())
            expected = hashlib.sha256(result.package_path.read_bytes()).hexdigest().upper()
            self.assertEqual(result.package_sha256, expected)
            self.assertEqual(
                result.checksum_path.read_text(encoding="utf-8"),
                "%s  %s\n" % (expected, result.package_path.name),
            )
            self.assertEqual((result.story_count, result.node_count), (1, 2))
            self.assertEqual(len(result.compile_report), 1)

    def test_invalid_semver_blocks_before_creating_output(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "bad.lommod"
            with self.assertRaises(ReleaseBuildBlocked) as caught:
                build_release(
                    output,
                    self._manifest("version one"),
                    {"main": self._story()},
                    self.editor_data,
                    "0.7.0",
                    content_root=Path(temp) / "content",
                )
            self.assertFalse(output.exists())
            self.assertTrue(any(
                issue.code == "invalid_release_version"
                for issue in caught.exception.issues
            ))

    def test_release_placeholder_blocks_before_creating_output(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "placeholder.lommod"
            with self.assertRaises(ReleaseBuildBlocked) as caught:
                build_release(
                    output,
                    self._manifest(),
                    {"main": self._story("在这里填写对白")},
                    self.editor_data,
                    "0.7.0",
                    content_root=Path(temp) / "content",
                )
            issues = caught.exception.issues
            self.assertTrue(any(i.code == "placeholder_text" and i.severity == "error" for i in issues))
            self.assertFalse(output.exists())
            self.assertFalse(Path(str(output) + ".sha256").exists())


if __name__ == "__main__":
    unittest.main()
