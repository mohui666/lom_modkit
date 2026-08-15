# -*- coding: utf-8 -*-
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from PySide6.QtWidgets import QApplication  # noqa: E402

import main  # noqa: E402
import models  # noqa: E402
from recovery_store import RecoverySession  # noqa: E402


class AutosaveIntegrationTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_dirty_timer_target_is_recovery_not_formal_story(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            formal = root / "project" / "main.json"
            formal.parent.mkdir()
            editor_data, fallback = models.load_editor_data(main.PROJECT_ROOT)
            win = main.MainWindow(editor_data, fallback)
            win._prompt_on_discard = False
            models.save_story(win.story, formal)
            before = formal.read_bytes()
            win.story_path = formal
            win._set_project_source("story", formal)
            win._recovery_session = RecoverySession(root / "recovery", "test")

            win.story["title"] = "仅存在于恢复副本"
            win._set_dirty(True)
            win._autosave_recovery()

            self.assertEqual(formal.read_bytes(), before)
            snapshot = win._recovery_session.snapshot_path
            payload = json.loads(snapshot.read_text(encoding="utf-8"))
            self.assertEqual(
                payload["stories"]["main"]["title"], "仅存在于恢复副本"
            )
            self.assertEqual(payload["source"]["path"], str(formal.resolve()))

            win._set_dirty(False)
            self.assertFalse(snapshot.exists())
            win.close()


if __name__ == "__main__":
    unittest.main()
