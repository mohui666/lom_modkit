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
from condition_inspector import ConditionInspectorDialog, inspect_conditions


class ConditionInspectorTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_dominating_mod_flag_write_proves_true_and_lists_exact_targets(self):
        stories = {"main": {"id": "main", "start": "f1", "nodes": [
            {"id": "f1", "type": "flag", "flag": "READY"},
            {"id": "b1", "type": "branch", "source": "mod", "flag": "READY", "cases": [
                {"value": 1, "goto": "win"}, {"value": 2, "goto": "lose"},
            ]},
            {"id": "win", "type": "end"}, {"id": "lose", "type": "end"},
        ]}}
        report = inspect_conditions(stories)[0]
        self.assertEqual(report.proof, "always_true")
        self.assertEqual(report.flags, ("READY",))
        self.assertEqual(report.variables, ())
        self.assertEqual(report.targets, ("win", "lose"))
        self.assertIn("READY", report.summary)

    def test_bypass_path_keeps_truth_unknown_instead_of_guessing_false(self):
        stories = {"main": {"id": "main", "start": "c1", "nodes": [
            {"id": "c1", "type": "choice", "options": [
                {"text": "set", "goto": "f1"}, {"text": "skip", "goto": "b1"},
            ]},
            {"id": "f1", "type": "flag", "flag": "READY", "goto": "b1"},
            {"id": "b1", "type": "branch", "source": "mod", "flag": "READY", "cases": [
                {"value": 1, "goto": "end1"},
            ]},
            {"id": "end1", "type": "end"},
        ]}}
        report = inspect_conditions(stories)[0]
        self.assertEqual(report.proof, "unknown")
        self.assertIn("end1", report.targets)  # explicit true + sequential false fallback, deduplicated

    def test_stat_and_official_conditions_expose_symbols_but_remain_unknown(self):
        stories = {"main": {"id": "main", "start": "b1", "nodes": [
            {"id": "b1", "type": "branch", "source": "stat", "stat": "mental", "cases": [
                {"op": ">=", "value": 50, "goto": "b2"},
            ]},
            {"id": "b2", "type": "branch", "source": "condition", "flag": "COND_X", "cases": [
                {"value": 1, "goto": "end1"}, {"value": 2, "goto": "end1"},
            ]},
            {"id": "end1", "type": "end"},
        ]}}
        stat, condition = inspect_conditions(stories)
        self.assertEqual(stat.variables, ("mental",))
        self.assertEqual(stat.proof, "unknown")
        self.assertEqual(condition.flags, ("COND_X",))
        self.assertEqual(condition.proof, "unknown")

    def test_dialog_renders_reports_and_proof_tooltip(self):
        stories = {"main": {"id": "main", "start": "b1", "nodes": [
            {"id": "b1", "type": "branch", "source": "game", "flag": "CHECK", "cases": [{"value": 1, "goto": "end1"}]},
            {"id": "end1", "type": "end"},
        ]}}
        dialog = ConditionInspectorDialog(stories, lambda *_: None)
        self.assertEqual(dialog.table.rowCount(), 1)
        self.assertTrue(dialog.table.item(0, 5).toolTip())
        dialog.close()


if __name__ == "__main__":
    unittest.main()
