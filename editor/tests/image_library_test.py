# -*- coding: utf-8 -*-
"""统一图片内容在用户库中的缩略图回归。"""

from __future__ import annotations

import os
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR_DIR = Path(__file__).resolve().parent.parent
COMPILER_DIR = EDITOR_DIR.parent / "compiler"
sys.path[:0] = [str(EDITOR_DIR), str(COMPILER_DIR)]

from PySide6.QtWidgets import QApplication  # noqa: E402
from PySide6.QtCore import Qt  # noqa: E402

import content_registry  # noqa: E402
from content_library_dialog import ContentLibraryDialog  # noqa: E402


class ImageLibraryTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

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

    def test_image_row_has_thumbnail(self):
        source = Path(self.temp.name) / "thumb.png"
        source.write_bytes(
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01"
            b"\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\xcf\xc0"
            b"\x00\x00\x00\x03\x00\x01\x00\x05\xfe\xd4\xef\x00\x00\x00\x00IEND\xaeB`\x82"
        )
        content_registry.register_image(source, "mohui.thumb", "缩略图")
        dialog = ContentLibraryDialog({})
        self.assertEqual(dialog.table.rowCount(), 1)
        self.assertFalse(dialog.table.item(0, 0).icon().isNull())
        self.assertEqual(dialog.table.item(0, 2).text(), "图片")
        dialog.close()

    def _png(self, name: str) -> Path:
        source = Path(self.temp.name) / name
        source.write_bytes(
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01"
            b"\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\xcf\xc0"
            b"\x00\x00\x00\x03\x00\x01\x00\x05\xfe\xd4\xef\x00\x00\x00\x00IEND\xaeB`\x82"
        )
        return source

    def test_content_browser_search_filter_metadata_usage_and_audio_preview(self):
        content_registry.register_image(self._png("moon.png"), "mohui.moon", "月夜")
        content_registry.register_image(self._png("unused.png"), "mohui.unused", "闲置图")
        wav = Path(self.temp.name) / "line.wav"
        wav.write_bytes(b"RIFF\x24\x00\x00\x00WAVEfmt " + b"\x00" * 32)
        content_registry.register_audio(wav, "mohui.line", "台词", "sound")
        stories = {
            "main": {"nodes": [{"id": "n1", "type": "background", "action": "show",
                                  "image": "user:mohui.moon"}]}
        }
        dialog = ContentLibraryDialog(stories)
        self.assertEqual(dialog.table.rowCount(), 3)

        dialog.search_edit.setText("moon")
        self.assertEqual(dialog.table.rowCount(), 1)
        self.assertEqual(dialog.table.item(0, 1).text(), "user:mohui.moon")
        self.assertEqual(dialog.table.item(0, 4).data(Qt.ItemDataRole.UserRole), 1)
        dialog.table.selectRow(0)
        self.assertIn("id=user:mohui.moon", dialog.metadata_view.toPlainText())

        dialog.search_edit.clear()
        dialog.type_filter.setCurrentIndex(dialog.type_filter.findData("image"))
        self.assertEqual(dialog.table.rowCount(), 2)
        usage = {dialog.table.item(row, 1).text(): dialog.table.item(row, 4).text()
                 for row in range(dialog.table.rowCount())}
        self.assertIn("未使用", usage["user:mohui.unused"])

        dialog.type_filter.setCurrentIndex(dialog.type_filter.findData("audio"))
        dialog.table.selectRow(0)
        with mock.patch("content_library_dialog.play_audio_file") as play:
            dialog._preview_selected()
            play.assert_called_once()
        dialog.close()

    def test_character_row_uses_normal_portrait_preview(self):
        content_registry.register_character(
            {"normal": self._png("hero.png")}, "mohui.hero", "侠客"
        )
        dialog = ContentLibraryDialog({})
        dialog.type_filter.setCurrentIndex(dialog.type_filter.findData("character"))
        self.assertEqual(dialog.table.rowCount(), 1)
        self.assertFalse(dialog.table.item(0, 0).icon().isNull())
        dialog.table.selectRow(0)
        self.assertIn("portraits=normal", dialog.metadata_view.toPlainText())
        dialog.close()


if __name__ == "__main__":
    unittest.main()
