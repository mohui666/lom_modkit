# -*- coding: utf-8 -*-
"""stage_guard 登场防线：线性判断、图级分析、自动补登场与入边重定向。"""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

import models  # noqa: E402
import stage_guard  # noqa: E402
import story_api  # noqa: E402
from preflight import apply_safe_fixes, run_preflight  # noqa: E402


def _node(node_id, node_type, **fields):
    return {"id": node_id, "type": node_type, **fields}


class RequiredCharacterTest(unittest.TestCase):
    def test_action_types_need_character(self):
        for t in ("move", "face", "hide", "focus", "offset", "shock", "dim", "rotate"):
            self.assertEqual(
                stage_guard.required_character(_node("n1", t, character="abc")),
                "abc",
                t,
            )

    def test_say_modes(self):
        self.assertEqual(
            stage_guard.required_character(
                _node("n1", "say", character="abc", mode="character")
            ),
            "abc",
        )
        self.assertEqual(
            stage_guard.required_character(
                _node("n1", "say", character="abc", mode="think")
            ),
            "abc",
        )
        for mode in ("narrative", "center"):
            self.assertIsNone(
                stage_guard.required_character(
                    _node("n1", "say", character="abc", mode=mode)
                ),
                mode,
            )

    def test_show_and_others_not_required(self):
        self.assertIsNone(stage_guard.required_character(_node("n1", "show", character="abc")))
        self.assertIsNone(stage_guard.required_character(_node("n1", "end")))
        self.assertIsNone(stage_guard.required_character(_node("n1", "say", mode="character")))


class LinearGuardTest(unittest.TestCase):
    def test_show_before_action_ok(self):
        nodes = [
            _node("n1", "show", character="abc"),
            _node("n2", "rotate", character="abc", angle=180, duration=1),
        ]
        self.assertIsNone(stage_guard.missing_stage_linear(nodes, 1))

    def test_no_show_or_hide_after_show(self):
        action = _node("n2", "rotate", character="abc", angle=180, duration=1)
        self.assertEqual(stage_guard.missing_stage_linear([action], 0), "abc")
        nodes = [
            _node("n1", "show", character="abc"),
            _node("n2", "hide", character="abc"),
            action,
        ]
        self.assertEqual(stage_guard.missing_stage_linear(nodes, 2), "abc")

    def test_latest_event_wins(self):
        nodes = [
            _node("n1", "hide", character="abc"),
            _node("n2", "show", character="abc"),
            _node("n3", "say", character="abc", mode="character", text="hi"),
        ]
        self.assertIsNone(stage_guard.missing_stage_linear(nodes, 2))

    def test_other_character_not_counted(self):
        nodes = [
            _node("n1", "show", character="xyz"),
            _node("n2", "say", character="abc", mode="character", text="hi"),
        ]
        self.assertEqual(stage_guard.missing_stage_linear(nodes, 1), "abc")


class GraphGuardTest(unittest.TestCase):
    def test_branch_join_missing_on_one_path(self):
        # n1 choice → n2(show abc)/n3(旁白) → 都汇聚到 n4(rotate abc)：n3 路径没登场
        story = {
            "id": "main",
            "start": "n1",
            "nodes": [
                _node("n1", "choice", options=[
                    {"text": "a", "goto": "n2"},
                    {"text": "b", "goto": "n3"},
                ]),
                _node("n2", "show", character="abc", goto="n4"),
                _node("n3", "say", mode="narrative", text="旁白", goto="n4"),
                _node("n4", "rotate", character="abc", angle=180, duration=1),
                _node("n5", "end"),
            ],
        }
        n4_goto = story["nodes"][3].setdefault("goto", "n5")
        self.assertEqual(n4_goto, "n5")
        issues = stage_guard.find_stage_issues(story)
        self.assertEqual(issues, [("n4", "abc")])

    def test_all_paths_shown_ok(self):
        story = {
            "id": "main",
            "start": "n1",
            "nodes": [
                _node("n1", "show", character="abc"),
                _node("n2", "say", character="abc", mode="character", text="hi"),
                _node("n3", "end"),
            ],
        }
        self.assertEqual(stage_guard.find_stage_issues(story), [])

    def test_unreachable_action_ignored(self):
        story = {
            "id": "main",
            "start": "n1",
            "nodes": [
                _node("n1", "say", mode="narrative", text="旁白", goto="n3"),
                _node("n2", "rotate", character="abc", angle=180, duration=1),
                _node("n3", "end"),
            ],
        }
        self.assertEqual(stage_guard.find_stage_issues(story), [])


