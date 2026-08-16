# -*- coding: utf-8 -*-
from __future__ import annotations

import copy
import json
import sys
import tempfile
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
        self.assertIn("missing_image", {issue.code for issue in issues})

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
        self.assertIn("invalid_cross_story_goto", codes)
        self.assertIn("unreachable_node", codes)
        self.assertIn("placeholder_text", codes)
        missing = next(issue for issue in issues if issue.code == "invalid_cross_story_goto")
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
        self.assertIn("no_exit_scc", codes)
        self.assertIn("unreachable_node", codes)


class PreflightV2Test(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def _content(self, ctype: str, cid: str, **changes) -> Path:
        folder = self.root / "assets" / "user" / ctype / cid
        folder.mkdir(parents=True)
        ext = ".wav" if ctype == "audio" else ".png"
        main = "main" + ext
        meta = {
            "schema": 1,
            "id": cid,
            "type": ctype,
            "name": cid,
            "files": {"main": main},
        }
        if ctype == "audio":
            meta["audio_kind"] = "sound"
        if ctype == "character":
            meta["portraits"] = {"normal": main}
        meta.update(changes)
        (folder / "content.json").write_text(
            json.dumps(meta, ensure_ascii=False), encoding="utf-8"
        )
        (folder / main).write_bytes(b"content")
        return folder

    @staticmethod
    def _story(nodes: list[dict], start: str = "n1") -> dict[str, dict]:
        return {"main": {"id": "main", "title": "v2", "start": start, "nodes": nodes}}

    def test_typed_content_missing_path_unused_and_stale_checks(self):
        self._content("audio", "test.wrong")
        self._content("audio", "test.unused")
        self._content("character", "test.hero")
        stale = self._content("image", "test.stale")
        raw = json.loads((stale / "content.json").read_text(encoding="utf-8"))
        raw["id"] = "test.other"
        (stale / "content.json").write_text(json.dumps(raw), encoding="utf-8")
        stories = self._story([
            {"id": "n1", "type": "background", "action": "show", "image": "user:test.wrong"},
            {"id": "n2", "type": "custom_cg", "action": "show", "image": "user:test.missing_image"},
            {"id": "n3", "type": "show", "character": "user:test.hero", "portrait": "happy", "position": "M"},
            {"id": "n4", "type": "say", "mode": "narrative", "text": "line", "voice": "user:test.missing_voice"},
            {"id": "n5", "type": "say", "mode": "narrative", "text": "bad", "voice": "C:/voice.wav"},
            {"id": "n6", "type": "say", "mode": "narrative", "text": "bad", "voice": "official_voice"},
            {"id": "n7", "type": "end"},
        ])
        issues = run_preflight(stories, EDITOR_DATA, "main", content_root=self.root)
        codes = {issue.code for issue in issues}
        self.assertTrue({
            "wrong_user_content_type",
            "missing_image",
            "missing_portrait",
            "missing_voice",
            "illegal_content_path",
            "impossible_content_reference",
            "unused_content",
            "stale_content_metadata",
        }.issubset(codes))
        self.assertTrue(all(not issue.fixable for issue in issues if issue.code in codes - {"stage_missing"}))

    def test_valid_referenced_content_has_no_v2_content_issue(self):
        self._content("image", "test.background")
        self._content("audio", "test.voice")
        self._content("character", "test.hero")
        stories = self._story([
            {"id": "n1", "type": "background", "action": "show", "image": "user:test.background"},
            {"id": "n2", "type": "show", "character": "user:test.hero", "portrait": "normal", "position": "M"},
            {"id": "n3", "type": "say", "mode": "character", "character": "user:test.hero", "portrait": "normal", "text": "line", "voice": "user:test.voice"},
            {"id": "n4", "type": "end"},
        ])
        issues = run_preflight(stories, EDITOR_DATA, "main", content_root=self.root)
        content_codes = {
            "wrong_user_content_type", "missing_image", "missing_portrait",
            "missing_voice", "illegal_content_path", "impossible_content_reference",
            "unused_content", "stale_content_metadata",
        }
        self.assertFalse(content_codes & {issue.code for issue in issues})

    def test_invalid_entry_cross_story_goto_and_trigger_are_errors(self):
        stories = self._story([
            {"id": "n1", "type": "end", "next_script": "missing"},
        ])
        issues = run_preflight(
            stories,
            EDITOR_DATA,
            "absent",
            manifest={
                "entry": "absent",
                "campaign": {"triggers": [{"script": "also_missing"}]},
            },
            content_root=self.root,
        )
        self.assertIn("invalid_entry", {issue.code for issue in issues})
        cross = [issue for issue in issues if issue.code == "invalid_cross_story_goto"]
        self.assertEqual(len(cross), 2)
        self.assertTrue(all(issue.severity == "error" for issue in cross))

    def test_cross_story_no_exit_scc_is_detected(self):
        stories = {
            "a": {"id": "a", "start": "a1", "nodes": [
                {"id": "a1", "type": "end", "next_script": "b"},
            ]},
            "b": {"id": "b", "start": "b1", "nodes": [
                {"id": "b1", "type": "end", "next_script": "a"},
            ]},
        }
        issues = run_preflight(stories, EDITOR_DATA, "a", content_root=self.root)
        loops = [issue for issue in issues if issue.code == "no_exit_scc"]
        self.assertEqual({issue.story_id for issue in loops}, {"a", "b"})

    def test_possible_read_before_write_is_located_and_not_auto_fixed(self):
        stories = self._story([
            {"id": "n1", "type": "choice", "options": [
                {"text": "write", "goto": "n2"},
                {"text": "read", "goto": "n3"},
            ]},
            {"id": "n2", "type": "flag", "flag": "ROUTE", "goto": "n3"},
            {"id": "n3", "type": "branch", "source": "mod", "flag": "ROUTE", "cases": [
                {"value": 1, "goto": "n4"}, {"value": 2, "goto": "n4"},
            ]},
            {"id": "n4", "type": "end"},
        ])
        before = copy.deepcopy(stories)
        issues = run_preflight(stories, EDITOR_DATA, "main", content_root=self.root)
        issue = next(item for item in issues if item.code == "possible_read_before_write")
        self.assertEqual((issue.story_id, issue.node_id, issue.fixable), ("main", "n3", False))
        apply_safe_fixes(stories, EDITOR_DATA)
        self.assertEqual(stories, before)

    def test_entry_chain_write_prevents_false_read_before_write_warning(self):
        stories = {
            "a": {"id": "a", "start": "a1", "nodes": [
                {"id": "a1", "type": "flag", "flag": "READY"},
                {"id": "a2", "type": "end", "next_script": "b"},
            ]},
            "b": {"id": "b", "start": "b1", "nodes": [
                {"id": "b1", "type": "branch", "source": "mod", "flag": "READY", "cases": [
                    {"value": 1, "goto": "b2"}, {"value": 2, "goto": "b2"},
                ]},
                {"id": "b2", "type": "end"},
            ]},
        }
        issues = run_preflight(
            stories, EDITOR_DATA, "a", manifest={"entry": "a"}, content_root=self.root
        )
        self.assertNotIn("possible_read_before_write", {issue.code for issue in issues})

    def test_external_trigger_keeps_read_before_write_warning(self):
        stories = {
            "a": {"id": "a", "start": "a1", "nodes": [
                {"id": "a1", "type": "flag", "flag": "READY"},
                {"id": "a2", "type": "end", "next_script": "b"},
            ]},
            "b": {"id": "b", "start": "b1", "nodes": [
                {"id": "b1", "type": "branch", "source": "mod", "flag": "READY", "cases": [
                    {"value": 1, "goto": "b2"}, {"value": 2, "goto": "b2"},
                ]},
                {"id": "b2", "type": "end"},
            ]},
        }
        issues = run_preflight(
            stories,
            EDITOR_DATA,
            "a",
            manifest={"entry": "a", "campaign": {"triggers": [{"script": "b"}]}},
            content_root=self.root,
        )
        self.assertIn("possible_read_before_write", {issue.code for issue in issues})


if __name__ == "__main__":
    unittest.main()
