# -*- coding: utf-8 -*-
import json
from pathlib import Path
import tempfile
import unittest

from recovery_store import RECOVERY_SCHEMA, RecoveryError, RecoverySession


class RecoveryStoreTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.formal = self.root / "project" / "main.json"
        self.formal.parent.mkdir()
        self.formal.write_bytes(b'{"formal":true}\n')

    def tearDown(self):
        self.temp.cleanup()

    def _write(self, session, text="draft"):
        return session.write_snapshot(
            stories={"main": {"id": "main", "title": text, "nodes": []}},
            current_story_id="main",
            manifest={"id": "demo_mod"},
            story_paths={"main": self.formal},
            source_kind="story",
            source_path=str(self.formal),
        )

    def test_autosave_is_atomic_separate_copy(self):
        before = self.formal.read_bytes()
        session = RecoverySession(self.root / "recovery", "test")
        snapshot = self._write(session)
        self.assertNotEqual(snapshot.resolve(), self.formal.resolve())
        self.assertEqual(self.formal.read_bytes(), before)
        payload = json.loads(snapshot.read_text(encoding="utf-8"))
        self.assertEqual(payload["recovery_schema"], RECOVERY_SCHEMA)
        self.assertEqual(payload["stories"]["main"]["title"], "draft")
        self.assertFalse(list(snapshot.parent.glob("*.tmp")))

        self._write(session, "newer")
        replaced = json.loads(snapshot.read_text(encoding="utf-8"))
        self.assertEqual(replaced["stories"]["main"]["title"], "newer")
        self.assertEqual(self.formal.read_bytes(), before)

    def test_clean_close_removes_snapshot_and_marks_session(self):
        session = RecoverySession(self.root / "recovery")
        snapshot = self._write(session)
        self.assertTrue(snapshot.is_file())
        session.mark_closed()
        self.assertFalse(snapshot.exists())
        marker = json.loads(session.marker_path.read_text(encoding="utf-8"))
        self.assertEqual(marker["status"], "closed")
        self.assertFalse(marker["has_snapshot"])

    def test_invalid_snapshot_never_replaces_existing_copy(self):
        session = RecoverySession(self.root / "recovery")
        snapshot = self._write(session)
        before = snapshot.read_bytes()
        with self.assertRaises(RecoveryError):
            session.write_snapshot(
                stories={}, current_story_id="main", manifest={}, story_paths={},
                source_kind="untitled", source_path=None,
            )
        self.assertEqual(snapshot.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
