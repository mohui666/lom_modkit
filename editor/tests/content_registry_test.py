# -*- coding: utf-8 -*-
"""User Content Registry：注册 / 解析 / 非法 ID / 删除 / 打包收集。"""

from __future__ import annotations

import json
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

    def _write_png(self, name="image.png") -> Path:
        path = Path(self.temp.name) / name
        path.write_bytes(
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01"
            b"\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\xcf\xc0"
            b"\x00\x00\x00\x03\x00\x01\x00\x05\xfe\xd4\xef\x00\x00\x00\x00IEND\xaeB`\x82"
        )
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

    def test_generic_image_register_thumbnail_source_and_references(self):
        src = self._write_png("moon.jpeg")
        rec = content_registry.register_image(src, "mohui.moon_bg", "月夜")
        self.assertEqual(rec.type, "image")
        self.assertEqual(rec.ref, "user:mohui.moon_bg")
        self.assertTrue((rec.folder / rec.main_file).is_file())
        got, main = content_registry.resolve(
            rec.ref, expected_type="image"
        )
        self.assertEqual(got.name, "月夜")
        self.assertEqual(main, rec.folder / rec.main_file)
        renamed = content_registry.update_image(rec.content_id, "月夜改")
        self.assertEqual(renamed.name, "月夜改")
        stories = {
            "main": {
                "nodes": [
                    {
                        "id": "n1",
                        "type": "future_image_node",
                        "image": rec.ref,
                    }
                ]
            }
        }
        refs = content_registry.find_references(rec.content_id, stories)
        self.assertEqual([(r["story_id"], r["node_id"]) for r in refs], [("main", "n1")])
        with self.assertRaises(content_registry.ContentRegistryError):
            content_registry.remove(rec.content_id, stories=stories)
        target = Path(self.temp.name) / "mod_copy"
        copied = content_registry.copy_into_mod(
            target, rec.content_id, expected_type="image"
        )
        self.assertTrue((copied / "content.json").is_file())
        self.assertTrue((copied / rec.main_file).is_file())

    def test_duplicate_id(self):
        src = self._write_wav()
        content_registry.register_audio(src, "mohui.battle", "一", "music")
        with self.assertRaises(content_registry.ContentRegistryError) as cm:
            content_registry.register_audio(src, "mohui.battle", "二", "music")
        self.assertIn("已存在", str(cm.exception))

    def test_content_id_is_unique_across_types(self):
        audio = self._write_wav("same.wav")
        image = self._write_png("same.png")
        content_registry.register_audio(audio, "mohui.same", "音频", "sound")
        with self.assertRaises(content_registry.ContentRegistryError) as cm:
            content_registry.register_image(image, "mohui.same", "图片")
        self.assertIn("所有类型之间必须唯一", str(cm.exception))

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

    def test_register_character_and_pack(self):
        png = (
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01"
            b"\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\xcf\xc0"
            b"\x00\x00\x00\x03\x00\x01\x00\x05\xfe\xd4\xef\x00\x00\x00\x00IEND\xaeB`\x82"
        )
        normal = Path(self.temp.name) / "normal.png"
        happy = Path(self.temp.name) / "happy.png"
        normal.write_bytes(png)
        happy.write_bytes(png)
        rec = content_registry.register_character(
            {"normal": normal, "happy": happy},
            "mohui.luoxue",
            "洛雪",
        )
        self.assertEqual(rec.ref, "user:mohui.luoxue")
        self.assertEqual(rec.portrait_ids()[0], "normal")
        self.assertIn("happy", rec.portrait_ids())
        got, main = content_registry.resolve(
            "user:mohui.luoxue", expected_type="character", portrait="happy"
        )
        self.assertEqual(got.name, "洛雪")
        self.assertTrue(main.is_file())
        with self.assertRaises(content_registry.ContentRegistryError):
            content_registry.resolve(
                "user:mohui.luoxue", expected_type="character", portrait="angry"
            )
        sad = Path(self.temp.name) / "sad.png"
        sad.write_bytes(png)
        updated = content_registry.update_character(
            "mohui.luoxue",
            name="洛雪改",
            portraits={"sad": sad},
        )
        self.assertEqual(updated.name, "洛雪改")
        self.assertIn("sad", updated.portrait_ids())
        self.assertIn("happy", updated.portrait_ids())
        titled = content_registry.update_character("mohui.luoxue", title="江湖新秀")
        self.assertEqual(titled.title, "江湖新秀")
        sized = content_registry.update_character(
            "mohui.luoxue", scale=80, art_facing="right"
        )
        self.assertEqual(sized.scale, 80)
        self.assertEqual(sized.art_facing, "right")
        self.assertEqual(sized.title, "江湖新秀")
        intro_img = Path(self.temp.name) / "intro.png"
        intro_img.write_bytes(png)
        with_intro = content_registry.update_character_intro(
            "mohui.luoxue",
            title="江湖新秀",
            name="洛雪",
            text="来历不明。",
            image=intro_img,
        )
        self.assertIsNotNone(with_intro.intro)
        self.assertEqual(with_intro.intro["name"], "洛雪")
        self.assertTrue((with_intro.folder / with_intro.intro["image"]).is_file())
        self.assertEqual(with_intro.title, "江湖新秀")
        self.assertEqual(with_intro.scale, 80)
        self.assertEqual(with_intro.art_facing, "right")
        cleared = content_registry.update_character_intro("mohui.luoxue", clear=True)
        self.assertIsNone(cleared.intro)
        # 称号仍保留
        self.assertEqual(cleared.title, "江湖新秀")

        manifest = {
            "format": 1,
            "id": "char_test",
            "name": "角色测试",
            "version": "1.0.0",
            "author": "tester",
            "description": "test",
            "entry": "main",
        }
        stories = {
            "main": {
                "id": "main",
                "title": "角色",
                "start": "n1",
                "nodes": [
                    {
                        "id": "n1",
                        "type": "show",
                        "character": "user:mohui.luoxue",
                        "position": "M",
                        "portrait": "happy",
                    },
                    {"id": "n2", "type": "end"},
                ],
            }
        }
        package = Path(self.temp.name) / "char_test.lommod"
        package_io.export_lommod(package, manifest, stories)
        with zipfile.ZipFile(package) as archive:
            names = archive.namelist()
            self.assertIn("assets/user/character/mohui.luoxue/content.json", names)
            self.assertTrue(any(n.endswith("happy.png") for n in names))
            lua = archive.read("lua/main.lua").decode("utf-8")
            self.assertIn("mod_char_show", lua)

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

    def test_audio_character_affiliation_and_compat(self):
        png = (
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01"
            b"\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\xcf\xc0"
            b"\x00\x00\x00\x03\x00\x01\x00\x05\xfe\xd4\xef\x00\x00\x00\x00IEND\xaeB`\x82"
        )
        normal = Path(self.temp.name) / "normal.png"
        normal.write_bytes(png)
        content_registry.register_character(
            {"normal": normal}, "mohui.luoxue", "洛雪"
        )
        src = self._write_wav("hello.wav")
        linked = content_registry.register_audio(
            src, "mohui.hello", "问候", "sound", character="user:mohui.luoxue"
        )
        self.assertEqual(linked.character, "user:mohui.luoxue")
        narr = content_registry.register_audio(
            src, "mohui.narrator", "旁白", "sound"
        )
        self.assertIsNone(narr.character)
        official = content_registry.register_audio(
            src, "mohui.player_hi", "主角", "sound", character="player"
        )
        self.assertEqual(official.character, "player")
        voices = content_registry.list_character_voices("user:mohui.luoxue")
        self.assertEqual([v.ref for v in voices], ["user:mohui.hello"])
        mine = content_registry.voices_for_say_picker("user:mohui.luoxue")
        self.assertEqual([v.ref for v in mine], ["user:mohui.hello"])
        grouped, others = content_registry.group_voices_for_speaker("user:mohui.luoxue")
        self.assertEqual([v.ref for v in grouped], ["user:mohui.hello"])
        self.assertEqual(others, [])
        narration = content_registry.voices_for_say_picker(None)
        narr_refs = {v.ref for v in narration}
        self.assertIn("user:mohui.narrator", narr_refs)
        self.assertNotIn("user:mohui.hello", narr_refs)
        self.assertNotIn("user:mohui.player_hi", narr_refs)
        player = content_registry.voices_for_say_picker("player")
        self.assertEqual([v.ref for v in player], ["user:mohui.player_hi"])
        renamed = content_registry.update_audio("mohui.hello", name="问候改")
        self.assertEqual(renamed.name, "问候改")
        self.assertEqual(renamed.character, "user:mohui.luoxue")
        unlinked = content_registry.update_audio("mohui.hello", character=None)
        self.assertIsNone(unlinked.character)
        self.assertEqual(content_registry.list_character_voices("user:mohui.luoxue"), [])

    def test_old_voice_metadata_without_character(self):
        src = self._write_wav("legacy.wav")
        rec = content_registry.register_audio(src, "mohui.legacy", "旧语音", "sound")
        raw = json.loads((rec.folder / "content.json").read_text(encoding="utf-8"))
        self.assertNotIn("character", raw)
        got = content_registry.get("mohui.legacy")
        self.assertIsNone(got.character)

    def test_delete_character_unlinks_voices(self):
        png = (
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01"
            b"\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\xcf\xc0"
            b"\x00\x00\x00\x03\x00\x01\x00\x05\xfe\xd4\xef\x00\x00\x00\x00IEND\xaeB`\x82"
        )
        normal = Path(self.temp.name) / "c.png"
        normal.write_bytes(png)
        content_registry.register_character({"normal": normal}, "mohui.luoxue", "洛雪")
        src = self._write_wav("hi.wav")
        content_registry.register_audio(
            src, "mohui.hi", "问候", "sound", character="user:mohui.luoxue"
        )
        content_registry.remove("mohui.luoxue")
        leftover = content_registry.get("mohui.hi")
        self.assertIsNone(leftover.character)
        raw = json.loads((leftover.folder / "content.json").read_text(encoding="utf-8"))
        self.assertNotIn("character", raw)

    def test_delete_voice_blocked_by_say_voice(self):
        src = self._write_wav("used.wav")
        content_registry.register_audio(src, "mohui.line", "台词", "sound")
        stories = {
            "main": {
                "id": "main",
                "start": "n1",
                "nodes": [
                    {
                        "id": "n1",
                        "type": "say",
                        "mode": "narrative",
                        "text": "有语音",
                        "voice": "user:mohui.line",
                    },
                    {"id": "n2", "type": "end"},
                ],
            }
        }
        with self.assertRaises(content_registry.ContentRegistryError) as cm:
            content_registry.remove("mohui.line", stories=stories)
        self.assertIn("仍被", str(cm.exception))

    def test_pack_does_not_include_unused_character_voices(self):
        png = (
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01"
            b"\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\xcf\xc0"
            b"\x00\x00\x00\x03\x00\x01\x00\x05\xfe\xd4\xef\x00\x00\x00\x00IEND\xaeB`\x82"
        )
        normal = Path(self.temp.name) / "n.png"
        normal.write_bytes(png)
        content_registry.register_character({"normal": normal}, "mohui.luoxue", "洛雪")
        used = self._write_wav("used.wav")
        unused = self._write_wav("unused.wav")
        content_registry.register_audio(
            used, "mohui.used", "用到", "sound", character="user:mohui.luoxue"
        )
        content_registry.register_audio(
            unused, "mohui.unused", "没用", "sound", character="user:mohui.luoxue"
        )
        package = Path(self.temp.name) / "voice_pack.lommod"
        package_io.export_lommod(
            package,
            {
                "format": 1,
                "id": "voice_pack",
                "name": "语音打包",
                "version": "1.0.0",
                "author": "tester",
                "description": "test",
                "entry": "main",
            },
            {
                "main": {
                    "id": "main",
                    "title": "语音",
                    "start": "n1",
                    "nodes": [
                        {
                            "id": "n1",
                            "type": "say",
                            "character": "user:mohui.luoxue",
                            "text": "用到",
                            "voice": "user:mohui.used",
                        },
                        {"id": "n2", "type": "end"},
                    ],
                }
            },
        )
        with zipfile.ZipFile(package) as archive:
            names = archive.namelist()
            self.assertIn("assets/user/audio/mohui.used/content.json", names)
            self.assertFalse(any("mohui.unused" in n for n in names))
            meta = json.loads(archive.read("assets/user/audio/mohui.used/content.json"))
            self.assertEqual(meta.get("character"), "user:mohui.luoxue")


class AudioPreviewTest(unittest.TestCase):
    def test_missing_file_raises(self):
        from audio_preview import AudioPreviewError, play_audio_file

        with self.assertRaises(AudioPreviewError):
            play_audio_file(Path("C:/definitely-missing-voice.wav"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
