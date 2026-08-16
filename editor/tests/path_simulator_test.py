# -*- coding: utf-8 -*-

from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parents[1]
if str(EDITOR) not in sys.path:
    sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication
from path_simulator import PathSimulatorDialog, simulate_paths


def _codes(simulation):
    return [issue.code for issue in simulation.issues]


class PathSimulatorTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_unreachable_broken_target_and_no_exit_scc(self):
        stories = {
            "broken": {"id": "broken", "start": "b1", "nodes": [
                {"id": "b1", "type": "choice", "options": [{"text": "bad", "goto": "missing"}]},
                {"id": "end1", "type": "end"},
            ]},
            "loop": {"id": "loop", "start": "w1", "nodes": [
                {"id": "w1", "type": "wait", "seconds": 1, "goto": "w1"},
            ]},
        }
        simulation = simulate_paths(stories)
        self.assertIn("unreachable", _codes(simulation))
        self.assertIn("broken_target", _codes(simulation))
        self.assertIn("no_exit_scc", _codes(simulation))
        self.assertIn("missing_ending", _codes(simulation))

    def test_only_proven_condition_marks_dead_branch(self):
        proven = {"id": "proven", "start": "f1", "nodes": [
            {"id": "f1", "type": "flag", "flag": "READY"},
            {"id": "b1", "type": "branch", "source": "mod", "flag": "READY", "cases": [
                {"value": 1, "goto": "win"}, {"value": 2, "goto": "lose"},
            ]},
            {"id": "win", "type": "end"}, {"id": "lose", "type": "end"},
        ]}
        unknown = {"id": "unknown", "start": "b2", "nodes": [
            {"id": "b2", "type": "branch", "source": "stat", "stat": "mental", "cases": [
                {"op": ">=", "value": 50, "goto": "end2"},
            ]},
            {"id": "end2", "type": "end"},
        ]}
        simulation = simulate_paths({"proven": proven, "unknown": unknown})
        dead = [issue for issue in simulation.issues if issue.code == "dead_branch"]
        self.assertEqual(len(dead), 1)
        self.assertEqual((dead[0].story_id, dead[0].node_id), ("proven", "b1"))
        self.assertEqual(simulation.uncertain_conditions, 1)

    def test_cross_story_chain_finishes_but_cycle_has_missing_ending(self):
        stories = {
            "a": {"id": "a", "start": "e1", "nodes": [{"id": "e1", "type": "end", "next_script": "b"}]},
            "b": {"id": "b", "start": "e2", "nodes": [{"id": "e2", "type": "end"}]},
            "c": {"id": "c", "start": "e3", "nodes": [{"id": "e3", "type": "end", "next_script": "d"}]},
            "d": {"id": "d", "start": "e4", "nodes": [{"id": "e4", "type": "end", "next_script": "c"}]},
        }
        missing = {issue.story_id for issue in simulate_paths(stories).issues if issue.code == "missing_ending"}
        self.assertEqual(missing, {"c", "d"})

    def test_raw_path_is_unknown_not_falsely_missing_ending(self):
        stories = {"raw": {"id": "raw", "start": "r1", "nodes": [{"id": "r1", "type": "raw", "code": "return"}]}}
        simulation = simulate_paths(stories)
        self.assertNotIn("missing_ending", _codes(simulation))

    def test_malformed_cross_story_nodes_entry_and_trigger_are_reported(self):
        stories = {"main": {"id": "main", "start": "e1", "nodes": [
            {"id": "e1", "type": "end", "next_script": "missing"},
        ]}}
        manifest = {"entry": "bad_entry", "campaign": {"triggers": [{"script": "bad_trigger"}]}}
        issues = [issue for issue in simulate_paths(stories, manifest).issues if issue.code == "malformed_cross_story"]
        self.assertEqual(len(issues), 3)

    def test_dialog_renders_issue_rows(self):
        stories = {"main": {"id": "main", "start": "w1", "nodes": [{"id": "w1", "type": "wait", "seconds": 1, "goto": "w1"}]}}
        dialog = PathSimulatorDialog(stories, lambda *_: None)
        self.assertGreater(dialog.table.rowCount(), 0)
        dialog.close()


if __name__ == "__main__":
    unittest.main()
