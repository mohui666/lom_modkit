# -*- coding: utf-8 -*-
import os
from pathlib import Path
import sys
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication  # noqa: E402

from preflight import PreflightIssue  # noqa: E402
from preflight_dialog import PreflightDialog  # noqa: E402


class ReleasePreflightUiTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_dialog_identifies_release_profile_and_preserves_severity(self):
        located = []
        issues = [
            PreflightIssue("error", "placeholder_text", "main", "say1", "占位"),
            PreflightIssue("warning", "missing_locale", "main", "", "缺语言"),
            PreflightIssue(
                "error", "missing_release_metadata", "", "", "缺发布资料"
            ),
        ]
        dialog = PreflightDialog(
            issues, located.append, lambda: ([], issues), profile="release"
        )
        self.assertIn("Release", dialog.windowTitle())
        self.assertIn("Release", dialog.summary.text())
        self.assertEqual(dialog.table.rowCount(), 3)
        self.assertEqual(dialog.issues[0].severity, "error")
        self.assertEqual(dialog.issues[1].severity, "warning")
        dialog.table.selectRow(2)
        self.assertTrue(dialog.locate_btn.isEnabled())
        self.assertNotEqual(dialog.table.item(2, 1).text(), "—")
        dialog._locate()
        self.assertEqual(located, [issues[2]])
        dialog.close()


if __name__ == "__main__":
    unittest.main()
