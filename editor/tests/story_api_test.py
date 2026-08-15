# -*- coding: utf-8 -*-
"""story_api 契约测试（无 GUI / 无 PySide6 依赖）。

用法（在 editor/ 目录下）：
    .venv/Scripts/python tests/story_api_test.py

覆盖 docs/zh_CN/mod_format.md §7「AI 工具接口」契约的全部函数：
new_story / add_node / update_node / get_node / list_nodes / delete_node /
move_node / set_start / add_choice / add_dice / add_say / add_scene /
check_story / compile_story / load_story_json / save_story_json / pack_mod。

设计原则（契约）：AI/调用方不直接手写 story JSON 或 Lua，一切写操作走
story_api（models 契约默认值 + lomc 校验/警告），防止骰子菜单崩溃、
transition 黑幕、choice 皮肤崩溃、背景黑屏等已知坑。
"""

from __future__ import annotations

import copy
import json
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))
sys.path.insert(0, str(EDITOR_DIR.parent))  # compiler/lomc 与 data/ 在项目根

import story_api  # noqa: E402  # type: ignore[reportMissingImports]
import models  # noqa: E402

PROJECT_ROOT = EDITOR_DIR.parent
DEMO_STORY = PROJECT_ROOT / "samples" / "demo_mod" / "story" / "main.json"

# 契约 §3.1 全量 63 种节点类型（与 models.NODE_TYPES 一一对应）
ALL_TYPES = [
    "music",
    "sound",
    "scene",
    "background",
    "custom_cg",
    "overlay",
    "show",
    "move",
    "face",
    "hide",
    "focus",
    "offset",
    "say",
    "choice",
    "shock",
    "mask",
    "intro",
    "effect",
    "transition",
    "camera",
    "block",
    "cg",
    "dim",
    "message",
    "rotate",
    "dayenv",
    "stat",
    "stat_set",
    "affinity",
    "talent",
    "item",
    "flag",
    "game_flag",
    "enemy",
    "battle_skill",
    "battle_setup",
    "combat",
    "battle",
    "battle_result",
    "reward",
    "result_screen",
    "custom_shop",
    "stat_check",
    "affinity_check",
    "item_check",
    "talent_check",
    "flag_check",
    "activity",
    "mod_quest",
    "quest_check",
    "persistent_var",
    "persistent_check",
    "mission",
    "time",
    "autosave",
    "branch",
    "dice",
    "goto_scene",
    "panel",
    "wait",
    "end",
    "death",
    "raw",
]

# 官方元数据检查点（data/editor_data.json 的 dice_meta）
DICE_2BAND = "S0205_01_001"  # 2 个结果带：无独立大成功档（故事检查点）
DICE_3BAND = "Ch_6_8_2_Break_01_001"  # 3 个结果带：必填 goto_大成功


def fixed_start_say(story: dict, nid: str | None = None) -> str:
    """把起始 say 节点填成合法可编译状态（保留默认人物，其已在开场 show 登场）。"""
    node_id = nid or next(
        n["id"] for n in story["nodes"] if n.get("type") == "say"
    )
    story_api.update_node(story, node_id, {"text": "开场白"})
    return node_id


def base_story() -> dict:
    """new_story + 修好起始节点，后续测试直接往上加节点。"""
    story = story_api.new_story()
    fixed_start_say(story)
    return story


class TestNewStory(unittest.TestCase):
    def test_rejects_trailing_newline_in_story_id(self):
        with self.assertRaises(ValueError):
            story_api.new_story("main\n")

    def test_structure(self):
        story = story_api.new_story()
        self.assertIsInstance(story, dict, "new_story 应返回 dict")
        self.assertEqual(story["id"], "main", "默认剧情 id 应为 main")
        self.assertEqual(story["title"], "新剧情", "默认标题应为「新剧情」")
        self.assertIn("start", story, "应含 start 字段")
        nodes = story["nodes"]
        self.assertIsInstance(nodes, list, "nodes 应为数组")
        self.assertEqual(len(nodes), 2, "新建剧情应为 登场+对白 两个起始节点")
        self.assertEqual(nodes[0]["id"], story["start"], "start 应指向起始节点")
        self.assertEqual(nodes[0]["type"], "show", "起始节点应为 show（先登场再动作）")
        self.assertEqual(nodes[0]["id"], "show1", "起始节点编号应为类型+次序")
        self.assertEqual(nodes[1]["type"], "say", "第二个节点应为 say（models 契约）")
        self.assertEqual(nodes[1]["id"], "say1")

        s2 = story_api.new_story(story_id="case_a", title="自定义标题")
        self.assertEqual(s2["id"], "case_a", "自定义 story id 应生效")
        self.assertEqual(s2["title"], "自定义标题", "自定义标题应生效")
        self.assertEqual(
            s2["start"], s2["nodes"][0]["id"], "s2 start 应指向自身起始节点"
        )
        # mood：默认 false；可显式开启；非 bool 拒绝
        self.assertIs(s2["mood"], False, "mood 默认应为 False")
        s3 = story_api.new_story(story_id="case_b", title="带气泡", mood=True)
        self.assertIs(s3["mood"], True, "mood=True 应写入 story")
        with self.assertRaises(ValueError, msg="mood 非 bool 应抛 ValueError"):
            story_api.new_story(
                story_id="case_c",
                title="坏",
                mood="yes",  # type: ignore[reportArgumentType] 故意传非法类型
            )


