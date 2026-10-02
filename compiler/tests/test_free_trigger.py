# -*- coding: utf-8 -*-
"""自由模式触发节点（free_trigger）：登记、汇总、校验与幂等性。

用法（在 compiler/ 目录下）：
    ../editor/.venv/Scripts/python -m unittest tests.test_free_trigger -v

覆盖的关键约定：
1. 节点只是「登记」，放在 end 之后不能被误判成「末节点无法结束」；
2. 打包时被汇总进 manifest.campaign.triggers，且走的是同一套清单校验；
3. 字段非法（位置 id / 月份 / 脚本 id）必须报错，而不是静默丢弃；
4. 反复「导入自己打好的包 → 再导出」不会让触发器越滚越多。
"""

from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from lomc import LomcError, compile_story, validate_story  # noqa: E402
from lomc.pack import free_trigger_from_node, pack_mod  # noqa: E402
from lomc.schema_versions import (  # noqa: E402
    CONTENT_SCHEMA,
    PACKAGE_FORMAT,
    STORY_SCHEMA,
)


def manifest(**changes):
    value = {
        "format": PACKAGE_FORMAT,
        "package_format": PACKAGE_FORMAT,
        "story_schema": STORY_SCHEMA,
        "content_schema": CONTENT_SCHEMA,
        "id": "free-trigger-package",
        "campaign_id": "free-trigger-campaign",
        "name": "Free trigger package",
        "version": "1.0.0",
        "author": "tests",
        "description": "free_trigger contract",
        "entry": "main",
        "campaign": {"new_game": True},
    }
    value.update(changes)
    return value


def trigger_node(node_id="ft1", **changes):
    node = {
        "id": node_id,
        "type": "free_trigger",
        "position": "Center",
        "script": "next_story",
        "when_month": "any",
        "when_stage": "any",
        "when_flag_set": "",
        "when_flag_clear": "",
        "when_affinity": "",
        "when_affinity_min": 0,
        "note": "",
    }
    node.update(changes)
    return node


class FreeTriggerStoryTest(unittest.TestCase):
    def test_declaration_after_end_is_not_a_dangling_tail(self):
        """end 之后放一条登记，不能被判成「末节点无法结束」。"""
        story = {
            "story_schema": STORY_SCHEMA,
            "id": "main",
            "start": "n1",
            "nodes": [
                {"id": "n1", "type": "say", "mode": "narrative", "text": "结束前"},
                {"id": "n2", "type": "end"},
                trigger_node(),
            ],
        }
        validate_story(story)  # 不抛异常即通过
        lua = compile_story(story)
        self.assertIn("自由模式触发（声明型节点", lua)
        self.assertIn('luamanager.ChangeScene("Free", "", "")', lua)

    def test_still_complains_when_no_flow_node_terminates(self):
        """真正没有收尾节点时，仍然要报错——声明型节点不能把问题掩盖掉。"""
        story = {
            "story_schema": STORY_SCHEMA,
            "id": "main",
            "start": "n1",
            "nodes": [
                {"id": "n1", "type": "say", "mode": "narrative", "text": "没有收尾"},
                trigger_node(),
            ],
        }
        with self.assertRaises(LomcError):
            validate_story(story)

    def test_mid_story_declaration_falls_through(self):
        """放在剧情中间时仍要顺延到下一节点，不能把链断在这里。"""
        story = {
            "story_schema": STORY_SCHEMA,
            "id": "main",
            "start": "n1",
            "nodes": [
                {"id": "n1", "type": "say", "mode": "narrative", "text": "前"},
                trigger_node("ft1"),
                {"id": "n3", "type": "end"},
            ],
        }
        lua = compile_story(story)
        self.assertIn("return node_n3()", lua)

    def test_bad_position_is_rejected(self):
        for bad in ("NotAPlace", "", None):
            node = trigger_node(position=bad)
            story = {
                "story_schema": STORY_SCHEMA,
                "id": "main",
                "start": "n1",
                "nodes": [{"id": "n1", "type": "end"}, node],
            }
            with self.subTest(position=bad), self.assertRaises(LomcError):
                validate_story(story)

    def test_bad_month_is_rejected(self):
        for bad in ("0", "13", "x"):
            node = trigger_node(when_month=bad)
            story = {
                "story_schema": STORY_SCHEMA,
                "id": "main",
                "start": "n1",
                "nodes": [{"id": "n1", "type": "end"}, node],
            }
            with self.subTest(month=bad), self.assertRaises(LomcError):
                validate_story(story)


