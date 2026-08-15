# -*- coding: utf-8 -*-
from __future__ import annotations

import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR_DIR = Path(__file__).resolve().parent.parent
COMPILER_DIR = EDITOR_DIR.parent / "compiler"
sys.path[:0] = [str(EDITOR_DIR), str(COMPILER_DIR)]

import content_registry  # noqa: E402
from content_library_dialog import ContentLibraryDialog  # noqa: E402
from content_pack import (  # noqa: E402
    ContentRegistryError,
    content_pack_defaults,
    export_content_pack,
    import_content_pack,
    inspect_content_pack,
)
from PySide6.QtWidgets import QApplication, QPushButton  # noqa: E402


PNG = (
    b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01"
    b"\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\xcf\xc0"
    b"\x00\x00\x00\x03\x00\x01\x00\x05\xfe\xd4\xef\x00\x00\x00\x00IEND\xaeB`\x82"
)


class ContentPackTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.old_appdata = os.environ.get("APPDATA")
        os.environ["APPDATA"] = self.temp.name
        self.root = Path(self.temp.name)

    def tearDown(self):
        if self.old_appdata is None:
            os.environ.pop("APPDATA", None)
        else:
            os.environ["APPDATA"] = self.old_appdata
        self.temp.cleanup()

    def _wav(self, name="voice.wav") -> Path:
        path = self.root / name
        path.write_bytes(b"RIFF\x04\x00\x00\x00WAVE")
        return path

    def _png(self, name="image.png") -> Path:
        path = self.root / name
        path.write_bytes(PNG)
        return path

    def _roundtrip(self, rec):
        one = self.root / (rec.content_id + "-1.lomcontent")
        two = self.root / (rec.content_id + "-2.lomcontent")
        kwargs = {
            "version": "1.2.3",
            "author": "墨绘",
            "license_name": "CC-BY-4.0",
        }
        first = export_content_pack(one, rec.content_id, **kwargs)
        export_content_pack(two, rec.content_id, **kwargs)
        self.assertEqual(one.read_bytes(), two.read_bytes())
        inspected = inspect_content_pack(one)
        self.assertEqual(inspected.content_id, rec.content_id)
        self.assertEqual(inspected.content_type, rec.type)
        self.assertEqual(inspected.version, "1.2.3")
        self.assertEqual(inspected.author, "墨绘")
        self.assertEqual(inspected.license, "CC-BY-4.0")
        self.assertEqual(len(inspected.package_sha256), 64)
        self.assertEqual(len(inspected.logical_content_hash), 64)
        self.assertEqual(inspected.collision_type, rec.type)
        content_registry.remove(rec.content_id)
        imported = import_content_pack(one)
        self.assertIsNone(imported.collision_type)
        restored = content_registry.get(rec.content_id)
        self.assertEqual(restored.type, rec.type)
        self.assertTrue((restored.folder / restored.main_file).is_file())
        self.assertEqual(content_pack_defaults(rec.content_id), {
            "version": "1.2.3", "author": "墨绘", "license": "CC-BY-4.0"
        })
        return one, restored

    def test_audio_character_and_image_roundtrip(self):
        audio = content_registry.register_audio(
            self._wav(), "mohui.voice", "语音", "sound"
        )
        self._roundtrip(audio)
        character = content_registry.register_character(
            {"normal": self._png("normal.png"), "happy": self._png("happy.png")},
            "mohui.hero",
            "侠客",
            title="少侠",
        )
        _package, restored = self._roundtrip(character)
        self.assertEqual(set(restored.portrait_ids()), {"normal", "happy"})
        image = content_registry.register_image(
            self._png("background.jpg"), "mohui.background", "背景"
        )
        self._roundtrip(image)

    def test_collision_is_detected_without_overwrite(self):
        rec = content_registry.register_audio(
            self._wav(), "mohui.same", "原内容", "sound"
        )
        package = self.root / "same.lomcontent"
        export_content_pack(
            package,
            rec.content_id,
            version="1.0.0",
            author="tester",
            license_name="MIT",
        )
        original = (rec.folder / rec.main_file).read_bytes()
        with self.assertRaises(ContentRegistryError) as cm:
            import_content_pack(package)
        self.assertIn("已存在", str(cm.exception))
        self.assertEqual((rec.folder / rec.main_file).read_bytes(), original)

    def test_tampered_file_or_unknown_entry_is_rejected(self):
        rec = content_registry.register_audio(
            self._wav(), "mohui.secure", "校验", "sound"
        )
        source = self.root / "source.lomcontent"
        export_content_pack(
            source,
            rec.content_id,
            version="1.0.0",
            author="tester",
            license_name="MIT",
        )
        tampered = self.root / "tampered.lomcontent"
        with zipfile.ZipFile(source) as reader, zipfile.ZipFile(tampered, "w") as writer:
            for info in reader.infolist():
                data = reader.read(info)
                if info.filename.startswith("files/"):
                    data += b"tamper"
                writer.writestr(info.filename, data)
            writer.writestr("undeclared.bin", b"x")
        with self.assertRaises(ContentRegistryError):
            inspect_content_pack(tampered)

        unsafe = self.root / "unsafe.lomcontent"
        with zipfile.ZipFile(unsafe, "w") as archive:
            archive.writestr("../evil", b"x")
        with self.assertRaises(ContentRegistryError):
            inspect_content_pack(unsafe)

    def test_failed_atomic_install_leaves_no_partial_record(self):
        rec = content_registry.register_image(
            self._png(), "mohui.atomic", "原子导入"
        )
        package = self.root / "atomic.lomcontent"
        export_content_pack(
            package,
            rec.content_id,
            version="1.0.0",
            author="tester",
            license_name="MIT",
        )
        content_registry.remove(rec.content_id)
        with mock.patch("content_pack.os.replace", side_effect=OSError("fault")):
            with self.assertRaises(ContentRegistryError):
                import_content_pack(package)
        with self.assertRaises(ContentRegistryError):
            content_registry.get(rec.content_id)

    def test_invalid_author_version_and_license_are_rejected(self):
        rec = content_registry.register_image(
            self._png(), "mohui.meta", "元数据"
        )
        for field, value in (
            ("version", "v1"),
            ("version", "1.0.0-01"),
            ("author", "bad\nname"),
            ("license_name", ""),
        ):
            values = {"version": "1.0.0", "author": "tester", "license_name": "MIT"}
            values[field] = value
            with self.subTest(field=field), self.assertRaises(ContentRegistryError):
                export_content_pack(
                    self.root / (field + ".lomcontent"), rec.content_id, **values
                )

    def test_content_library_exposes_offline_import_and_export(self):
        dialog = ContentLibraryDialog()
        buttons = [button.text() for button in dialog.findChildren(QPushButton)]
        self.assertTrue(any("导入内容包" in text for text in buttons), buttons)
        self.assertTrue(any("分享所选" in text for text in buttons), buttons)
        dialog.close()


if __name__ == "__main__":
    unittest.main()