class TestAddNode(unittest.TestCase):
    def test_63_types(self):
        story = story_api.new_story()
        self.assertEqual(len(ALL_TYPES), 63, "契约应有 63 种节点类型")
        self.assertEqual(
            set(ALL_TYPES), set(models.NODE_TYPES), "类型表与 models.NODE_TYPES 不一致"
        )
        ids = {n["id"] for n in story["nodes"]}
        for t in ALL_TYPES:
            node = story_api.add_node(story, t)
            self.assertIsInstance(node, dict, f"add_node({t}) 应返回 dict")
            self.assertEqual(node["type"], t, f"add_node({t}) type 应为 {t}")
            self.assertNotIn(node["id"], ids, f"{t} 节点 id 应唯一：{node['id']}")
            ids.add(node["id"])
            found = [n for n in story["nodes"] if n["id"] == node["id"]]
            self.assertEqual(len(found), 1, f"{t} 节点应写入 story.nodes")
        # 2 个开场节点（show+say）+ 63 种各一个 + hide 之后的首个动作节点
        # 触发登场防线自动补 1 个 show
        self.assertEqual(len(story["nodes"]), 2 + 63 + 1, "63 种类型应全部追加进 story（含自动补登场）")

    def test_unknown_type(self):
        story = story_api.new_story()
        with self.assertRaises(ValueError, msg="未知节点类型应抛 ValueError"):
            story_api.add_node(story, "no_such_type")

    def test_unknown_field(self):
        story = story_api.new_story()
        with self.assertRaises(ValueError, msg="未知字段应抛 ValueError"):
            story_api.add_node(story, "wait", {"seconds": 1, "bogus": 2})

    def test_field_type(self):
        story = story_api.new_story()
        with self.assertRaises(ValueError, msg="数值字段给字符串应抛 ValueError"):
            story_api.add_node(story, "wait", {"seconds": "abc"})

    def test_ids_are_type_plus_order(self):
        story = story_api.new_story()
        a = story_api.add_node(story, "wait")
        b = story_api.add_node(story, "wait")
        c = story_api.add_say(story, "第二句", mode="narrative")
        self.assertEqual(a["id"], "wait1")
        self.assertEqual(b["id"], "wait2")
        self.assertEqual(c["id"], "say2")
        extra_show = [n["id"] for n in story["nodes"] if n["type"] == "show"]
        self.assertTrue(all(i.startswith("show") for i in extra_show))

    def test_after_insert(self):
        story = story_api.new_story()
        first_id = story["start"]
        say_id = story["nodes"][1]["id"]  # 开场 say（show 之后）
        tail = story_api.add_node(story, "wait")  # 追加到末尾
        mid = story_api.add_node(story, "wait", {"seconds": 5}, after=first_id)
        order = [n["id"] for n in story["nodes"]]
        self.assertEqual(
            order,
            [first_id, mid["id"], say_id, tail["id"]],
            f"after=应插到指定节点之后：{order}",
        )
        mid_node = story_api.get_node(story, mid["id"])
        self.assertEqual(mid_node["seconds"], 5, "fields 应写入新节点")

    def test_after_missing(self):
        story = story_api.new_story()
        with self.assertRaises(ValueError, msg="after 指向不存在节点应抛 ValueError"):
            story_api.add_node(story, "wait", after="no_such")

    def test_custom_character_stage_actions_compile(self):
        story = base_story()
        raw = "user:mohui.luoxue"
        story_api.add_node(story, "show", {"character": raw, "position": "M"})
        story_api.add_node(
            story,
            "offset",
            {"character": raw, "x": 10, "y": -2, "duration": 0.2},
        )
        story_api.add_node(story, "shock", {"character": raw, "duration": 0.4})
        story_api.add_node(story, "dim", {"character": raw, "dimmed": True})
        story_api.add_node(
            story,
            "rotate",
            {"character": raw, "angle": 30, "duration": 0.3},
        )
        story_api.add_node(story, "end")

        lua, _warnings, _texts = story_api.compile_story(story)
        for global_name in (
            "mod_char_offset",
            "mod_char_shock",
            "mod_char_dim",
            "mod_char_rotate",
        ):
            self.assertIn(global_name, lua)


