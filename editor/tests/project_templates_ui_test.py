# -*- coding: utf-8 -*-
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication, QDialog  # noqa: E402

import main  # noqa: E402
import models  # noqa: E402


class ProjectTemplatesUiTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_dialog_lists_five_templates_and_warns_for_placeholders(self):
        dialog = main.ProjectTemplateDialog()
        self.assertEqual(dialog.list.count(), 5)
        dialog.list.setCurrentRow(3)
        self.assertIn("user:template.*", dialog.description.text())
        dialog._accept_selected()
        self.assertEqual(dialog.template_key, "custom_character_showcase")

    def test_main_window_creates_dirty_editable_template_project(self):
        editor_data, fallback = models.load_editor_data(main.PROJECT_ROOT)
        win = main.MainWindow(editor_data, fallback)
        win._prompt_on_discard = False

        class FakeDialog:
            template_key = "branching_story"

            def __init__(self, _parent):
                pass

            def exec(self):
                return QDialog.DialogCode.Accepted

        with patch.object(main, "ProjectTemplateDialog", FakeDialog):
            win.new_story_from_template()
        self.assertEqual(win.story["title"], "分支剧情")
        self.assertEqual(win.story["nodes"][1]["type"], "choice")
        self.assertTrue(win._dirty)
        self.assertIsNone(win.story_path)
        win.close()


if __name__ == "__main__":
    unittest.main()
