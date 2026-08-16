# -*- coding: utf-8 -*-
"""自定义人物/结局图片的托管、打包和回读测试。"""

from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

EDITOR_DIR = Path(__file__).resolve().parent.parent
COMPILER_DIR = EDITOR_DIR.parent / "compiler"
sys.path[:0] = [str(EDITOR_DIR), str(COMPILER_DIR)]

from asset_store import import_image_file, resolve_image_asset  # noqa: E402
import package_io  # noqa: E402
from schema_versions import manifest_versions, STORY_SCHEMA  # noqa: E402


class AssetStoreTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.old_appdata = os.environ.get("APPDATA")
        os.environ["APPDATA"] = self.temp.name

    def tearDown(self):
        if self.old_appdata is None:
            os.environ.pop("APPDATA", None)
        else:
            os.environ["APPDATA"] = self.old_appdata
        self.temp.cleanup()

    def test_custom_intro_image_roundtrip(self):
        source = Path(self.temp.name) / "侠客.png"
        source.write_bytes(b"\x89PNG\r\n\x1a\n" + b"portrait")
        relative, stored = import_image_file(source)
        self.assertTrue(stored.is_file())
        self.assertEqual(resolve_image_asset(relative), stored)

        manifest = {
            **manifest_versions(),
            "id": "image_test",
            "name": "图片测试",
            "version": "1.0.0",
            "author": "tester",
            "description": "test",
            "entry": "main",
            "campaign_id": "campaign_image_test",
            "campaign": {"new_game": True},
        }
        stories = {
            "main": {
                "story_schema": STORY_SCHEMA,
                "id": "main",
                "title": "图片测试",
                "start": "n1",
                "nodes": [
                    {
                        "id": "n1",
                        "type": "intro",
                        "intro_source": "custom",
                        "title": "游侠",
                        "name": "墨小侠",
                        "text": "来历不明。",
                        "image": relative,
                    },
                    {"id": "n2", "type": "end"},
                ],
            }
        }
        package = Path(self.temp.name) / "image_test.lommod"
        package_io.export_lommod(package, manifest, stories)
        with zipfile.ZipFile(package) as archive:
            self.assertIn(relative, archive.namelist())
            lua = archive.read("lua/main.lua").decode("utf-8")
            self.assertIn(relative, lua)
        loaded_manifest, loaded_stories = package_io.import_lommod(package)
        self.assertEqual(loaded_manifest["id"], "image_test")
        loaded_image = loaded_stories["main"]["nodes"][0]["image"]
        self.assertIsNotNone(resolve_image_asset(loaded_image))

    def _write_minimal_package(self, path: Path, extra=None):
        manifest = {"format": 1, "id": "safe", "entry": "main"}
        story = {"id": "main", "start": "n1", "nodes": [{"id": "n1", "type": "end"}]}
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("manifest.json", json.dumps(manifest))
            archive.writestr("story/main.json", json.dumps(story))
            for name, data in extra or []:
                archive.writestr(name, data)

    def test_import_rejects_unsafe_archive_paths(self):
        for index, unsafe in enumerate(
            (
                "assets/user/../../escaped.wav", "/absolute.png", "C:/drive.png",
                "assets\\user\\escaped.wav", "assets//escaped.wav", "assets/./escaped.wav",
            )
        ):
            package = Path(self.temp.name) / f"unsafe-{index}.lommod"
            self._write_minimal_package(package, [(unsafe, b"payload")])
            with self.subTest(path=unsafe), self.assertRaises(package_io.PackError):
                package_io.import_lommod(package)
        self.assertFalse((Path(self.temp.name) / "escaped.wav").exists())

    def test_import_rejects_duplicate_case_and_file_directory_conflicts(self):
        cases = (
            [("assets/A.png", b"a"), ("assets/a.png", b"b")],
            [("assets", b"file"), ("assets/a.png", b"nested")],
        )
        for index, extras in enumerate(cases):
            package = Path(self.temp.name) / f"conflict-{index}.lommod"
            self._write_minimal_package(package, extras)
            with self.subTest(index=index), self.assertRaises(package_io.PackError):
                package_io.import_lommod(package)

    def test_import_rejects_oversized_json_and_too_many_entries(self):
        oversized = Path(self.temp.name) / "oversized.lommod"
        self._write_minimal_package(
            oversized,
            [("story/huge.json", b"{" + b" " * package_io.MAX_JSON_BYTES + b"}")],
        )
        with self.assertRaises(package_io.PackError) as cm:
            package_io.import_lommod(oversized)
        self.assertIn("过大", str(cm.exception))

        crowded = Path(self.temp.name) / "crowded.lommod"
        extras = [(f"unused/{i}", b"") for i in range(package_io.MAX_ARCHIVE_ENTRIES)]
        self._write_minimal_package(crowded, extras)
        with self.assertRaises(package_io.PackError) as cm:
            package_io.import_lommod(crowded)
        self.assertIn("条目过多", str(cm.exception))

    def test_import_wraps_malformed_json_as_pack_error(self):
        package = Path(self.temp.name) / "bad-json.lommod"
        with zipfile.ZipFile(package, "w") as archive:
            archive.writestr("manifest.json", b"\xff")
            archive.writestr("story/main.json", b"{}")
        with self.assertRaises(package_io.PackError):
            package_io.import_lommod(package)


if __name__ == "__main__":
    unittest.main(verbosity=2)