class TestUpdateNode(unittest.TestCase):
    def test_normal(self):
        story = story_api.new_story()
        w = story_api.add_node(story, "wait")
        ret = story_api.update_node(story, w["id"], {"seconds": 7.5})
        self.assertIsInstance(ret, dict, "update_node 应返回 dict")
        self.assertEqual(
            story_api.get_node(story, w["id"])["seconds"], 7.5, "改字段应生效"
        )

    def test_unknown_field(self):
        story = story_api.new_story()
        w = story_api.add_node(story, "wait")
        with self.assertRaises(ValueError, msg="未知字段应抛 ValueError"):
            story_api.update_node(story, w["id"], {"bogus": 1})

    def test_missing_node(self):
        story = story_api.new_story()
        with self.assertRaises(ValueError, msg="不存在节点应抛 ValueError"):
            story_api.update_node(story, "no_such", {"seconds": 1})


class TestListGetDelete(unittest.TestCase):
    def test_list_nodes(self):
        story = base_story()
        w = story_api.add_node(story, "wait")
        lst = story_api.list_nodes(story)
        self.assertIsInstance(lst, list, "list_nodes 应返回 list")
        self.assertEqual(len(lst), 3, "应有 3 个节点（开场 show+say + wait）")
        for entry, node in zip(lst, story["nodes"]):
            self.assertEqual(entry["id"], node["id"], "清单 id 应与节点一致")
            self.assertEqual(entry["type"], node["type"], "清单 type 应与节点一致")
            self.assertIsInstance(entry["summary"], str, "summary 应为字符串")
        self.assertEqual(lst[-1]["id"], w["id"], "清单顺序应跟随节点顺序")

    def test_get_node(self):
        story = base_story()
        w = story_api.add_node(story, "wait")
        node = story_api.get_node(story, w["id"])
        self.assertIs(node, w, "get_node 应返回该节点 dict")
        self.assertEqual(node["type"], "wait", "type 应为 wait")
        with self.assertRaises(ValueError, msg="get_node 不存在应抛 ValueError"):
            story_api.get_node(story, "no_such")

    def test_delete_node(self):
        story = base_story()
        w = story_api.add_node(story, "wait")
        ret = story_api.delete_node(story, w["id"])
        self.assertIsInstance(ret, dict, "delete_node 应返回 dict")
        self.assertNotIn(w["id"], [n["id"] for n in story["nodes"]], "节点应被删除")
        with self.assertRaises(ValueError, msg="delete_node 不存在应抛 ValueError"):
            story_api.delete_node(story, "no_such")


class TestMoveSetStart(unittest.TestCase):
    def test_move_node(self):
        story = base_story()  # n1(show) + n2(say)
        say_id = story["nodes"][1]["id"]
        w = story_api.add_node(story, "wait")
        e = story_api.add_node(story, "end")
        order = lambda: [n["id"] for n in story["nodes"]]  # noqa: E731
        story_api.move_node(story, story["start"], 1)  # 向后移一格
        self.assertEqual(
            order(),
            [say_id, story["start"], w["id"], e["id"]],
            "+1 应向后移动一格",
        )
        story_api.move_node(story, story["start"], -1)  # 移回
        self.assertEqual(
            order(),
            [story["start"], say_id, w["id"], e["id"]],
            "-1 应移回原位",
        )
        with self.assertRaises(ValueError, msg="move 不存在节点应抛 ValueError"):
            story_api.move_node(story, "no_such", 1)

    def test_rename_node(self):
        story = base_story()
        say_id = story["nodes"][1]["id"]
        story["nodes"][0]["goto"] = say_id
        node = story_api.rename_node(story, say_id, "talk")
        self.assertEqual(node["id"], "talk")
        self.assertEqual(story["nodes"][0]["goto"], "talk")
        self.assertEqual(story["start"], story["nodes"][0]["id"])
        with self.assertRaises(ValueError):
            story_api.rename_node(story, "talk", story["nodes"][0]["id"])

    def test_set_start(self):
        story = base_story()
        w = story_api.add_node(story, "wait")
        ret = story_api.set_start(story, w["id"])
        self.assertIsInstance(ret, dict, "set_start 应返回 dict")
        self.assertEqual(story["start"], w["id"], "start 应更新为指定节点")
        with self.assertRaises(ValueError, msg="set_start 不存在节点应抛 ValueError"):
            story_api.set_start(story, "no_such")


class TestAddChoice(unittest.TestCase):
    def test_2_and_4_options(self):
        story = base_story()
        n1 = story["start"]
        c2 = story_api.add_choice(story, [("甲", n1), ("乙", n1)])
        self.assertEqual(c2["type"], "choice", "应生成 choice 节点")
        self.assertEqual(len(c2["options"]), 2, "2 项选项应成功")
        c4 = story_api.add_choice(
            story, [("一", n1), ("二", n1), ("三", n1), ("四", n1)]
        )
        self.assertEqual(len(c4["options"]), 4, "4 项选项应成功")
        for opt in c2["options"] + c4["options"]:
            self.assertEqual(set(opt), {"text", "goto"}, "选项应含 text/goto")

    def test_1_option_rejected(self):
        story = base_story()
        with self.assertRaises(ValueError, msg="1 项选项应抛 ValueError"):
            story_api.add_choice(story, [("只有一个", story["start"])])

    def test_dialog_forced_options(self):
        story = base_story()
        node = story_api.add_choice(
            story, [("甲", story["start"]), ("乙", story["start"])]
        )
        self.assertEqual(
            node["dialog"], "Options", "choice.dialog 必须强制为 Options（契约 §3.3）"
        )


