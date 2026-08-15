# -*- coding: utf-8 -*-

from __future__ import annotations

import copy
import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
ROOT = Path(__file__).resolve().parents[2]
for path in (ROOT / "editor", ROOT / "compiler"):
    if str(path) not in sys.path:
        sys.path.insert(0, str(path))

from lomc.compiler import compile_story
from PySide6.QtWidgets import QApplication
from main import MainWindow
from models import FALLBACK_EDITOR_DATA
from story_sections import (
    add_structure, expand_for_node, get_sections, repair_after_delete,
    retarget_structure_ids, set_all_collapsed, set_collapsed, structure_rows,
)


def _story() -> dict:
    return {
        "id": "main", "start": "say1",
        "nodes": [
            {"id": "say1", "type": "say", "character": "player", "text": "1"},
            {"id": "say2", "type": "say", "character": "player", "text": "2"},
            {"id": "say3", "type": "say", "character": "player", "text": "3"},
            {"id": "say4", "type": "say", "character": "player", "text": "4"},
            {"id": "end1", "type": "end"},
        ],
    }


class StorySectionsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_section_group_tree_and_collapse(self):
        story = _story()
        section = add_structure(story, "Act One", 0, 3)
        group = add_structure(story, "Conversation", 1, 2, "group", section["id"])
        rows = structure_rows(story)
        self.assertEqual([row.kind for row in rows], ["section", "node", "group", "node", "node", "node", "node"])
        self.assertEqual([row.node_index for row in rows if row.kind == "node"], [0, 1, 2, 3, 4])
        set_collapsed(story, group["id"], True)
        self.assertEqual([row.node_index for row in structure_rows(story) if row.kind == "node"], [0, 3, 4])
        set_collapsed(story, section["id"], True)
        self.assertEqual([row.node_index for row in structure_rows(story) if row.kind == "node"], [4])
        self.assertTrue(expand_for_node(story, 2))
        self.assertEqual([row.node_index for row in structure_rows(story) if row.kind == "node"], [0, 1, 2, 3, 4])

    def test_overlapping_ranges_and_outside_group_are_rejected(self):
        story = _story()
        section = add_structure(story, "First", 0, 2)
        with self.assertRaisesRegex(ValueError, "重叠"):
            add_structure(story, "Overlap", 2, 4)
        with self.assertRaisesRegex(ValueError, "父 Section 内"):
            add_structure(story, "Outside", 1, 3, "group", section["id"])
        add_structure(story, "G1", 0, 1, "group", section["id"])
        with self.assertRaisesRegex(ValueError, "不能重叠"):
            add_structure(story, "G2", 1, 2, "group", section["id"])

    def test_editor_metadata_does_not_change_compiled_lua_or_cfg(self):
        plain = _story()
        grouped = copy.deepcopy(plain)
        section = add_structure(grouped, "Visual only", 0, 3)
        add_structure(grouped, "Nested", 1, 2, "group", section["id"])
        set_all_collapsed(grouped, True)
        self.assertEqual(compile_story(plain), compile_story(grouped))
        self.assertEqual(plain["nodes"], grouped["nodes"])
        self.assertEqual(plain["start"], grouped["start"])

    def test_rename_and_delete_repair_anchors_without_touching_flow(self):
        story = _story()
        section = add_structure(story, "Range", 0, 2)
        retarget_structure_ids(story, {"say1": "opening"})
        self.assertEqual(section["start"], "opening")
        story["nodes"][0]["id"] = "opening"
        old_nodes = list(story["nodes"])
        repair_after_delete(story, "opening", old_nodes)
        story["nodes"].pop(0)
        self.assertEqual(get_sections(story)[0]["start"], "say2")
        self.assertEqual([node["id"] for node in story["nodes"]], ["say2", "say3", "say4", "end1"])

    def test_single_node_structure_is_removed_with_node(self):
        story = _story()
        add_structure(story, "One", 1, 1)
        repair_after_delete(story, "say2", story["nodes"])
        story["nodes"].pop(1)
        self.assertEqual(get_sections(story), [])

    def test_main_navigation_renders_headers_and_collapsed_tree(self):
        story = _story()
        section = add_structure(story, "Act", 0, 3)
        add_structure(story, "Group", 1, 2, "group", section["id"])
        window = MainWindow(FALLBACK_EDITOR_DATA, True)
        window._prompt_on_discard = False
        window.story = story
        window._refresh_all(select_row=2)
        roles = [window.node_list.item(row).data(window._ROLE_KIND) for row in range(window.node_list.count())]
        self.assertTrue(any(isinstance(role, tuple) and role[1] == "section" for role in roles))
        self.assertTrue(any(isinstance(role, tuple) and role[1] == "group" for role in roles))
        self.assertEqual([role for role in roles if isinstance(role, int)], [0, 1, 2, 3, 4])
        window._toggle_structure(section["id"])
        collapsed_roles = [window.node_list.item(row).data(window._ROLE_KIND) for row in range(window.node_list.count())]
        self.assertEqual([role for role in collapsed_roles if isinstance(role, int)], [4])
        window.close()


if __name__ == "__main__":
    unittest.main()
