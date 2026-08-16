# -*- coding: utf-8 -*-
from __future__ import annotations
import os, sys, unittest
from pathlib import Path
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parents[1]
if str(EDITOR) not in sys.path: sys.path.insert(0, str(EDITOR))
from story_test_runner import run_story_test, run_test_suite, set_story_tests, get_story_tests, validate_test_cases


class StoryTestRunnerTest(unittest.TestCase):
    def test_choice_flags_variables_and_assertions_pass(self):
        stories = {"main": {"id": "main", "start": "c1", "nodes": [
            {"id": "c1", "type": "choice", "options": [{"text": "go", "goto": "f1"}]},
            {"id": "f1", "type": "flag", "flag": "READY"},
            {"id": "s1", "type": "stat", "key": "mental", "delta": 2},
            {"id": "b1", "type": "branch", "source": "stat", "stat": "mental", "cases": [{"op": ">=", "value": 5, "goto": "end1"}]},
            {"id": "end1", "type": "end"},
        ]}}
        case = {"name": "route", "story": "main", "initial": {"variables": {"mental": 3}, "flags": {}}, "actions": {"choices": [{"node": "c1", "option": 0}]}, "assert": {"reaches_node": ["b1"], "reaches_ending": True, "variables": {"mental": 5}, "flags": {"READY": True}}}
        result = run_story_test(stories, case)
        self.assertEqual(result.status, "pass")
        self.assertEqual(result.variables["mental"], 5)
        self.assertTrue(result.flags["READY"])

    def test_failed_assertion_is_fail(self):
        stories = {"main": {"id": "main", "start": "e1", "nodes": [{"id": "e1", "type": "end"}]}}
        case = {"name": "bad", "story": "main", "assert": {"reaches_node": "missing"}}
        result = run_story_test(stories, case)
        self.assertEqual(result.status, "fail")
        self.assertIn("未到达", result.message)

    def test_raw_dice_missing_choice_and_unknown_stat_are_unsupported(self):
        types = {
            "raw": {"id": "n1", "type": "raw", "code": "return"},
            "dice": {"id": "n1", "type": "dice", "check": "x", "options": []},
            "choice": {"id": "n1", "type": "choice", "options": [{"text": "x", "goto": "e1"}]},
            "stat": {"id": "n1", "type": "stat", "key": "mental", "delta": 1},
        }
        for label, node in types.items():
            stories = {"main": {"id": "main", "start": "n1", "nodes": [node, {"id": "e1", "type": "end"}]}}
            result = run_story_test(stories, {"name": label, "story": "main"})
            self.assertEqual(result.status, "unsupported", label)

    def test_cross_story_end_chain_keeps_state(self):
        stories = {
            "a": {"id": "a", "start": "f1", "nodes": [{"id": "f1", "type": "flag", "flag": "A"}, {"id": "e1", "type": "end", "next_script": "b"}]},
            "b": {"id": "b", "start": "b1", "nodes": [{"id": "b1", "type": "branch", "source": "mod", "flag": "A", "cases": [{"value": 1, "goto": "e2"}]}, {"id": "e2", "type": "end"}]},
        }
        result = run_story_test(stories, {"name": "chain", "story": "a", "assert": {"reaches_ending": True, "flags": {"A": True}}})
        self.assertEqual(result.status, "pass")
        self.assertIn(("b", "b1"), result.visited)

    def test_editor_metadata_round_trip_and_validation(self):
        story = {"id": "main", "nodes": []}
        tests = [{"name": "one", "story": "main"}]
        set_story_tests(story, tests)
        self.assertEqual(get_story_tests(story), tests)
        with self.assertRaises(ValueError): validate_test_cases([tests[0], tests[0]])


if __name__ == "__main__": unittest.main()