class TestAddDice(unittest.TestCase):
    def test_2band_success(self):
        story = base_story()
        n1 = story["start"]
        node = story_api.add_dice(story, DICE_2BAND, n1, n1)
        self.assertEqual(node["type"], "dice", "应生成 dice 节点")
        self.assertEqual(node["check"], DICE_2BAND, "check 应原样写入")
        self.assertEqual(
            node["options"],
            [{"goto_大成功": "", "goto_成功": n1, "goto_失败": n1}],
            "2 带检查点 options[0] 三向 goto 应正确（大成功为空）",
        )

    def test_3band_success(self):
        story = base_story()
        n1 = story["start"]
        node = story_api.add_dice(story, DICE_3BAND, n1, n1, n1)
        self.assertEqual(
            node["options"][0],
            {"goto_大成功": n1, "goto_成功": n1, "goto_失败": n1},
            "3 带检查点三向 goto 应全部写入",
        )

    def test_3band_missing_great_success(self):
        story = base_story()
        n1 = story["start"]
        with self.assertRaises(ValueError, msg="3 带缺 goto_大成功 应抛 ValueError"):
            story_api.add_dice(story, DICE_3BAND, n1, n1)

    def test_no_metadata_check(self):
        story = base_story()
        n1 = story["start"]
        with self.assertRaises(ValueError, msg="无官方元数据检查点应抛 ValueError"):
            story_api.add_dice(story, "No_Such_Dice_Check", n1, n1, n1)

    def test_band_texts_override_3band(self):
        # band_texts：逐带覆写骰子菜单选项文本（条数等于结果带数）
        story = base_story()
        n1 = story["start"]
        node = story_api.add_dice(
            story, DICE_3BAND, n1, n1, n1, band_texts=["失败", "成功", "大成功"]
        )
        self.assertEqual(
            node["options"][0]["band_texts"],
            ["失败", "成功", "大成功"],
            "3 带检查点 band_texts 应逐带写入",
        )

    def test_band_texts_override_2band(self):
        story = base_story()
        n1 = story["start"]
        node = story_api.add_dice(
            story, "S0120_01_001", n1, n1, band_texts=["失手", "得手"]
        )
        self.assertEqual(
            node["options"][0]["band_texts"], ["失手", "得手"], "2 带 band_texts 应写入"
        )

    def test_band_texts_len_mismatch(self):
        story = base_story()
        n1 = story["start"]
        with self.assertRaises(ValueError, msg="band_texts 条数不符应抛 ValueError"):
            story_api.add_dice(story, DICE_3BAND, n1, n1, n1, band_texts=["只有一条"])

    def test_band_texts_not_list(self):
        story = base_story()
        n1 = story["start"]
        with self.assertRaises(ValueError, msg="band_texts 非数组应抛 ValueError"):
            # 故意传非法类型验证校验层
            story_api.add_dice(
                story,
                DICE_3BAND,
                n1,
                n1,
                n1,
                band_texts="不是数组",  # type: ignore[reportArgumentType]
            )

    def test_band_texts_empty_item(self):
        story = base_story()
        n1 = story["start"]
        with self.assertRaises(ValueError, msg="band_texts 空条目应抛 ValueError"):
            story_api.add_dice(
                story, DICE_3BAND, n1, n1, n1, band_texts=["", "二", "三"]
            )


