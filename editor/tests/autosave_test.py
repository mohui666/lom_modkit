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
from recovery_store import RecoverySession, list_recovery_candidates  # noqa: E402


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

    def test_restore_loads_memory_without_formal_overwrite_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            formal = root / "project" / "main.json"
            formal.parent.mkdir()
            formal.write_bytes(b'{"formal":"untouched"}\n')
            old = RecoverySession(root / "recovery", "old")
            old.write_snapshot(
                stories={
                    "main": {
                        "id": "main", "title": "crash draft", "start": "end1",
                        "nodes": [{"id": "end1", "type": "end"}],
                    }
                },
                current_story_id="main",
                manifest={"id": "demo_mod", "name": "Recovered"},
                story_paths={"main": formal},
                source_kind="story",
                source_path=str(formal),
            )
            candidate = list_recovery_candidates(
                old.root, include_live=True
            )[0]

            editor_data, fallback = models.load_editor_data(main.PROJECT_ROOT)
            win = main.MainWindow(editor_data, fallback)
            win._prompt_on_discard = False
            dialog = main.RecoveryDialog([candidate], win)
            self.assertEqual(dialog.table.rowCount(), 1)
            self.assertIs(dialog._selected(), candidate)
            dialog.close()
            win._recovery_session = RecoverySession(root / "recovery", "new")
            self.assertTrue(win._restore_recovery_candidate(candidate))
            self.assertEqual(win.story["title"], "crash draft")
            self.assertTrue(win._dirty)
            self.assertIsNone(win.story_path)
            self.assertEqual(formal.read_bytes(), b'{"formal":"untouched"}\n')
            self.assertTrue(win._recovery_session.snapshot_path.is_file())
            old_marker = json.loads(old.marker_path.read_text(encoding="utf-8"))
            self.assertEqual(old_marker["status"], "recovered")
            win.close()


if __name__ == "__main__":
    unittest.main()
