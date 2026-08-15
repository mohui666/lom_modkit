# -*- coding: utf-8 -*-
import os
from pathlib import Path
import sys
import tempfile
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication  # noqa: E402

import main  # noqa: E402
import models  # noqa: E402
from project_statistics import calculate_project_statistics  # noqa: E402


class ProjectStatisticsUiTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_simple_table_has_all_required_rows(self):
        story = {
            "id": "main", "start": "say1", "nodes": [
                {"id": "say1", "type": "say", "mode": "narrative", "text": "x"},
                {"id": "end1", "type": "end"},
            ]
        }
        dialog = main.ProjectStatisticsDialog(
            calculate_project_statistics({"main": story})
        )
        self.assertEqual(dialog.table.rowCount(), 11)
        labels = [dialog.table.item(row, 0).text() for row in range(11)]
        for required in ("剧情章节", "节点", "对白", "选项", "结尾", "人物",
                         "图片", "音频", "语音覆盖", "不可达节点", "未使用资产"):
            self.assertIn(required, labels)
        dialog.close()

    def test_main_window_scans_project_assets_without_reading_them(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "mod"
            story_path = root / "story" / "main.json"
            story_path.parent.mkdir(parents=True)
            story_path.write_text("{}", encoding="utf-8")
            (root / "assets" / "nested").mkdir(parents=True)
            (root / "assets" / "used.png").write_bytes(b"not decoded")
            (root / "assets" / "nested" / "unused.ogg").write_bytes(b"not decoded")
            editor_data, fallback = models.load_editor_data(main.PROJECT_ROOT)
            win = main.MainWindow(editor_data, fallback)
            win._prompt_on_discard = False
            win._set_project_source("story", story_path)
            self.assertEqual(
                sorted(win._project_bundled_assets()),
                ["assets/nested/unused.ogg", "assets/used.png"],
            )
            win.close()


if __name__ == "__main__":
    unittest.main()