class TestAddSay(unittest.TestCase):
    def test_character_mode_requires_character(self):
        story = base_story()
        with self.assertRaises(
            ValueError, msg="character 模式缺 character 应抛 ValueError"
        ):
            story_api.add_say(story, "你好", mode="character")

    def test_narrative_mode_no_character(self):
        story = base_story()
        node = story_api.add_say(story, "山门前的风很大。", mode="narrative")
        self.assertEqual(node["type"], "say", "应生成 say 节点")
        self.assertEqual(node["text"], "山门前的风很大。", "text 应写入")
        self.assertNotIn("character", node, "narrative 节点不应写 character 字段")

    def test_character_mode_with_character(self):
        story = base_story()
        node = story_api.add_say(story, "师弟！", character="brother4")
        self.assertEqual(node["character"], "brother4", "character 应写入")
        self.assertEqual(node["portrait"], "normal", "portrait 默认应为 normal")
        self.assertEqual(node["mode"], "character", "mode 默认应为 character")

    def test_voice_optional_and_emitted(self):
        story = base_story()
        story_api.add_node(story, "end")
        bare = story_api.add_say(
            story, "没语音", mode="narrative", after=story["nodes"][1]["id"]
        )
        self.assertNotIn("voice", bare)
        lua, errors, _warnings = story_api.compile_story(story)
        self.assertEqual(errors, [])
        self.assertIsNotNone(lua)
        self.assertNotIn("mod_play_voice", lua)
        voiced = story_api.add_say(
            story, "有语音", mode="narrative", voice="user:mohui.line_01", after=bare["id"]
        )
        self.assertEqual(voiced["voice"], "user:mohui.line_01")
        lua, errors, _warnings = story_api.compile_story(story)
        self.assertEqual(errors, [])
        self.assertIn('mod_play_voice("user:mohui.line_01")', lua)

    def test_voice_must_be_user_ref(self):
        story = base_story()
        story_api.add_say(story, "坏", mode="narrative", voice="鈴鐺_001")
        errors, _warnings = story_api.check_story(story)
        self.assertTrue(errors)
        self.assertTrue(any("user:" in e for e in errors))


class TestPortraitValidation(unittest.TestCase):
    """show/say 的 (character, portrait) 必须落在官方角色表情表内（与编译器同表）。"""

    def test_valid_portrait_ok(self):
        story = base_story()
        node = story_api.add_say(
            story, "听好了！", character="brother4", portrait="laugh1"
        )
        self.assertEqual(node["portrait"], "laugh1", "合法表情应写入")

    def test_user_character_portrait_format(self):
        story = base_story()
        node = story_api.add_say(
            story, "自定义", character="user:mohui.luoxue", portrait="happy"
        )
        self.assertEqual(node["character"], "user:mohui.luoxue")
        self.assertEqual(node["portrait"], "happy")
        with self.assertRaises(ValueError):
            story_api.add_say(
                story, "坏表情", character="user:mohui.luoxue", portrait="开心"
            )

    def test_invalid_portrait_add_say(self):
        story = base_story()
        with self.assertRaises(ValueError, msg="非法表情应抛 ValueError"):
            story_api.add_say(story, "哼！", character="brother4", portrait="angry3")

    def test_invalid_portrait_add_node(self):
        story = base_story()
        with self.assertRaises(ValueError, msg="add_node(show) 非法表情应抛 ValueError"):
            story_api.add_node(
                story,
                "show",
                {"character": "brother4", "position": "R1", "portrait": "angry3"},
            )
        with self.assertRaises(ValueError, msg="add_node(say) 非法表情应抛 ValueError"):
            story_api.add_node(
                story,
                "say",
                {"text": "哼！", "character": "brother4", "portrait": "angry3"},
            )

    def test_invalid_portrait_update_node(self):
        story = base_story()
        node = story_api.add_say(story, "先正常说一句。", character="brother4")
        with self.assertRaises(ValueError, msg="update_node 非法表情应抛 ValueError"):
            story_api.update_node(story, node["id"], {"portrait": "angry3"})

    def test_unknown_character_passes(self):
        story = base_story()
        node = story_api.add_say(
            story, "自造角色随便说。", character="my_oc", portrait="whatever"
        )
        self.assertEqual(node["portrait"], "whatever", "表外角色应放行")


class TestAddScene(unittest.TestCase):
    def test_normal(self):
        story = base_story()
        node = story_api.add_scene(story, "center")
        self.assertEqual(node["type"], "scene", "应生成 scene 节点")
        self.assertEqual(node["view"], "center", "view 应写入")


