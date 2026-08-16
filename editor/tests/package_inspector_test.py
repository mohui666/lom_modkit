# -*- coding: utf-8 -*-
from __future__ import annotations

import os
from pathlib import Path
import sys
import tempfile
import unittest
import zipfile

EDITOR_DIR = Path(__file__).resolve().parent.parent
COMPILER_DIR = EDITOR_DIR.parent / "compiler"
sys.path[:0] = [str(EDITOR_DIR), str(COMPILER_DIR)]

from lomc.deterministic_zip import DeterministicPackageBuilder, stable_json_bytes  # noqa: E402
from lomc import compile_story  # noqa: E402
from lomc.story_lua_integrity import (  # noqa: E402
    STORY_LUA_INTEGRITY_ENTRY,
    build_story_lua_integrity,
)
from schema_versions import PACKAGE_FORMAT  # noqa: E402
import package_io  # noqa: E402
from package_inspector import inspect_lommod  # noqa: E402


def _manifest(**extra):
    result = {
        "format": PACKAGE_FORMAT,
        "package_format": PACKAGE_FORMAT,
        "story_schema": 1,
        "content_schema": 1,
        "id": "inspect_test",
        "name": "检查器测试",
        "version": "1.0.0",
        "author": "tester",
        "description": "package inspector",
        "entry": "main",
    }
    result.update(extra)
    return result


def _story():
    return {
        "story_schema": 1,
        "id": "main",
        "title": "检查器测试",
        "start": "n1",
        "nodes": [{"id": "n1", "type": "end"}],
    }


class PackageInspectorTest(unittest.TestCase):
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

    def test_exported_package_lists_and_verifies_all_text_layers(self):
        package = Path(self.temp.name) / "good.lommod"
        package_io.export_lommod(package, _manifest(), {"main": _story()})
        result = inspect_lommod(package)
        self.assertEqual(result.errors, [])
        self.assertTrue(result.content_hash_valid)
        self.assertEqual(result.manifest["id"], "inspect_test")
        self.assertEqual(len(result.stories), 1)
        self.assertEqual(len(result.lua_files), 1)
        self.assertIn('"id": "main"', result.stories[0].preview)
        self.assertIn("luamanager.ChangeScene", result.lua_files[0].preview)
        self.assertEqual(len(result.package_sha256), 64)
        self.assertTrue(all(len(item.sha256) == 64 for item in result.entries))

    def test_tampered_logical_content_is_reported(self):
        source = Path(self.temp.name) / "source.lommod"
        target = Path(self.temp.name) / "tampered.lommod"
        package_io.export_lommod(source, _manifest(), {"main": _story()})
        with zipfile.ZipFile(source) as reader, zipfile.ZipFile(
            target, "w", zipfile.ZIP_DEFLATED
        ) as writer:
            for info in reader.infolist():
                data = reader.read(info)
                if info.filename == "lua/main.lua":
                    data += b"\n-- tampered\n"
                writer.writestr(info.filename, data)
        result = inspect_lommod(target)
        self.assertFalse(result.content_hash_valid)
        self.assertTrue(any("不一致" in message for message in result.errors))

    def test_matching_hashes_cannot_hide_lua_not_compiled_from_story(self):
        package = Path(self.temp.name) / "substituted.lommod"
        manifest = _manifest()
        story = _story()
        story_data = stable_json_bytes(story)
        bad_lua = b"return 'substituted'\n"
        builder = DeterministicPackageBuilder()
        builder.add_json("manifest.json", manifest)
        builder.add_bytes("story/main.json", story_data)
        builder.add_bytes("lua/main.lua", bad_lua)
        builder.add_bytes(
            STORY_LUA_INTEGRITY_ENTRY,
            build_story_lua_integrity([
                ("story/main.json", story_data, "lua/main.lua", bad_lua)
            ]),
        )
        builder.write(package)
        result = inspect_lommod(package)
        self.assertTrue(result.content_hash_valid)
        self.assertTrue(any("不是由对应" in message for message in result.errors))
        with self.assertRaisesRegex(package_io.PackError, "不是由对应"):
            package_io.import_lommod(package)

    def test_v2_requires_story_lua_integrity_record(self):
        package = Path(self.temp.name) / "missing-story-lua-hash.lommod"
        manifest = _manifest()
        story = _story()
        with zipfile.ZipFile(package, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("manifest.json", stable_json_bytes(manifest))
            archive.writestr("story/main.json", stable_json_bytes(story))
            archive.writestr(
                "lua/main.lua",
                compile_story(story, mod_info=manifest, source="story/main.json"),
            )
        with self.assertRaisesRegex(package_io.PackError, "必须包含 story-lua"):
            package_io.import_lommod(package)
        result = inspect_lommod(package)
        self.assertTrue(any("必须包含 story-lua" in error for error in result.errors))

    def test_references_compatibility_and_unused_assets_are_visible(self):
        package = Path(self.temp.name) / "refs.lommod"
        story = {
            "story_schema": 1,
            "id": "main",
            "title": "资源",
            "start": "n0",
            "nodes": [
                {"id": "n0", "type": "music", "name": "user:mohui.voice"},
                {
                    "id": "n1",
                    "type": "intro",
                    "intro_source": "custom",
                    "title": "侠客",
                    "name": "墨小侠",
                    "text": "来历不明。",
                    "image": "assets/missing.png",
                },
                {"id": "n2", "type": "end"},
            ],
        }
        builder = DeterministicPackageBuilder()
        builder.add_json("manifest.json", _manifest(min_host_version="9.0.0"))
        builder.add_json("story/main.json", story)
        builder.add_bytes("lua/main.lua", "return nil\n")
        builder.add_bytes("assets/unused.png", b"PNG")
        builder.add_json(
            "assets/user/audio/mohui.voice/content.json",
            {
                "content_schema": 1,
                "id": "mohui.voice",
                "type": "audio",
                "name": "测试语音",
                "audio_kind": "music",
                "files": {"main": "voice.wav"},
            },
        )
        builder.add_bytes("assets/user/audio/mohui.voice/voice.wav", b"RIFF")
        builder.write(package)
        result = inspect_lommod(package, current_host_version="0.6.0")
        self.assertIn("assets/missing.png", result.missing_assets)
        self.assertIn("assets/unused.png", result.unreferenced_assets)
        self.assertEqual(len(result.user_contents), 2)
        self.assertNotIn(
            "assets/user/audio/mohui.voice/voice.wav", result.unreferenced_assets
        )
        self.assertTrue(any("Host" in message for message in result.errors))
        self.assertTrue(result.content_hash_valid)


if __name__ == "__main__":
    unittest.main()
