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
from variable_manager import VariableManagerDialog, analyze_symbols


def _report(reports, kind, name):
    return next(report for report in reports if report.kind == kind and report.name == name)


class VariableManagerTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_counts_first_write_unused_and_external_namespaces(self):
        stories = {"main": {
            "id": "main", "start": "flag1", "nodes": [
                {"id": "flag1", "type": "flag", "flag": "READY"},
                {"id": "branch1", "type": "branch", "source": "mod", "flag": "READY", "cases": [{"value": 1, "goto": "flag2"}]},
                {"id": "flag2", "type": "flag", "flag": "UNUSED"},
                {"id": "gf1", "type": "game_flag", "flag": "OFFICIAL_X", "value": 1},
                {"id": "block1", "type": "block", "flowchart": "common", "name": "Show", "vars": [{"name": "ViewName", "value": "black"}]},
                {"id": "end1", "type": "end"},
            ],
        }}
        manifest = {"campaign": {"triggers": [{"script": "main", "when_flag_set": "OFFICIAL_X"}]}}
        reports = analyze_symbols(stories, manifest)
        ready = _report(reports, "mod_flag", "READY")
        self.assertEqual((ready.reads, ready.writes), (1, 1))
        self.assertEqual(ready.first_write.node_id, "flag1")
        self.assertFalse(ready.unused)
        self.assertFalse(ready.possibly_read_before_write)
        unused = _report(reports, "mod_flag", "UNUSED")
        self.assertTrue(unused.unused)
        official = _report(reports, "game_flag", "OFFICIAL_X")
        self.assertEqual((official.reads, official.writes), (1, 1))
        self.assertIsNone(official.unused)
        flow = _report(reports, "flow_variable", "ViewName")
        self.assertEqual((flow.reads, flow.writes), (0, 1))
        self.assertIsNone(flow.unused)

    def test_cfg_detects_possible_read_before_write_on_one_branch(self):
        stories = {"main": {
            "id": "main", "start": "choice1", "nodes": [
                {"id": "choice1", "type": "choice", "options": [
                    {"text": "write", "goto": "flag1"},
                    {"text": "read", "goto": "branch1"},
                ]},
                {"id": "flag1", "type": "flag", "flag": "ROUTE", "goto": "branch1"},
                {"id": "branch1", "type": "branch", "source": "mod", "flag": "ROUTE", "cases": [
                    {"value": 1, "goto": "end1"}, {"value": 2, "goto": "end1"},
                ]},
                {"id": "end1", "type": "end"},
            ],
        }}
        report = _report(analyze_symbols(stories), "mod_flag", "ROUTE")
        self.assertTrue(report.possibly_read_before_write)

    def test_unreachable_read_does_not_produce_false_before_write_warning(self):
        stories = {"main": {
            "id": "main", "start": "flag1", "nodes": [
                {"id": "flag1", "type": "flag", "flag": "SAFE", "goto": "end1"},
                {"id": "branch1", "type": "branch", "source": "mod", "flag": "SAFE", "cases": [{"value": 1, "goto": "end1"}]},
                {"id": "end1", "type": "end"},
            ],
        }}
        report = _report(analyze_symbols(stories), "mod_flag", "SAFE")
        self.assertFalse(report.possibly_read_before_write)

    def test_checkpoint_condition_and_flag_value_are_not_conflated(self):
        stories = {"main": {"id": "main", "start": "b1", "nodes": [
            {"id": "b1", "type": "branch", "source": "game", "flag": "SW", "cases": [{"value": 1, "goto": "b2"}]},
            {"id": "b2", "type": "branch", "source": "condition", "flag": "COND", "cases": [{"value": 1, "goto": "b3"}]},
            {"id": "b3", "type": "branch", "source": "flag_value", "flag": "NUM", "cases": [{"value": 1, "goto": "end1"}]},
            {"id": "end1", "type": "end"},
        ]}}
        reports = analyze_symbols(stories)
        self.assertEqual(_report(reports, "checkpoint", "SW").reads, 1)
        self.assertEqual(_report(reports, "condition", "COND").reads, 1)
        self.assertEqual(_report(reports, "game_flag", "NUM").reads, 1)

    def test_dialog_filters_reports(self):
        stories = {"main": {"id": "main", "start": "f1", "nodes": [
            {"id": "f1", "type": "flag", "flag": "A"}, {"id": "end1", "type": "end"},
        ]}}
        dialog = VariableManagerDialog(stories, lambda *_: None)
        self.assertEqual(dialog.table.rowCount(), 1)
        dialog.filter_combo.setCurrentIndex(dialog.filter_combo.findData("game_flag"))
        self.assertEqual(dialog.table.rowCount(), 0)
        dialog.close()


if __name__ == "__main__":
    unittest.main()
