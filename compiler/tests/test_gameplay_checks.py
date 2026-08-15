# -*- coding: utf-8 -*-
import sys
import unittest
from pathlib import Path

COMPILER = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(COMPILER))

from lomc import LomcError, compile_story


def story(node):
    return {
        "id": "main", "start": node["id"],
        "nodes": [
            node,
            {"id": "ok", "type": "end"},
            {"id": "bad", "type": "end"},
        ],
    }


class GameplayCheckCompilerTest(unittest.TestCase):
    def test_emits_verified_read_only_checks(self):
        cases = [
            (
                {"id": "c", "type": "stat_check", "key": "mouth", "op": ">=", "value": 50,
                 "success": "ok", "failure": "bad"},
                'luamanager.GetStatData("mouth", 1) >= 50',
            ),
            (
                {"id": "c", "type": "affinity_check", "character": "brother4", "op": ">=",
                 "value": 20, "success": "ok", "failure": "bad"},
                'mod_affinity_value("brother4") >= 20',
            ),
            (
                {"id": "c", "type": "item_check", "category": "book", "item": "Book_A",
                 "success": "ok", "failure": "bad"},
                'mod_has_item("book", "Book_A")',
            ),
            (
                {"id": "c", "type": "talent_check", "talent": "Talent_A", "op": ">=",
                 "value": 1, "success": "ok", "failure": "bad"},
                'mod_talent_level("Talent_A") >= 1',
            ),
            (
                {"id": "c", "type": "flag_check", "source": "condition", "flag": "CP_A",
                 "invert": True, "success": "ok", "failure": "bad"},
                'not (checkpointmanager.Condition("CP_A"))',
            ),
            (
                {"id": "c", "type": "flag_check", "source": "flag_value", "flag": "F_A",
                 "op": "==", "value": 2, "success": "ok", "failure": "bad"},
                'tonumber(luamanager.GetFlagData("F_A")) == 2',
            ),
        ]
        for node, expression in cases:
            with self.subTest(node_type=node["type"]):
                lua = compile_story(story(node))
                self.assertIn("if " + expression + " then", lua)
                self.assertIn("return node_ok()", lua)
                self.assertIn("return node_bad()", lua)

    def test_rejects_unknown_targets_and_malformed_comparisons(self):
        bad_nodes = [
            {"id": "c", "type": "stat_check", "key": "mouth", "op": "~=", "value": 1,
             "success": "ok", "failure": "bad"},
            {"id": "c", "type": "affinity_check", "character": "brother4", "op": ">=",
             "value": 1.5, "success": "ok", "failure": "bad"},
            {"id": "c", "type": "item_check", "category": "consume", "item": "C",
             "success": "ok", "failure": "bad"},
            {"id": "c", "type": "talent_check", "talent": "T", "op": ">=", "value": 1,
             "success": "missing", "failure": "bad"},
            {"id": "c", "type": "flag_check", "source": "mod", "flag": "F", "op": ">=",
             "value": 1, "success": "ok", "failure": "bad"},
            {"id": "c", "type": "flag_check", "source": "flag_value", "flag": "F",
             "success": "ok", "failure": "bad"},
        ]
        for node in bad_nodes:
            with self.subTest(node=node), self.assertRaises(LomcError):
                compile_story(story(node))


if __name__ == "__main__":
    unittest.main(verbosity=2)
