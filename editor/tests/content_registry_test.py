# -*- coding: utf-8 -*-
"""User Content Registry：注册 / 解析 / 非法 ID / 删除 / 打包收集。"""

from __future__ import annotations

import os
import struct
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

EDITOR_DIR = Path(__file__).resolve().parent.parent
COMPILER_DIR = EDITOR_DIR.parent / "compiler"
sys.path[:0] = [str(EDITOR_DIR), str(COMPILER_DIR)]

import content_registry  # noqa: E402
import package_io  # noqa: E402
from lomc.content import parse_content_ref  # noqa: E402
from lomc.errors import LomcError  # noqa: E402


def _minimal_wav() -> bytes:
    data = b"\x00\x00" * 16
    return (
        b"RIFF"
        + struct.pack("<I", 36 + len(data))
        + b"WAVE"
        + b"fmt "
        + struct.pack("<IHHIIHH", 16, 1, 1, 8000, 16000, 2, 16)
        + b"data"
        + struct.pack("<I", len(data))
        + data
    )


class ContentRegistryTest(unittest.TestCase):
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

    def _write_wav(self, name="battle.wav") -> Path:
        path = Path(self.temp.name) / name
        path.write_bytes(_minimal_wav())
        return path

    def test_register_resolve_list(self):
        src = self._write_wav()
        rec = content_registry.register_audio(src, "mohui.battle", "决战曲", "music")
        self.assertEqual(rec.ref, "user:mohui.battle")
        self.assertEqual(rec.name, "决战曲")
        listed = content_registry.list_contents(content_type="audio", audio_kind="music")
        self.assertEqual([item.ref for item in listed], ["user:mohui.battle"])
        got, main = content_registry.resolve("user:mohui.battle", expected_kind="music")
        self.assertEqual(got.content_id, "mohui.battle")
        self.assertTrue(main.is_file())

    def test_duplicate_id(self):
        src = self._write_wav()
        content_registry.register_audio(src, "mohui.battle", "一", "music")
        with self.assertRaises(content_registry.ContentRegistryError) as cm:
            content_registry.register_audio(src, "mohui.battle", "二", "music")
        self.assertIn("已存在", str(cm.exception))

    def test_illegal_id(self):
        src = self._write_wav()
        with self.assertRaises(content_registry.ContentRegistryError):
            content_registry.register_audio(src, "../evil", "坏", "music")
        with self.assertRaises(content_registry.ContentRegistryError):
            content_registry.register_audio(src, "MOHUI.Boss", "坏", "music")
        with self.assertRaises(LomcError):
            parse_content_ref("user:mohui/boss")

    def test_same_file_different_ids(self):
        src = self._write_wav("same.wav")
        a = content_registry.register_audio(src, "mohui.theme_a", "甲", "music")
        b = content_registry.register_audio(src, "mohui.theme_b", "乙", "music")
        self.assertNotEqual(a.folder, b.folder)
        self.assertTrue((a.folder / a.main_file).is_file())
        self.assertTrue((b.folder / b.main_file).is_file())

    def test_missing_file_and_corrupt_metadata(self):
        src = self._write_wav()
        rec = content_registry.register_audio(src, "mohui.broken", "坏元数据", "music")
        (rec.folder / rec.main_file).unlink()
        with self.assertRaises(content_registry.ContentRegistryError) as cm:
            content_registry.resolve("user:mohui.broken")
        self.assertIn("不存在", str(cm.exception))
        src2 = self._write_wav("ok.wav")
        rec2 = content_registry.register_audio(src2, "mohui.meta", "元数据", "music")
        (rec2.folder / "content.json").write_text("{not json", encoding="utf-8")
        with self.assertRaises(content_registry.ContentRegistryError):
            content_registry.resolve("user:mohui.meta")

    def test_path_traversal_filename(self):
        src = self._write_wav()
        with self.assertRaises(content_registry.ContentRegistryError):
            content_registry.register_audio(src, "mohui..hack", "穿越", "music")

    def test_delete_blocks_when_referenced(self):
        src = self._write_wav()
        content_registry.register_audio(src, "mohui.used", "在用", "music")
        stories = {
            "main": {
                "id": "main",
                "start": "n1",
                "nodes": [
                    {"id": "n1", "type": "music", "name": "user:mohui.used"},
                    {"id": "n2", "type": "end"},
                ],
            }
        }
        with self.assertRaises(content_registry.ContentRegistryError) as cm:
            content_registry.remove("mohui.used", stories=stories)
        self.assertIn("仍被", str(cm.exception))
        content_registry.remove("mohui.used", stories={"main": {"id": "main", "nodes": []}})
        with self.assertRaises(content_registry.ContentRegistryError):
            content_registry.get("mohui.used")

    def test_export_includes_only_referenced_audio(self):
        used = self._write_wav("used.wav")
        unused = self._write_wav("unused.wav")
        content_registry.register_audio(used, "mohui.used", "用到的", "music")
        content_registry.register_audio(unused, "mohui.unused", "没用的", "music")
        manifest = {
            "format": 1,
            "id": "audio_test",
            "name": "音频测试",
            "version": "1.0.0",
            "author": "tester",
            "description": "test",
            "entry": "main",
        }
        stories = {
            "main": {
                "id": "main",
                "title": "音频测试",
                "start": "n1",
                "nodes": [
                    {"id": "n1", "type": "music", "name": "user:mohui.used"},
                    {"id": "n2", "type": "end"},
                ],
            }
        }
        package = Path(self.temp.name) / "audio_test.lommod"
        package_io.export_lommod(package, manifest, stories)
        with zipfile.ZipFile(package) as archive:
            names = archive.namelist()
            self.assertIn("assets/user/audio/mohui.used/content.json", names)
            self.assertTrue(any(n.startswith("assets/user/audio/mohui.used/") and n.endswith(".wav") for n in names))
            self.assertFalse(any("mohui.unused" in n for n in names))
            lua = archive.read("lua/main.lua").decode("utf-8")
            self.assertIn("user:mohui.used", lua)
            self.assertNotIn(str(used), lua)

    def test_wrong_kind_on_export(self):
        src = self._write_wav()
        content_registry.register_audio(src, "mohui.sfx", "敲门", "sound")
        manifest = {
            "format": 1,
            "id": "kind_test",
            "name": "类型测试",
            "version": "1.0.0",
            "author": "tester",
            "description": "test",
            "entry": "main",
        }
        stories = {
            "main": {
                "id": "main",
                "start": "n1",
                "nodes": [
                    {"id": "n1", "type": "music", "name": "user:mohui.sfx"},
                    {"id": "n2", "type": "end"},
                ],
            }
        }
        package = Path(self.temp.name) / "kind_test.lommod"
        with self.assertRaises(package_io.PackError) as cm:
            package_io.export_lommod(package, manifest, stories)
        self.assertIn("音效", str(cm.exception))


if __name__ == "__main__":
    unittest.main(verbosity=2)
