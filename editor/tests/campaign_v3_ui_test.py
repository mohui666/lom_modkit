# -*- coding: utf-8 -*-
from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR))

from PySide6.QtCore import Qt  # noqa: E402
from PySide6.QtWidgets import QApplication, QComboBox  # noqa: E402

import main  # noqa: E402
import models  # noqa: E402
from project_templates import new_project_manifest  # noqa: E402
from table_layout import (  # noqa: E402
    MIN_COMBO_WIDTH,
    MIN_ROW_HEIGHT,
    ReadableTableWidget,
)


class CampaignV3EditorTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_new_project_gets_stable_id_but_imported_data_is_not_inferred(self):
        first = new_project_manifest()
        second = new_project_manifest()
        self.assertRegex(first["campaign_id"], r"^[a-z0-9_-]{1,64}$")
        self.assertNotEqual(first["campaign_id"], second["campaign_id"])
        self.assertEqual(first["campaign"], {"new_game": True})

        imported = main.ManifestDialog(
            "main",
            models.FALLBACK_EDITOR_DATA,
            ["main"],
            {"id": "legacy-name", "campaign": {"new_game": True}},
        )
        self.assertEqual(imported.campaign_id_edit.text(), "")
        self.assertEqual(imported.manifest()["campaign_id"], "")

    def test_manifest_round_trips_required_campaign_identity(self):
        dialog = main.ManifestDialog(
            "main",
            models.FALLBACK_EDITOR_DATA,
            ["main"],
            {
                "id": "package-name",
                "campaign_id": "story-campaign",
                "campaign": {"new_game": True},
            },
        )
        value = dialog.manifest()
        self.assertEqual(value["campaign_id"], "story-campaign")
        self.assertIs(value["campaign"]["new_game"], True)

    def test_table_combo_policy_prevents_text_and_arrow_clipping(self):
        table = ReadableTableWidget(1, 1)
        combo = QComboBox()
        long_label = "很长的官方人物名称 / long localized character label"
        combo.addItems(("短", long_label))
        combo.setCurrentText(long_label)
        table.setCellWidget(0, 0, combo)
        self.app.processEvents()

        self.assertGreaterEqual(combo.minimumWidth(), MIN_COMBO_WIDTH)
        self.assertGreaterEqual(table.columnWidth(0), combo.minimumWidth() + 8)
        self.assertGreaterEqual(table.rowHeight(0), MIN_ROW_HEIGHT)
        self.assertEqual(combo.toolTip(), long_label)
        self.assertEqual(
            combo.view().textElideMode(), Qt.TextElideMode.ElideNone
        )
        self.assertEqual(
            table.horizontalScrollBarPolicy(), Qt.ScrollBarPolicy.ScrollBarAsNeeded
        )


if __name__ == "__main__":
    unittest.main()
