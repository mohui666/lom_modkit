# -*- coding: utf-8 -*-
"""统一图片内容在用户库中的缩略图回归。"""

from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR_DIR = Path(__file__).resolve().parent.parent
COMPILER_DIR = EDITOR_DIR.parent / "compiler"
sys.path[:0] = [str(EDITOR_DIR), str(COMPILER_DIR)]

from PySide6.QtWidgets import QApplication  # noqa: E402

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


if __name__ == "__main__":
    unittest.main()
