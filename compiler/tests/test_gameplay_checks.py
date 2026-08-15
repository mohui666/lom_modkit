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
    def test_mod_quest_and_state_check_use_host_owned_state_machine(self):
        lua = compile_story({
            "id": "main", "start": "start_quest",
            "nodes": [
                {"id": "start_quest", "type": "mod_quest", "quest": "find_student",
                 "op": "start", "message": "接受任务"},
                {"id": "check", "type": "quest_check", "quest": "find_student",
                 "state": "active", "success": "ok", "failure": "bad"},
                {"id": "ok", "type": "end"}, {"id": "bad", "type": "end"},
            ],
        })
        self.assertIn('mainui.DisplayMessageText("接受任务")', lua)
        self.assertIn('mod_quest_set("find_student", "start")', lua)
        self.assertIn('if mod_quest_state("find_student") == "active" then', lua)
        self.assertIn("return node_ok()", lua)
        self.assertIn("return node_bad()", lua)

    def test_mod_quest_rejects_bad_identity_operation_and_state(self):
        bad_nodes = [
            {"id": "c", "type": "mod_quest", "quest": "bad quest", "op": "start"},
            {"id": "c", "type": "mod_quest", "quest": "q", "op": "delete"},
            {"id": "c", "type": "quest_check", "quest": "q", "state": "done",
             "success": "ok", "failure": "bad"},
        ]
        for node in bad_nodes:
            with self.subTest(node=node), self.assertRaises(LomcError):
                compile_story(story(node))

    def test_activity_expands_to_existing_check_time_and_reward_apis(self):
        lua = compile_story(story({
            "id": "c", "type": "activity", "kind": "training",
            "message": "开始练武", "stat": "stamina", "op": ">=", "value": 30,
            "time": "round", "success": "ok", "failure": "bad",
            "success_rewards": [
                {"kind": "stat", "key": "stamina", "amount": 2},
                {"kind": "flag", "key": "trained"},
            ],
            "failure_rewards": [{"kind": "stat", "key": "stamina", "amount": -1}],
        }))
        self.assertIn('mainui.DisplayMessageText("开始练武")', lua)
        self.assertIn("luamanager.NextRound()", lua)
        self.assertIn('if luamanager.GetStatData("stamina", 1) >= 30 then', lua)
        self.assertIn('Player("stamina", 2, "", 1)', lua)
        self.assertIn('modflags["trained"] = true', lua)
        self.assertIn('Player("stamina", -1, "", 1)', lua)

    def test_activity_rejects_unknown_kind_and_bad_rewards(self):
        bad_nodes = [
            {"id": "c", "type": "activity", "kind": "fight", "stat": "s",
             "op": ">=", "value": 1, "success": "ok", "failure": "bad"},
            {"id": "c", "type": "activity", "kind": "study", "stat": "s",
             "op": ">=", "value": 1.5, "success": "ok", "failure": "bad"},
            {"id": "c", "type": "activity", "kind": "forge", "stat": "s",
             "op": ">=", "value": 1, "success": "ok", "failure": "bad",
             "success_rewards": [{"kind": "item", "category": "book", "key": "B", "amount": 0}]},
        ]
        for node in bad_nodes:
            with self.subTest(node=node), self.assertRaises(LomcError):
                compile_story(story(node))

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