class TestAddDeath(unittest.TestCase):
    def test_success_default_title(self):
        story = base_story()
        node = story_api.add_death(story, "你坠入山崖，万事休矣。", "910021")
        self.assertEqual(node["type"], "death", "应生成 death 节点")
        self.assertEqual(node["text"], "你坠入山崖，万事休矣。", "text 应写入")
        self.assertEqual(node["death_id"], "910021", "death_id 应写入")
        self.assertEqual(node["next"], "Title", "next 默认应为 Title")

    def test_next_free_rejected_because_game_ignores_it(self):
        story = base_story()
        with self.assertRaises(ValueError, msg="原版死亡画面不支持 next=Free"):
            story_api.add_death(story, "命数已尽。", "910021", next="Free")

    def test_title_optional(self):
        story = base_story()
        # 不给 title：不写字段（codegen 用缺省「勝敗乃兵家常事」）
        node = story_api.add_death(story, "命数已尽。", "910021")
        self.assertNotIn("title", node, "缺省 title 不应写字段")
        # 给 title：写入字段
        node2 = story_api.add_death(story, "命数已尽。", "910021", title="坠崖谢幕")
        self.assertEqual(node2["title"], "坠崖谢幕", "title 应写入")
        # 非法 title 类型：抛 ValueError
        with self.assertRaises(ValueError, msg="非法 title 应抛 ValueError"):
            story_api.add_death(story, "命数已尽。", "910021", title=123)  # type: ignore[reportArgumentType]

    def test_death_id_required(self):
        story = base_story()
        with self.assertRaises(TypeError, msg="缺 death_id 应抛 TypeError"):
            story_api.add_death(story, "命数已尽。")  # type: ignore[reportCallIssue]
        for bad in ("", None, 123):
            with self.assertRaises(
                ValueError, msg=f"非法 death_id 应抛 ValueError: {bad!r}"
            ):
                story_api.add_death(story, "命数已尽。", bad)  # type: ignore[reportArgumentType]

    def test_death_id_official_rejected(self):
        story = base_story()
        for bad in ("10021", "20003", "899999", "abc", "91002x"):
            with self.assertRaises(
                ValueError, msg=f"<900000 的 death_id 应抛 ValueError: {bad!r}"
            ):
                story_api.add_death(story, "命数已尽。", bad)

    def test_empty_text_rejected(self):
        story = base_story()
        for bad in ("", "   ", None, 123):
            with self.assertRaises(
                ValueError, msg=f"空/非法 text 应抛 ValueError: {bad!r}"
            ):
                # 故意传非法类型验证校验层
                story_api.add_death(story, bad, "910021")  # type: ignore[reportArgumentType]

    def test_bad_next_rejected(self):
        story = base_story()
        with self.assertRaises(ValueError, msg="非法 next 应抛 ValueError"):
            story_api.add_death(story, "命数已尽。", "910021", next="Story")

    def test_multiline_text(self):
        story = base_story()
        node = story_api.add_death(story, "第一行\n第二行", "910021")
        self.assertEqual(node["text"], "第一行\n第二行", "多行文本应保留换行")

    def test_compiles(self):
        # death：黑屏 → mod_set_death_text(title, text)（两段式，字面量）→ 官方 GameOver
        # 死亡画面（mod 专属 death_id）；死亡文本不走已读 key（死亡节点的 key 不入 Lua）
        story = base_story()
        node = story_api.add_death(story, "命数已尽。", "910021", next="Title")
        lua, errors, _warnings = story_api.compile_story(story)
        assert lua is not None, f"death 剧情应编译成功：{errors}"
        self.assertIn('\tmod_set_death_text("勝敗乃兵家常事", "命数已尽。")', lua)
        self.assertNotIn("MOD_MOD_main_%s" % node["id"], lua)
        self.assertIn('luamanager.ChangeScene("GameOver", "910021", "Title")', lua)

    def test_goto_scene_title_desc(self):
        # goto_scene 的 title/desc 字段（结局卡片）经 add_node/update_node 写入
        story = base_story()
        node = story_api.add_node(
            story,
            "goto_scene",
            {"scene": "End", "key": "920047", "title": "武林传奇", "desc": "传说。"},
        )
        self.assertEqual(node["title"], "武林传奇", "title 应写入")
        self.assertEqual(node["desc"], "传说。", "desc 应写入")
        lua, errors, _warnings = story_api.compile_story(story)
        assert lua is not None, f"goto_scene 剧情应编译成功：{errors}"
        self.assertIn('\tmod_set_ending_text("武林传奇", "传说。")', lua)
        # 非法类型（title 非 str）被字段校验拒绝
        with self.assertRaises(ValueError, msg="title 非 str 应抛 ValueError"):
            story_api.add_node(
                story,
                "goto_scene",
                {"scene": "End", "title": 42},  # type: ignore[reportArgumentType]
            )


class TestCheckStory(unittest.TestCase):
    def test_demo_style_no_errors(self):
        story = story_api.load_story_json(DEMO_STORY)
        errors, warnings = story_api.check_story(story)
        self.assertEqual(errors, [], f"demo 剧情应 0 错误：{errors}")
        self.assertIsInstance(warnings, list, "warnings 应为 list")

    def test_transition_in_without_out_warns(self):
        story = base_story()
        story_api.add_node(story, "transition", {"phase": "in"})
        story_api.add_node(story, "end")
        errors, warnings = story_api.check_story(story)
        self.assertEqual(errors, [], f"transition 未配对应无错误：{errors}")
        self.assertTrue(warnings, "应产生非致命警告")
        self.assertTrue(
            any("黑幕" in w for w in warnings),
            f"警告应提示黑幕隐患：{warnings}",
        )

    def test_choice_dialog_talk_errors(self):
        story = base_story()
        n1 = story["start"]
        node = story_api.add_choice(story, [("甲", n1), ("乙", n1)])
        story_api.update_node(story, node["id"], {"dialog": "Talk"})
        story_api.add_node(story, "end")
        errors, warnings = story_api.check_story(story)
        self.assertTrue(errors, "choice.dialog=Talk 应报错")
        self.assertTrue(
            any("Options" in e for e in errors),
            f"错误应指向仅支持 Options：{errors}",
        )


