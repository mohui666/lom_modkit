# -*- coding: utf-8 -*-
from __future__ import annotations
import os, sys, unittest
from pathlib import Path
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
EDITOR = Path(__file__).resolve().parents[1]
ROOT = EDITOR.parent
for path in (EDITOR, ROOT / "compiler"):
    if str(path) not in sys.path: sys.path.insert(0, str(path))
from PySide6.QtWidgets import QApplication
from story_localization import StoryLocalizationDialog, apply_localization_settings, normalized_localization, translation_coverage


class StoryLocalizationTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls): cls.app = QApplication.instance() or QApplication([])

    def story(self):
        return {"id": "main", "title": "标题", "start": "s1", "nodes": [
            {"id": "s1", "type": "say", "character": "0", "text": "你好"},
            {"id": "c1", "type": "choice", "options": [{"text": "走", "goto": "e1"}]},
            {"id": "e1", "type": "end"},
        ]}

    def test_old_story_is_not_mutated(self):
        story = self.story()
        config = normalized_localization(story)
        self.assertNotIn("localization", story)
        self.assertEqual(config["default_locale"], "chs")

    def test_apply_and_remove(self):
        story = self.story()
        apply_localization_settings(story, {"default_locale": "chs", "fallback_locale": "cht", "translations": {"ja": {"s1.text": "こんにちは"}}})
        self.assertEqual(story["localization"]["translations"]["ja"]["s1.text"], "こんにちは")
        apply_localization_settings(story, None)
        self.assertNotIn("localization", story)

    def test_dialog_edits_stable_path_catalog(self):
        story = self.story()
        dialog = StoryLocalizationDialog(story)
        dialog.locale_combo.setCurrentIndex(dialog.locale_combo.findData("ja"))
        row = next(i for i in range(dialog.table.rowCount()) if dialog.table.item(i, 0).text() == "s1.text")
        dialog.table.item(row, 2).setText("こんにちは")
        dialog._save()
        result = dialog.result_config()
        self.assertEqual(result["translations"]["ja"]["s1.text"], "こんにちは")
        self.assertEqual(dialog.table.rowCount(), 3)

    def test_stale_and_blank_entries_are_cleaned(self):
        story = self.story()
        story["localization"] = {"default_locale": "chs", "fallback_locale": "chs", "translations": {"ja": {"s1.text": " ", "gone.text": "x", "story.title": "題"}}}
        config = normalized_localization(story)
        self.assertEqual(config["translations"]["ja"], {"story.title": "題"})

    def test_changing_default_never_keeps_duplicate_catalog(self):
        dialog = StoryLocalizationDialog(self.story())
        dialog.locale_combo.setCurrentIndex(dialog.locale_combo.findData("ja"))
        dialog.table.item(0, 2).setText("題")
        dialog.default_combo.setCurrentIndex(dialog.default_combo.findData("ja"))
        dialog._save()
        self.assertNotIn("ja", dialog.result_config()["translations"])

    def test_coverage_is_exclusive_and_missing_filter_preserves_hidden_values(self):
        story = self.story()
        config = {"default_locale": "chs", "fallback_locale": "cht", "translations": {
            "cht": {"story.title": "標題"}, "ja": {"s1.text": "こんにちは"}
        }}
        coverage = translation_coverage(story, config, "ja")
        self.assertEqual((coverage["total"], coverage["translated"], coverage["fallback"], coverage["missing"]), (3, 1, 1, 1))
        dialog = StoryLocalizationDialog({**story, "localization": config})
        dialog.locale_combo.setCurrentIndex(dialog.locale_combo.findData("ja"))
        dialog.missing_only.setChecked(True)
        self.assertEqual(dialog.table.rowCount(), 1)
        dialog.table.item(0, 2).setText("進む")
        self.assertIn("1", dialog.coverage_label.text())
        dialog._save()
        result = dialog.result_config()["translations"]["ja"]
        self.assertEqual(result["s1.text"], "こんにちは")
        self.assertIn("c1.options.0.text", result)


if __name__ == "__main__": unittest.main()
