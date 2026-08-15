# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtWidgets import QApplication, QComboBox  # noqa: E402

import main  # noqa: E402
import models  # noqa: E402


class CampaignTriggerEditorTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_typed_trigger_controls_round_trip_verified_manifest_shape(self):
        data = dict(models.FALLBACK_EDITOR_DATA)
        data["free_positions"] = [{"id": "Center", "name": "练功场"}]
        data["game_flags"] = [{"id": "FLAG_A", "name": "条件 A"}]
        data["affinity_characters"] = ["brother4"]
        dialog = main.ManifestDialog(
            "main", data, ["main", "event"],
            {"campaign": {"triggers": [{
                "type": "position", "position": "Center", "script": "event",
                "when_flag_set": "FLAG_A", "when_month": 12, "when_stage": 3,
                "when_affinity": {"character": "brother4", "min": 20},
            }]}},
        )
        table = dialog.triggers_table
        self.assertEqual(table.columnCount(), 8)
        for column in range(7):
            self.assertIsInstance(table.cellWidget(0, column), QComboBox)
        month = table.cellWidget(0, 4)
        stage = table.cellWidget(0, 5)
        self.assertEqual(month.currentData(), 12)
        self.assertEqual(stage.currentData(), 3)
        self.assertEqual(month.findData(13), -1)
        trigger = dialog.manifest()["campaign"]["triggers"][0]
        self.assertEqual(trigger["when_flag_set"], "FLAG_A")
        self.assertEqual(trigger["when_month"], 12)
        self.assertEqual(trigger["when_stage"], 3)
        self.assertEqual(trigger["when_affinity"], {"character": "brother4", "min": 20})

    def test_empty_typed_conditions_are_omitted(self):
        dialog = main.ManifestDialog(
            "main", models.FALLBACK_EDITOR_DATA, ["main"],
            {"campaign": {"triggers": [{
                "type": "position", "position": "Center", "script": "main",
            }]}},
        )
        trigger = dialog.manifest()["campaign"]["triggers"][0]
        self.assertEqual(set(trigger), {"type", "position", "script"})

    def test_unknown_but_valid_editable_ids_round_trip(self):
        dialog = main.ManifestDialog(
            "main", models.FALLBACK_EDITOR_DATA, ["main"],
            {"campaign": {"triggers": [{
                "type": "position", "position": "CustomPlace", "script": "main",
                "when_flag_set": "FUTURE_FLAG",
                "when_affinity": {"character": "future_character", "min": 7},
            }]}},
        )
        trigger = dialog.manifest()["campaign"]["triggers"][0]
        self.assertEqual(trigger["position"], "CustomPlace")
        self.assertEqual(trigger["when_flag_set"], "FUTURE_FLAG")
        self.assertEqual(
            trigger["when_affinity"], {"character": "future_character", "min": 7}
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
