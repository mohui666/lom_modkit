# -*- coding: utf-8 -*-
"""story_api 契约测试（无 GUI / 无 PySide6 依赖）。

用法（在 editor/ 目录下）：
    .venv/Scripts/python tests/story_api_test.py

覆盖 docs/mod_format.md §7「AI 工具接口」契约的全部函数：
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

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))
sys.path.insert(0, str(EDITOR_DIR.parent))  # compiler/lomc 与 data/ 在项目根

try:
    # story_api.py 由并行 agent 落地；落地后该 ignore 可去掉
    import story_api  # noqa: E402  # type: ignore[reportMissingImports]
except ImportError as exc:  # 并行 agent 尚未落地时给出明确提示
    raise SystemExit(
        "无法导入 editor/story_api.py（%s）——story_api 可能尚未实现，请稍后重试" % exc
    ) from exc

import models  # noqa: E402

PROJECT_ROOT = EDITOR_DIR.parent
DEMO_STORY = PROJECT_ROOT / "samples" / "demo_mod" / "story" / "main.json"

# 契约 §3.1 全量 39 种节点类型（与 models.NODE_TYPES 一一对应）
ALL_TYPES = [
    "music",
    "sound",
    "scene",
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
    "stat",
    "stat_set",
    "affinity",
    "talent",
    "item",
    "flag",
    "game_flag",
    "enemy",
    "battle_skill",
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
DICE_2BAND = "Travel_601_101_001"  # 2 个结果带：无独立大成功档
DICE_3BAND = "Ch_6_8_2_Break_01_001"  # 3 个结果带：必填 goto_大成功


def fixed_start_say(story: dict, nid: str | None = None) -> str:
    """把起始 say 节点填成合法可编译状态（text + character），返回节点 id。"""
    node_id = nid or story["start"]
    story_api.update_node(story, node_id, {"text": "开场白", "character": "player"})
    return node_id


def base_story() -> dict:
    """new_story + 修好起始节点，后续测试直接往上加节点。"""
    story = story_api.new_story()
    fixed_start_say(story)
    return story


class TestNewStory(unittest.TestCase):
    def test_structure(self):
        story = story_api.new_story()
        self.assertIsInstance(story, dict, "new_story 应返回 dict")
        self.assertEqual(story["id"], "main", "默认剧情 id 应为 main")
        self.assertEqual(story["title"], "新剧情", "默认标题应为「新剧情」")
        self.assertIn("start", story, "应含 start 字段")
        nodes = story["nodes"]
        self.assertIsInstance(nodes, list, "nodes 应为数组")
        self.assertEqual(len(nodes), 1, "新建剧情应恰好含 1 个起始节点")
        self.assertEqual(nodes[0]["id"], story["start"], "start 应指向起始节点")
        self.assertEqual(nodes[0]["type"], "say", "起始节点应为 say（models 契约）")

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
                story_id="case_c", title="坏", mood="yes"  # type: ignore[reportArgumentType] 故意传非法类型
            )


class TestAddNode(unittest.TestCase):
    def test_39_types(self):
        story = story_api.new_story()
        self.assertEqual(len(ALL_TYPES), 39, "契约应有 39 种节点类型")
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
        self.assertEqual(len(story["nodes"]), 1 + 39, "39 种类型应全部追加进 story")

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

    def test_after_insert(self):
        story = story_api.new_story()
        first_id = story["start"]
        tail = story_api.add_node(story, "wait")  # 追加到末尾
        mid = story_api.add_node(story, "wait", {"seconds": 5}, after=first_id)
        order = [n["id"] for n in story["nodes"]]
        self.assertEqual(
            order,
            [first_id, mid["id"], tail["id"]],
            f"after=应插到指定节点之后：{order}",
        )
        mid_node = story_api.get_node(story, mid["id"])
        self.assertEqual(mid_node["seconds"], 5, "fields 应写入新节点")

    def test_after_missing(self):
        story = story_api.new_story()
        with self.assertRaises(ValueError, msg="after 指向不存在节点应抛 ValueError"):
            story_api.add_node(story, "wait", after="no_such")


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
        self.assertEqual(len(lst), 2, "应有 2 个节点")
        for entry, node in zip(lst, story["nodes"]):
            self.assertEqual(entry["id"], node["id"], "清单 id 应与节点一致")
            self.assertEqual(entry["type"], node["type"], "清单 type 应与节点一致")
            self.assertIsInstance(entry["summary"], str, "summary 应为字符串")
        self.assertEqual(lst[1]["id"], w["id"], "清单顺序应跟随节点顺序")

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
        story = base_story()  # n1(say)
        w = story_api.add_node(story, "wait")  # n2
        e = story_api.add_node(story, "end")  # n3
        order = lambda: [n["id"] for n in story["nodes"]]  # noqa: E731
        story_api.move_node(story, story["start"], 1)  # 向后移一格
        self.assertEqual(
            order(), [w["id"], story["start"], e["id"]], "+1 应向后移动一格"
        )
        story_api.move_node(story, story["start"], -1)  # 移回
        self.assertEqual(order(), [story["start"], w["id"], e["id"]], "-1 应移回原位")
        with self.assertRaises(ValueError, msg="move 不存在节点应抛 ValueError"):
            story_api.move_node(story, "no_such", 1)

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


class TestAddScene(unittest.TestCase):
    def test_normal(self):
        story = base_story()
        node = story_api.add_scene(story, "center")
        self.assertEqual(node["type"], "scene", "应生成 scene 节点")
        self.assertEqual(node["view"], "center", "view 应写入")


class TestAddDeath(unittest.TestCase):
    def test_success_default_title(self):
        story = base_story()
        node = story_api.add_death(story, "你坠入山崖，万事休矣。")
        self.assertEqual(node["type"], "death", "应生成 death 节点")
        self.assertEqual(node["text"], "你坠入山崖，万事休矣。", "text 应写入")
        self.assertEqual(node["next"], "Title", "next 默认应为 Title")

    def test_next_free(self):
        story = base_story()
        node = story_api.add_death(story, "命数已尽。", next="Free")
        self.assertEqual(node["next"], "Free", "next=Free 应写入")

    def test_empty_text_rejected(self):
        story = base_story()
        for bad in ("", "   ", None, 123):
            with self.assertRaises(ValueError, msg=f"空/非法 text 应抛 ValueError: {bad!r}"):
                # 故意传非法类型验证校验层
                story_api.add_death(story, bad)  # type: ignore[reportArgumentType]

    def test_bad_next_rejected(self):
        story = base_story()
        with self.assertRaises(ValueError, msg="非法 next 应抛 ValueError"):
            story_api.add_death(story, "命数已尽。", next="Story")

    def test_multiline_text(self):
        story = base_story()
        node = story_api.add_death(story, "第一行\n第二行")
        self.assertEqual(node["text"], "第一行\n第二行", "多行文本应保留换行")

    def test_compiles(self):
        # death 走已读 key + 场景跳转，编译产物应含 GetStoryText 与 ChangeScene
        story = base_story()
        node = story_api.add_death(story, "命数已尽。", next="Title")
        lua, errors, _warnings = story_api.compile_story(story)
        assert lua is not None, f"death 剧情应编译成功：{errors}"
        self.assertIn(
            'luamanager.GetStoryText("MOD_MOD_main_%s")' % node["id"], lua
        )
        self.assertIn('luamanager.ChangeScene("Title", "", "")', lua)


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
                    texts.get("MOD_api_test_mod_main_n2"), "打包测试文本。",
                    "texts.json 应含 say 文本（key=MOD_<modid>_<scriptid>_<nodeid>）",
                )
                self.assertIn("MOD_api_test_mod_main_n1", texts, "起始 say 也应入表")

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