class FreeTriggerConversionTest(unittest.TestCase):
    def test_any_conditions_are_dropped(self):
        trigger = free_trigger_from_node(trigger_node())
        self.assertEqual(
            trigger,
            {"type": "position", "position": "Center", "script": "next_story"},
            f"未设置的条件不该出现在触发器里：{trigger}",
        )

    def test_all_conditions_are_carried_over(self):
        node = trigger_node(
            when_month="7",
            when_stage="3",
            when_flag_set="MOD_X_DONE",
            when_affinity="brother4",
            when_affinity_min=5,
        )
        trigger = free_trigger_from_node(node)
        self.assertEqual(trigger["when_month"], 7)
        self.assertEqual(trigger["when_stage"], 3)
        self.assertEqual(trigger["when_flag_set"], "MOD_X_DONE")
        self.assertEqual(trigger["when_affinity"], {"character": "brother4", "min": 5})

    def test_incomplete_node_is_skipped_defensively(self):
        # 字段兜底：交给 validate_story 报错，这里只保证不产出非法项
        self.assertIsNone(free_trigger_from_node(trigger_node(script="")))
        self.assertIsNone(free_trigger_from_node(trigger_node(position="")))
        self.assertIsNone(free_trigger_from_node(trigger_node(when_month="oops")))
        self.assertIsNone(free_trigger_from_node({"type": "say"}))


class FreeTriggerPackTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.mod_dir = os.path.join(self.tmp.name, "mod")
        os.makedirs(os.path.join(self.mod_dir, "story"))
        self.write_manifest(manifest())
        self.write_story("main", {
            "story_schema": STORY_SCHEMA,
            "id": "main",
            "title": "入口",
            "start": "n1",
            "nodes": [
                {"id": "n1", "type": "say", "mode": "narrative", "text": "结束前"},
                {"id": "n2", "type": "end"},
                trigger_node("ft1", when_month="7", when_stage="2"),
            ],
        })
        self.write_story("next_story", {
            "story_schema": STORY_SCHEMA,
            "id": "next_story",
            "title": "被触发的剧情",
            "start": "m1",
            "nodes": [{"id": "m1", "type": "end"}],
        })

    def tearDown(self):
        self.tmp.cleanup()

    def write_manifest(self, value):
        path = os.path.join(self.mod_dir, "manifest.json")
        Path(path).write_text(
            json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        return path

    def write_story(self, name, value):
        path = os.path.join(self.mod_dir, "story", "%s.json" % name)
        Path(path).write_text(
            json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        return path

    @staticmethod
    def packed_manifest(package):
        with zipfile.ZipFile(package) as archive:
            return json.loads(archive.read("manifest.json").decode("utf-8"))

    def test_pack_merges_node_trigger_into_manifest(self):
        out = os.path.join(self.tmp.name, "out.lommod")
        pack_mod(self.mod_dir, out)
        triggers = self.packed_manifest(out)["campaign"]["triggers"]
        self.assertEqual(len(triggers), 1, f"应汇总 1 条触发器：{triggers}")
        self.assertEqual(triggers[0]["position"], "Center")
        self.assertEqual(triggers[0]["script"], "next_story")
        self.assertEqual(triggers[0]["when_month"], 7)
        self.assertEqual(triggers[0]["when_stage"], 2)

    def test_repeated_pack_does_not_duplicate(self):
        """把打好的包再当作源目录打一次，触发器数量不能翻倍。"""
        first = os.path.join(self.tmp.name, "first.lommod")
        pack_mod(self.mod_dir, first)
        self.write_manifest(self.packed_manifest(first))  # 模拟「重新导入自己打好的包」
        second = os.path.join(self.tmp.name, "second.lommod")
        pack_mod(self.mod_dir, second)
        triggers = self.packed_manifest(second)["campaign"]["triggers"]
        self.assertEqual(len(triggers), 1, f"重复打包后触发器变多了：{triggers}")

    def test_hand_written_manifest_triggers_keep_priority(self):
        value = manifest()
        value["campaign"]["triggers"] = [
            {"type": "position", "position": "Center", "script": "main"}
        ]
        self.write_manifest(value)
        out = os.path.join(self.tmp.name, "priority.lommod")
        pack_mod(self.mod_dir, out)
        triggers = self.packed_manifest(out)["campaign"]["triggers"]
        self.assertEqual(
            [t["script"] for t in triggers],
            ["main", "next_story"],
            f"清单里手写的应排在前面：{triggers}",
        )

    def test_trigger_pointing_to_missing_script_is_rejected(self):
        self.write_story("main", {
            "story_schema": STORY_SCHEMA,
            "id": "main",
            "title": "入口",
            "start": "n1",
            "nodes": [
                {"id": "n1", "type": "end"},
                trigger_node("ft1", script="not_there"),
            ],
        })
        with self.assertRaisesRegex(LomcError, "not_there"):
            pack_mod(self.mod_dir, os.path.join(self.tmp.name, "bad.lommod"))


if __name__ == "__main__":
    unittest.main()