class EnsureStageTest(unittest.TestCase):
    def test_insert_and_retarget(self):
        story = {
            "id": "main",
            "start": "n1",
            "nodes": [
                _node("n1", "choice", options=[
                    {"text": "a", "goto": "n3"},
                    {"text": "b", "goto": "n2"},
                ]),
                _node("n2", "say", mode="narrative", text="旁白", goto="n3"),
                _node("n3", "rotate", character="abc", angle=180, duration=1),
                _node("n4", "end"),
            ],
        }
        story["nodes"][2]["goto"] = "n4"
        show = stage_guard.ensure_stage(story, "n3")
        self.assertIsNotNone(show)
        self.assertEqual(show["type"], "show")
        self.assertEqual(show["character"], "abc")
        self.assertEqual(show["position"], "M")
        nodes = story["nodes"]
        # 插在 n3 正前面
        self.assertEqual([n["id"] for n in nodes], ["n1", "n2", show["id"], "n3", "n4"])
        # 显式入边全部改指新 show
        self.assertEqual(nodes[0]["options"][0]["goto"], show["id"])
        self.assertEqual(nodes[1]["goto"], show["id"])
        # 修复后图级分析不再有登场问题
        self.assertEqual(stage_guard.find_stage_issues(story), [])

    def test_start_retarget(self):
        story = {
            "id": "main",
            "start": "n1",
            "nodes": [
                _node("n1", "say", character="abc", mode="character", text="hi"),
                _node("n2", "end"),
            ],
        }
        show = stage_guard.ensure_stage(story, "n1")
        self.assertEqual(story["start"], show["id"])
        self.assertEqual([n["id"] for n in story["nodes"]], [show["id"], "n1", "n2"])

    def test_noop_for_non_action(self):
        story = {"id": "main", "start": "n1", "nodes": [_node("n1", "end")]}
        self.assertIsNone(stage_guard.ensure_stage(story, "n1"))
        self.assertIsNone(stage_guard.ensure_stage(story, "nX"))


class StoryApiGuardTest(unittest.TestCase):
    def test_new_story_has_entrance(self):
        story = story_api.new_story()
        self.assertEqual([n["type"] for n in story["nodes"]], ["show", "say"])
        self.assertEqual(story["start"], story["nodes"][0]["id"])
        self.assertEqual(story["nodes"][0].get("position"), "M")

    def test_add_say_auto_insert_after_hide(self):
        story = story_api.new_story()
        cid = story["nodes"][0]["character"]
        story_api.add_node(story, "hide", {"character": cid})
        story_api.add_say(story, "又见面了", character=cid)
        types = [n["type"] for n in story["nodes"]]
        # show, say, hide, 自动补的 show, say
        self.assertEqual(types, ["show", "say", "hide", "show", "say"])

    def test_add_say_no_insert_when_on_stage(self):
        story = story_api.new_story()
        cid = story["nodes"][0]["character"]
        story_api.add_say(story, "第二句", character=cid)
        self.assertEqual([n["type"] for n in story["nodes"]], ["show", "say", "say"])

    def test_update_node_auto_insert_and_retarget(self):
        story = story_api.new_story()
        say_id = story["nodes"][1]["id"]
        story_api.update_node(story, say_id, {"goto": "nX"})  # 占位不影响
        story_api.update_node(story, say_id, {"character": "someone_else"})
        nodes = story["nodes"]
        self.assertEqual(nodes[1]["type"], "show")
        self.assertEqual(nodes[1]["character"], "someone_else")
        # 起始节点是原 show(n1)，未受影响；say 仍在最后
        self.assertEqual(nodes[-1]["character"], "someone_else")


class PreflightGuardTest(unittest.TestCase):
    def test_stage_missing_issue_and_fix(self):
        story = {
            "id": "main",
            "title": "测试",
            "start": "n1",
            "nodes": [
                _node("n1", "say", character="abc", mode="character", text="hi"),
                _node("n2", "end"),
            ],
        }
        stories = {"main": story}
        ed = story_api._get_ed()
        issues = run_preflight(stories, ed)
        stage_issues = [i for i in issues if i.code == "stage_missing"]
        self.assertEqual(len(stage_issues), 1)
        self.assertTrue(stage_issues[0].fixable)
        fixes = apply_safe_fixes(stories, ed)
        self.assertTrue(any("登场" in f for f in fixes))
        self.assertEqual(stage_guard.find_stage_issues(story), [])
        self.assertEqual(story["nodes"][0]["type"], "show")
        self.assertEqual(story["start"], story["nodes"][0]["id"])


if __name__ == "__main__":
    unittest.main()
