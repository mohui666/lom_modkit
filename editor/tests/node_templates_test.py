# -*- coding: utf-8 -*-

from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parents[1]
if str(EDITOR) not in sys.path:
    sys.path.insert(0, str(EDITOR))

from node_templates import (
    add_template, create_template, instantiate_template, load_templates,
    save_templates,
)


class NodeTemplateTest(unittest.TestCase):
    def test_single_and_consecutive_nodes_get_fresh_ids(self):
        story = {"start": "say1", "nodes": [{"id": "say1", "type": "say", "text": "existing"}]}
        template = create_template("conversation", [
            {"id": "say1", "type": "say", "text": "hello", "goto": "choice1"},
            {"id": "choice1", "type": "choice", "options": [
                {"text": "inside", "goto": "say1"},
                {"text": "outside", "goto": "external_end"},
            ]},
        ])
        first, count, mapping = instantiate_template(story, template, 1)
        self.assertEqual((first, count), (1, 2))
        self.assertEqual(mapping, {"say1": "say2", "choice1": "choice1"})
        inserted = story["nodes"][1:]
        self.assertEqual(inserted[0]["id"], "say2")
        self.assertEqual(inserted[0]["goto"], "choice1")
        self.assertEqual(inserted[1]["options"][0]["goto"], "say2")
        self.assertEqual(inserted[1]["options"][1]["goto"], "external_end")
        self.assertEqual(story["start"], "say1")

    def test_nested_dice_and_branch_targets_are_remapped(self):
        story = {"nodes": []}
        template = create_template("flow", [
            {"id": "dice1", "type": "dice", "options": [{
                "goto_大成功": "end1", "goto_成功": "end1", "goto_失败": "outside"
            }]},
            {"id": "branch1", "type": "branch", "cases": [{"value": 1, "goto": "end1"}]},
            {"id": "end1", "type": "end"},
        ])
        instantiate_template(story, template, 0)
        ids = {node["type"]: node["id"] for node in story["nodes"]}
        self.assertEqual(story["nodes"][0]["options"][0]["goto_大成功"], ids["end"])
        self.assertEqual(story["nodes"][0]["options"][0]["goto_失败"], "outside")
        self.assertEqual(story["nodes"][1]["cases"][0]["goto"], ids["end"])

    def test_generated_id_that_matches_another_source_id_is_not_retargeted_twice(self):
        story = {"nodes": [{"id": "say1", "type": "say", "text": "existing"}]}
        template = create_template("two says", [
            {"id": "say1", "type": "say", "text": "one", "goto": "say2"},
            {"id": "say2", "type": "say", "text": "two"},
        ])
        instantiate_template(story, template, 1)
        self.assertEqual([node["id"] for node in story["nodes"]], ["say1", "say2", "say3"])
        self.assertEqual(story["nodes"][1]["goto"], "say3")

    def test_absolute_asset_path_is_rejected_but_dialogue_is_not(self):
        with self.assertRaisesRegex(ValueError, "绝对资源路径"):
            create_template("bad", [{"id": "cg1", "type": "custom_cg", "image": r"C:\\secret\\art.png"}])
        valid = create_template("text", [{"id": "say1", "type": "say", "text": "/home is dialogue"}])
        self.assertEqual(valid["nodes"][0]["text"], "/home is dialogue")

    def test_store_round_trip_and_never_overwrites_name(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "templates.json"
            first = create_template("My Template", [{"id": "end1", "type": "end"}])
            add_template(first, path)
            self.assertEqual(load_templates(path), [first])
            with self.assertRaisesRegex(ValueError, "未覆盖"):
                add_template(create_template("my template", [{"id": "end2", "type": "end"}]), path)
            self.assertEqual(load_templates(path), [first])
            save_templates([], path)
            self.assertEqual(load_templates(path), [])

    def test_input_template_is_not_mutated(self):
        nodes = [{"id": "say1", "type": "say", "text": "hello"}]
        template = create_template("copy", nodes)
        story = {"nodes": []}
        instantiate_template(story, template, 0)
        self.assertEqual(template["nodes"][0]["id"], "say1")
        self.assertEqual(nodes[0]["id"], "say1")


if __name__ == "__main__":
    unittest.main()
