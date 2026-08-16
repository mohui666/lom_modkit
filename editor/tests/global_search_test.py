# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from PySide6.QtWidgets import QApplication  # noqa: E402

from global_search import GlobalSearchDialog, index_project, search_hits  # noqa: E402


STORIES = {
    "main": {
        "id": "main", "title": "序章", "start": "say1",
        "nodes": [
            {"id": "say1", "type": "say", "character": "user:mohui.hero",
             "portrait": "happy", "voice": "user:mohui.hello", "text": "月下相逢",
             "goto": "choice1"},
            {"id": "choice1", "type": "choice",
             "options": [{"text": "留下", "goto": "flag1"},
                         {"text": "离开", "goto": "end1"}]},
            {"id": "flag1", "type": "flag", "flag": "ROUTE_A", "value": 1,
             "goto": "end1"},
            {"id": "end1", "type": "end", "next_script": "side"},
        ],
    },
    "side": {
        "id": "side", "title": "支线", "start": "b1",
        "nodes": [
            {"id": "b1", "type": "background", "action": "show",
             "image": "user:mohui.moon", "goto": "vars1"},
            {"id": "vars1", "type": "block", "flowchart": "common", "name": "x",
             "vars": [{"name": "chapter_score", "value": "1"}], "goto": "end2"},
            {"id": "end2", "type": "end"},
        ],
    },
}


class GlobalSearchTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_indexes_required_story_fields(self):
        hits = index_project(STORIES)
        expected = {
            ("story", "main", None, "title", "序章"),
            ("node", "main", "say1", "id", "say1"),
            ("text", "main", "say1", "text", "月下相逢"),
            ("character", "main", "say1", "character", "user:mohui.hero"),
            ("portrait", "main", "say1", "portrait", "happy"),
            ("voice", "main", "say1", "voice", "user:mohui.hello"),
            ("image", "side", "b1", "image", "user:mohui.moon"),
            ("flag", "main", "flag1", "flag", "ROUTE_A"),
            ("variable", "side", "vars1", "vars[0].name", "chapter_score"),
            ("goto", "main", "choice1", "options[0].goto", "flag1"),
        }
        actual = {(h.category, h.story_id, h.node_id, h.field, h.value) for h in hits}
        self.assertTrue(expected.issubset(actual), expected - actual)

    def test_search_is_casefolded_multi_term_and_filterable(self):
        hits = index_project(STORIES)
        result = search_hits(hits, "MOHUI HERO")
        self.assertEqual([(h.story_id, h.node_id, h.field) for h in result],
                         [("main", "say1", "character")])
        self.assertEqual([h.value for h in search_hits(hits, "flag1", "goto")],
                         ["flag1", "end1"])

    def test_dialog_filters_and_locates(self):
        located = []
        dialog = GlobalSearchDialog(STORIES, lambda sid, nid: located.append((sid, nid)))
        dialog.query_edit.setText("月下相逢")
        self.assertEqual(dialog.table.rowCount(), 1)
        dialog._jump_selected()
        self.assertEqual(located, [("main", "say1")])


if __name__ == "__main__":
    unittest.main()