class TestCompileStory(unittest.TestCase):
    def test_success(self):
        story = base_story()
        n1 = story["start"]
        story_api.add_scene(story, "center")
        story_api.add_say(story, "去吧。", character="player")
        story_api.add_choice(story, [("去", n1), ("留", n1)])
        story_api.add_node(story, "end")
        lua, errors, warnings = story_api.compile_story(story)
        assert lua is not None, "合法剧情应编译成功"
        self.assertEqual(errors, [], f"合法剧情应无编译错误：{errors}")
        self.assertIsInstance(warnings, list, "warnings 应为 list")
        self.assertIn("function", lua, "Lua 产物应含 node 函数")

    def test_error_returns_none(self):
        story = base_story()
        story_api.update_node(story, story["start"], {"goto": "not_exist"})
        lua, errors, warnings = story_api.compile_story(story)
        self.assertIsNone(lua, "编译失败应返回 None")
        self.assertTrue(errors, "悬空 goto 应产生错误")
        self.assertIsInstance(warnings, list, "warnings 应为 list")


class TestSaveLoad(unittest.TestCase):
    def test_round_trip(self):
        story = base_story()
        story_api.add_say(story, "往返测试", character="player", mode="narrative")
        story_api.add_choice(story, [("回", story["start"]), ("再回", story["start"])])
        story_api.add_node(story, "end")
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "story.json"
            self.assertIsNone(story_api.save_story_json(story, p), "save 应无返回值")
            back = story_api.load_story_json(p)
        self.assertEqual(copy.deepcopy(story), back, "save→load 往返应保持一致")
        self.assertEqual(back["id"], "main", "往返后 id 应保留")

    def test_save_failure_preserves_old_file_and_cleans_temp(self):
        story = base_story()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "story.json"
            old_content = json.dumps(base_story(), ensure_ascii=False)
            target.write_text(old_content, encoding="utf-8")
            with mock.patch.object(models.os, "replace", side_effect=OSError("boom")):
                with self.assertRaises(OSError):
                    models.save_story(story, target)
            self.assertEqual(target.read_text(encoding="utf-8"), old_content)
            self.assertEqual(list(root.glob("story.json.*.tmp")), [])


