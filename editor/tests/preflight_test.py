# -*- coding: utf-8 -*-
from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path

EDITOR_DIR = Path(__file__).resolve().parent.parent
PROJECT_ROOT = EDITOR_DIR.parent
sys.path.insert(0, str(EDITOR_DIR))
sys.path.insert(0, str(PROJECT_ROOT / "compiler"))

from preflight import apply_safe_fixes, run_preflight  # noqa: E402


EDITOR_DATA = {
    "schema": 3,
    "characters": [{"id": "player", "name": "主角", "portraits": ["normal"]}],
    "dice_checks": [],
    "dice_meta": {},
}


class PreflightTest(unittest.TestCase):
    def test_missing_user_background_is_reported(self):
        stories = {
            "main": {
                "id": "main",
                "title": "背景",
                "start": "n1",
                "nodes": [
                    {
                        "id": "n1",
                        "type": "background",
                        "action": "show",
                        "image": "user:missing.background_image",
                    },
                    {"id": "n2", "type": "end"},
                ],
            }
        }
        issues = run_preflight(stories, EDITOR_DATA, "main")
        self.assertIn("missing_user_content", {issue.code for issue in issues})

    def test_finds_unreachable_placeholder_and_missing_story(self):
        stories = {
            "main": {
                "id": "main",
                "title": "测试",
                "start": "n1",
                "nodes": [
                    {"id": "n1", "type": "end", "next_script": "missing"},
                    {"id": "n2", "type": "say", "text": "在这里填写对白。", "mode": "narrative"},
                    {"id": "n3", "type": "end"},
                ],
            }
        }
        issues = run_preflight(stories, EDITOR_DATA, "main")
        codes = {issue.code for issue in issues}
        self.assertIn("missing_story", codes)
        self.assertIn("unreachable_node", codes)
        self.assertIn("placeholder_text", codes)
        missing = next(issue for issue in issues if issue.code == "missing_story")
        self.assertEqual((missing.story_id, missing.node_id), ("main", "n1"))

    def test_safe_fixes_are_conservative_and_undoable_by_snapshot(self):
        stories = {
            "main": {
                "id": "main",
                "title": "测试",
                "start": "lost",
                "nodes": [
                    {
                        "id": "n1",
                        "type": "say",
                        "text": "正式对白",
                        "mode": "character",
                        "character": "主角（player）",
                        "portrait": "normal",
                        "goto": "",
                    },
                    {"id": "n2", "type": "end", "next_script": ""},
                ],
            }
        }
        before = copy.deepcopy(stories)
        fixes = apply_safe_fixes(stories, EDITOR_DATA)
        self.assertTrue(fixes)
        self.assertEqual(stories["main"]["start"], "n1")
        self.assertEqual(stories["main"]["nodes"][0]["character"], "player")
        self.assertNotIn("goto", stories["main"]["nodes"][0])
        self.assertNotIn("next_script", stories["main"]["nodes"][1])
        self.assertEqual(before["main"]["nodes"][0]["text"], "正式对白")

    def test_warns_about_static_back_stage_entrance(self):
        stories = {
            "main": {
                "id": "main",
                "title": "测试",
                "start": "n1",
                "nodes": [
                    {
                        "id": "n1",
                        "type": "show",
                        "character": "player",
                        "position": "LB2",
                        "portrait": "normal",
                        "facing": "right",
                        "fadeDuration": 0,
                        "moveDuration": 0,
                    },
                    {"id": "n2", "type": "end"},
                ],
            }
        }
        issues = run_preflight(stories, EDITOR_DATA, "main")
        self.assertIn("back_stage_position", {issue.code for issue in issues})

    def test_warns_about_broken_flow_and_infinite_loop(self):
        stories = {
            "main": {
                "id": "main",
                "title": "循环",
                "start": "n1",
                "nodes": [
                    {"id": "n1", "type": "say", "mode": "narrative", "text": "一", "goto": "n2"},
                    {"id": "n2", "type": "say", "mode": "narrative", "text": "二", "goto": "n1"},
                    {"id": "n3", "type": "say", "mode": "narrative", "text": "孤立"},
                ],
            }
        }
        codes = {issue.code for issue in run_preflight(stories, EDITOR_DATA, "main")}
        self.assertIn("infinite_loop", codes)
        self.assertIn("unreachable_node", codes)


if __name__ == "__main__":
    unittest.main()
