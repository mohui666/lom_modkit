# -*- coding: utf-8 -*-
from pathlib import Path
import sys
import tempfile
import unittest

EDITOR = Path(__file__).resolve().parent.parent
COMPILER = EDITOR.parent / "compiler"
sys.path[:0] = [str(EDITOR), str(COMPILER)]

import models
from preflight import run_preflight
from release_preflight import apply_release_profile


class ReleasePreflightTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.editor_data, _ = models.load_editor_data(EDITOR.parent)

    def _story(self):
        return {
            "id": "main", "title": "新剧情", "start": "show1", "mood": False,
            "localization": {
                "default_locale": "zh_CN", "fallback_locale": "zh_CN",
                "translations": {"ja": {"story.title": "物語"}},
            },
            "nodes": [
                {"id": "show1", "type": "show", "character": "player",
                 "position": "LB2", "fadeDuration": 0, "moveDuration": 0},
                {"id": "say1", "type": "say", "character": "player",
                 "text": "在这里填写第一句对白"},
                {"id": "end1", "type": "end"},
            ],
        }

    def test_release_adds_strict_checks_without_promoting_every_warning(self):
        stories = {"main": self._story()}
        manifest = {
            "id": "demo_mod", "name": "Demo", "version": "1.0.0",
            "author": "Author", "description": "Description", "entry": "main",
            "min_host_version": "9.0.0", "tested_host_version": "9.0.0",
        }
        with tempfile.TemporaryDirectory() as content_root:
            editing = run_preflight(
                stories, self.editor_data, "main", manifest=manifest,
                content_root=content_root,
            )
        release = apply_release_profile(
            editing, stories, manifest, "0.6.0",
            bundled_assets=["assets/unused.wav"],
        )
        by_code = {}
        for issue in release:
            by_code.setdefault(issue.code, []).append(issue)
        self.assertEqual(by_code["placeholder_text"][0].severity, "error")
        self.assertEqual(by_code["back_stage_position"][0].severity, "warning")
        self.assertEqual(by_code["incompatible_runtime_requirement"][0].severity, "error")
        self.assertEqual(len(by_code["missing_locale"]), 2)
        self.assertEqual(by_code["unused_critical_asset"][0].severity, "warning")

    def test_missing_metadata_is_release_error_only(self):
        stories = {"main": {"id": "main", "start": "end1",
                            "nodes": [{"id": "end1", "type": "end"}]}}
        with tempfile.TemporaryDirectory() as content_root:
            editing = run_preflight(
                stories, self.editor_data, "main", manifest={"entry": "main"},
                content_root=content_root,
            )
        self.assertFalse(any(i.code == "missing_release_metadata" for i in editing))
        release = apply_release_profile(editing, stories, {"entry": "main"}, "0.6.0")
        missing = [i for i in release if i.code == "missing_release_metadata"]
        self.assertEqual({i.message.split("manifest.")[1].split()[0] for i in missing},
                         {"id", "name", "version", "author", "description"})
        self.assertTrue(all(i.severity == "error" for i in missing))

    def test_public_package_version_must_be_semver(self):
        stories = {"main": {"id": "main", "start": "end1",
                            "nodes": [{"id": "end1", "type": "end"}]}}
        manifest = {
            "id": "demo", "name": "Demo", "version": "version one",
            "author": "Author", "description": "Description", "entry": "main",
        }
        with tempfile.TemporaryDirectory() as content_root:
            editing = run_preflight(
                stories, self.editor_data, "main", manifest=manifest,
                content_root=content_root,
            )
        release = apply_release_profile(editing, stories, manifest, "0.7.0")
        invalid = [i for i in release if i.code == "invalid_release_version"]
        self.assertEqual(len(invalid), 1)
        self.assertEqual(invalid[0].severity, "error")


if __name__ == "__main__":
    unittest.main()