class TestPackMod(unittest.TestCase):
    def _manifest(self) -> dict:
        # 字段仿 samples/demo_mod/manifest.json（契约 §2）
        return {
            "format": 1,
            "id": "api_test_mod",
            "name": "API 测试 Mod",
            "version": "1.0.0",
            "author": "story_api_test",
            "description": "story_api pack_mod 契约测试",
            "entry": "main",
        }

    def _make_mod_dir(self, root: Path) -> Path:
        mod_dir = root / "api_test_mod"
        (mod_dir / "story").mkdir(parents=True)
        (mod_dir / "manifest.json").write_text(
            json.dumps(self._manifest(), ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        story = base_story()
        story_api.add_say(story, "打包测试文本。", character="player")
        story_api.add_node(story, "end")
        story_api.save_story_json(story, mod_dir / "story" / "main.json")
        return mod_dir

    def test_pack_ok(self):
        with tempfile.TemporaryDirectory() as tmp:
            mod_dir = self._make_mod_dir(Path(tmp))
            out = story_api.pack_mod(str(mod_dir))
            self.assertIsInstance(out, str, "pack_mod 应返回输出路径字符串")
            self.assertTrue(Path(out).is_file(), f"打包产物应存在：{out}")
            self.assertTrue(out.lower().endswith(".lommod"), "产物应为 .lommod")
            with zipfile.ZipFile(out) as zf:
                names = set(zf.namelist())
                self.assertIn("manifest.json", names, "zip 应含 manifest.json")
                self.assertIn("story/main.json", names, "zip 应含 story/main.json")
                self.assertIn("lua/main.lua", names, "zip 应含 lua/main.lua")
                self.assertIn("texts.json", names, "zip 应含 texts.json（已读文本表）")
                lua = zf.read("lua/main.lua").decode("utf-8")
                self.assertIn("function", lua, "包内 Lua 应为编译产物")
                texts = json.loads(zf.read("texts.json").decode("utf-8"))
                self.assertEqual(
                    texts.get("MOD_api_test_mod_main_say2"),
                    "打包测试文本。",
                    "texts.json 应含 say 文本（key=MOD_<modid>_<scriptid>_<nodeid>）",
                )
                self.assertIn("MOD_api_test_mod_main_say1", texts, "起始 say 也应入表")

    def test_missing_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            mod_dir = Path(tmp) / "no_manifest"
            (mod_dir / "story").mkdir(parents=True)
            with self.assertRaises(ValueError, msg="缺失 manifest 应抛 ValueError"):
                story_api.pack_mod(str(mod_dir))


class TestLuaMarkers(unittest.TestCase):
    """防回归关键点：API 构建的剧情编译产物必须含官方范式关键调用。"""

    def test_markers(self):
        story = base_story()
        n1 = story["start"]
        story_api.add_node(
            story,
            "show",
            {"character": "player", "position": "M", "portrait": "nervous1"},
        )
        story_api.add_scene(story, "center")
        story_api.add_say(story, "关键点检查。", character="player")
        story_api.add_choice(story, [("去", n1), ("留", n1)])
        story_api.add_node(story, "end")
        lua, errors, _warnings = story_api.compile_story(story)
        assert lua is not None, f"应编译成功：{errors}"
        for marker, why in (
            ("LoadCharacterPortrait", "show/say 应自动加载表情差分"),
            ("flowcharts.LoadView", "scene 应预载背景防黑屏"),
            ("sayoptions.waitforinput", "say 前应设等待输入"),
            ("luamanager.GetStoryText", "say 文本应走已读 key 机制"),
            ("mod_hide_mood", "story.mood 默认 false 应隐藏官方心情气泡"),
            ("menudialogs.Options", "choice 应使用 Options 皮肤"),
        ):
            self.assertIn(marker, lua, f"Lua 产物应包含 {marker}（{why}）")


class TestBranchSources(unittest.TestCase):
    """branch 五来源（mod/game/stat/flag_value/condition）经 story_api 往返。"""

    def test_stat_source(self):
        story = base_story()
        node = story_api.add_node(
            story,
            "branch",
            {
                "source": "stat",
                "stat": "mental",
                "cases": [{"op": ">=", "value": 50, "goto": story["start"]}],
            },
        )
        story_api.add_node(story, "end")
        self.assertEqual(node["source"], "stat", "source 应写入")
        self.assertEqual(node["stat"], "mental", "stat 字段应写入")
        self.assertNotIn("flag", node, "stat 来源不应残留 flag 键")
        errors, _ = story_api.check_story(story)
        self.assertEqual(errors, [], f"stat 分支应校验通过：{errors}")
        lua, _, _ = story_api.compile_story(story)
        self.assertIn('luamanager.GetStatData("mental", 1)', lua or "")

    def test_flag_value_source(self):
        story = base_story()
        story_api.add_node(
            story,
            "branch",
            {
                "source": "flag_value",
                "flag": "50019",
                "cases": [{"op": "==", "value": 1, "goto": story["start"]}],
            },
        )
        story_api.add_node(story, "end")
        errors, _ = story_api.check_story(story)
        self.assertEqual(errors, [], f"flag_value 分支应校验通过：{errors}")
        lua, _, _ = story_api.compile_story(story)
        self.assertIn('tonumber(luamanager.GetFlagData("50019"))', lua or "")

    def test_condition_source(self):
        story = base_story()
        story_api.add_node(
            story,
            "branch",
            {
                "source": "condition",
                "flag": "S0030_01_001",
                "cases": [
                    {"value": 1, "goto": story["start"]},
                    {"value": 2, "goto": story["start"]},
                ],
            },
        )
        story_api.add_node(story, "end")
        errors, _ = story_api.check_story(story)
        self.assertEqual(errors, [], f"condition 分支应校验通过：{errors}")
        lua, _, _ = story_api.compile_story(story)
        self.assertIn('checkpointmanager.Condition("S0030_01_001")', lua or "")

    def test_bad_stat_missing(self):
        story = base_story()
        story_api.add_node(
            story,
            "branch",
            {"source": "stat", "cases": [{"value": 1, "goto": story["start"]}]},
        )
        story_api.add_node(story, "end")
        errors, _ = story_api.check_story(story)
        self.assertTrue(errors, "stat 来源缺 stat 字段应报错")
        self.assertTrue(any('必填字段 "stat"' in e for e in errors), str(errors))

    def test_bad_op_rejected(self):
        story = base_story()
        story_api.add_node(
            story,
            "branch",
            {
                "source": "stat",
                "stat": "mental",
                "cases": [{"op": "!=", "value": 1, "goto": story["start"]}],
            },
        )
        story_api.add_node(story, "end")
        errors, _ = story_api.check_story(story)
        self.assertTrue(any('"op" 必须是' in e for e in errors), str(errors))


class TestLoadEditorData(unittest.TestCase):
    def test_load_editor_data(self):
        editor_data, is_fallback = story_api.load_editor_data()
        self.assertIsInstance(editor_data, dict, "editor_data 应为 dict")
        self.assertIn("characters", editor_data, "editor_data 应含 characters")
        self.assertFalse(is_fallback, "项目自带 data/editor_data.json，不应兜底")
        self.assertIn("dice_meta", editor_data, "editor_data 应含 dice_meta")
        self.assertIn(DICE_2BAND, editor_data["dice_meta"], "官方元数据应含 2 带检查点")


def main_fn() -> int:
    prog = unittest.main(verbosity=2, exit=False)
    result = prog.result
    return 0 if result is not None and result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main_fn())
