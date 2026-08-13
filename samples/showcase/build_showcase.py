#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""全功能展示 mod 构建脚本（samples/showcase/build_showcase.py）。

剧本梗概：元叙事喜剧——赵活发现自己被「编剧」折腾，与四师兄逐一吐槽
演出/数值/流程三大类共 39 种节点，最后掷骰子打赌，按检定结果进入三种
结局（链式第二幕 / 回自由模式 / 认输离场）；第二幕经 end.next_script
链式接力，用自定义死亡文本演出「坠崖梗」收尾。

一切剧情构建都经 story_api（editor/story_api.py，AI 与编辑器共用的
受控写入口），不手写 story JSON / Lua；随后本脚本自带硬性自查：
  1) 39 种节点全覆盖清单；
  2) 清单校验：music/effect/view/position/character/portrait/stat/
     talent/item/game_flag 全部来自 data/editor_data.json（不编造）；
  3) 骰子检查点验证：check 在 dice_meta 中、非 Travel_*、官方调用点
     文件不是旅行脚本、未被 SetCurrentTravelScript/SetTempScript 引用；
  4) transition in/out 成对、choice 皮肤=Options、story 顶层不设 mood
     （默认 false=隐藏官方心情气泡，正好演示该修复）、末节点收尾；
  5) 从 start 出发的图可达性自检（不可达节点会被剔除，必须 100%）；
  6) story_api.check_story / compile_story（与 lomc pack 同一套校验）。

运行：
    editor/.venv/Scripts/python samples/showcase/build_showcase.py
打包（另行执行）：
    PYTHONPATH=compiler python -m lomc pack samples/showcase -o samples/showcase.lommod
"""

# pyright: reportMissingImports=false
# ruff: noqa: UP031
# 本文件按任务要求以「sys.path.insert(0, <项目根>/editor) + import story_api」
# 的运行时方式接入受控写入口（见下），静态分析器无法解析该动态导入，属预期。
# 文案沿用项目既有 % 格式化风格（editor/story_api.py、compiler/lomc 同风格）。
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

# Windows 控制台统一 UTF-8 输出
for _stream in (sys.stdout, sys.stderr):
    _reconfigure = getattr(_stream, "reconfigure", None)
    if _reconfigure is not None:
        _reconfigure(encoding="utf-8", errors="replace")

PROJECT_ROOT = Path(__file__).resolve().parents[2]
EDITOR_DIR = PROJECT_ROOT / "editor"
if str(EDITOR_DIR) not in sys.path:
    sys.path.insert(0, str(EDITOR_DIR))
import story_api  # type: ignore[reportMissingImports]  # noqa: E402

SHOWCASE_DIR = Path(__file__).resolve().parent
STORY_DIR = SHOWCASE_DIR / "story"
RAW_SCRIPTS_DIR = Path(r"C:/Users/mohui666/lom_unpack/raw_scripts")

MOD_ID = "showcase"
MANIFEST = {
    "format": 1,
    "id": MOD_ID,
    "name": "全功能展示",
    "version": "1.0.0",
    "author": "lom_modkit",
    "description": (
        "演示全部自定义功能：39 种节点、表情差分、已读变黄快进、"
        "自定义死亡文本、心情气泡开关、多剧情链式"
    ),
    "entry": "main",
}

# 骰子检查点选择（构建期动态验证，见 validate_dice_check）：
#   3 带：Ch_1_1_1_001                 —— 官方 ch1_1_1.lua.txt 主线剧情检定，
#        骰子 99，带 <=30 / <=60 / >60，三向 goto 全演示。
#   2 带：Talk_Brother4_02_01_01_001   —— 官方 talk_brother4_option_02_01_01
#        .lua.txt 与四师兄谈话检定，骰子 99，带 <50 / >=50，无独立大成功档。
DICE_3BAND = "Ch_1_1_1_001"
DICE_2BAND = "Talk_Brother4_02_01_01_001"
# branch source=game 的官方检查点 Switch 名（与 3 带骰子同名，ch1_1_1 实证）
SWITCH_GAME = "Ch_1_1_1_001"

# ---------------------------------------------------------------------------
# 自查基础件
# ---------------------------------------------------------------------------
ERRORS: list[str] = []


def check(cond: bool, msg: str) -> None:
    if cond:
        print("[通过] %s" % msg)
    else:
        ERRORS.append("[失败] %s" % msg)
        print("[失败] %s" % msg)


def load_editor_data() -> dict:
    ed, is_fallback = story_api.load_editor_data()
    if is_fallback:
        raise SystemExit("data/editor_data.json 不可用（走了兜底数据），拒绝构建")
    return ed


def read_raw_scripts() -> dict[str, str]:
    """官方脚本目录 -> {文件名: 内容}；目录缺失时直接失败（骰子验证必须）。"""
    if not RAW_SCRIPTS_DIR.is_dir():
        raise SystemExit("官方脚本目录不存在: %s" % RAW_SCRIPTS_DIR)
    out: dict[str, str] = {}
    for f in RAW_SCRIPTS_DIR.iterdir():
        if f.name.endswith(".lua.txt"):
            out[f.name] = f.read_text(encoding="utf-8", errors="replace")
    if not out:
        raise SystemExit("官方脚本目录为空: %s" % RAW_SCRIPTS_DIR)
    return out


# ---------------------------------------------------------------------------
# 剧本构建（全部经 story_api 固定规则写入）
# ---------------------------------------------------------------------------
def build_main(ed: dict) -> dict:
    """第一幕：演出/数值/流程全演示，末尾三结局分支。"""
    st = story_api.new_story("main", "全功能展示·第一幕")
    # 规则 6：story 顶层不设 mood——缺省即 false（隐藏官方心情气泡），
    # 正好演示该修复；显式 pop 掉 new_story 写入的 mood 键。
    st.pop("mood", None)

    def say(text, character=None, mode="character", portrait="normal", after=None):
        return story_api.add_say(
            st, text, character=character, mode=mode, portrait=portrait, after=after
        )["id"]

    def node(t, fields=None, after=None):
        return story_api.add_node(st, t, fields, after=after)["id"]

    # --- 开场（new_story 自带的 n1 改为旁白） ---
    n1 = story_api.update_node(
        st,
        "n1",
        {
            "text": "【旁白】话说唐门校场，本日风和日丽。\n赵活却觉得哪里不对劲——因为他听见了自己的旁白。",
            "mode": "narrative",
        },
    )
    n1.pop("character", None)  # narrative 模式忽略 character，去掉更干净

    # --- 演出类：音乐 / 场景 / 出场 / 表情差分 ---
    node("music", {"name": "古怪_001"})
    node("scene", {"view": "center"})
    node("show", {"character": "player", "position": "M", "portrait": "normal"})
    say("谁？谁在说话！", "player", portrait="normal")
    node("show", {"character": "brother4", "position": "R1", "portrait": "normal"})
    say("赵师弟，冷静。那个声音——我也能听见它。", "brother4")
    say("四师兄你也能听见？！这到底是怎么回事！", "player", portrait="nervous1")
    say("哈哈，我们被一个叫「编剧」的家伙抓来，演出这出《全功能展示》了。", "brother4", portrait="laugh1")
    say("编剧……所以刚才的旁白、音乐、场景，全是他的手笔？", "player", portrait="nervous2")
    say("没错。他说这出戏里藏着三十九种节点，一样都不能少。", "brother4")
    node("move", {"character": "player", "from": "M", "to": "L2", "duration": 1.2})
    node("face", {"character": "brother4", "facing": "left"})
    node("focus", {"character": "brother4"})
    say("你别说，这镜头还怪会聚焦的……", "player")
    node("shock", {"character": "brother4", "duration": 0.4})
    say("哇！谁在摇我！", "brother4", portrait="shock")
    node("offset", {"character": "player", "x": 20, "y": -10, "duration": 0.4})
    say("我、我被弹飞了？！", "player", portrait="suck1")
    node("effect", {"name": "Hit_001", "x": 10, "y": -5})
    say("【旁白】shock 震动、offset 位移、effect 特效——三连击。赵活，撑住。", mode="narrative")
    node("camera", {"name": "stage-memory", "active": True})
    say("（think·内心独白）这回忆滤镜……我好像看见了自己被编剧反复改稿的人生走马灯。", "player", mode="think")
    node("camera", {"name": "stage-memory", "active": False})

    # --- transition in/out 成对 + 第二场景 ---
    node("transition", {"phase": "in", "dir": "lr"})
    node("scene", {"view": "backmountain2"})
    node("transition", {"phase": "out", "dir": "lr"})
    say("黑场转场加切景——我们瞬间从校场到了后山。", "player")

    # --- 音效三变体 / 通用块 / 标题卡 / 介绍卡 / 遮罩 ---
    node("sound", {"name": "鈴鐺_001"})
    say("哪来的铃铛声？唐门后山可没养骡子。", "brother4")
    node("sound", {"name": "鳥_001", "kind": "env"})
    say("【旁白】环境音起。鸟鸣山更幽……除了旁边有个碎碎念的赵活。", mode="narrative")
    node("sound", {"name": "鳥_001", "kind": "env", "op": "fadeout", "seconds": 1})
    node("block", {"flowchart": "common", "name": "flash"})
    node("block", {"flowchart": "common", "name": "shake"})
    say("白屏、地震！flowchart 通用块 flash 和 shake，一次看够！", "player", portrait="shock")
    node("cg", {"action": "show", "kind": "title", "key": "P_Back_01"})
    say("【居中旁白】大标题字卡——「後山」二字高悬。", mode="center")
    say("【旁白】接下来请欣赏：人物介绍卡。", mode="narrative")
    node("intro", {"character": "brother4"})
    say("轮到我的高光时刻了！", "brother4", portrait="laugh2")
    node("mask", {"show": True})
    say("（遮罩下的独白）mask 一拉，全世界都安静了……", "player", mode="think")
    node("mask", {"show": False})
    node("hide", {"character": "brother4", "fadeDuration": 0.5})
    say("四师兄消失了。编剧的 hide 节点来无影去无踪。", "player")
    node("show", {"character": "brother4", "position": "L1", "portrait": "laugh1"})
    say("我又回来了！看我的笑脸——show 节点直接加载表情差分。", "brother4", portrait="laugh1")
    node("show", {"character": "trainee1", "position": "R1", "portrait": "normal"})
    say("那个……我是被编剧临时拉来凑数演示的师弟。", "trainee1")
    say("好家伙，连路人都是编制内的。", "player")
    say("别贫了。下面是数值状态环节，编剧说一共十二种。", "brother4")

    # --- 数值状态类 12 种 ---
    node("stat", {"key": "mental", "delta": 5})
    node("stat", {"key": "money", "delta": 50})
    node("stat_set", {"key": "talking", "value": 66})
    node("affinity", {"character": "brother4", "delta": 1})
    node("talent", {"talent": "1010", "level": 1})
    node("item", {"kind": "misc", "item": "2001", "count": 2})
    node("item", {"kind": "book", "item": "2001", "count": 1})
    node("item", {"kind": "misc", "item": "2001", "count": 1, "remove": True})
    node("item", {"kind": "special", "item": "2001", "count": 1})
    say(
        "心相加五、银两加五十、嘴力拉到六十六、四师兄好感加一、学会金钟罩、"
        "两瓶黄酒、一本流星剑谱、又丢了一瓶、白捡一个机巧飞蝗……",
        "player",
    )
    say("师兄，你赚麻了。", "trainee1")
    node("flag", {"flag": "SHOWCASE_MET_BRO4"})
    s_flag_say = say(
        "【旁白】剧情 flag「SHOWCASE_MET_BRO4」已写入：AddStory 记录经历 + modflags 供分支读取。",
        mode="narrative",
    )

    # branch source=mod：case 1/2 全覆盖，两条支线汇入 mergeA
    merge_a = say("【汇合点】mod 分支两条路在此相会。", "brother4", after=s_flag_say)
    bm2 = say(
        "branch（source=mod）case 2：flag 未设置才会来这（静态可达，演出中走不到）。",
        "player",
        after=s_flag_say,
    )
    bm1 = say(
        "branch（source=mod）case 1：flag 已设置——此路必通。",
        "player",
        after=s_flag_say,
    )
    node(
        "branch",
        {
            "flag": "SHOWCASE_MET_BRO4",
            "source": "mod",
            "cases": [{"value": 1, "goto": bm1}, {"value": 2, "goto": bm2}],
        },
        after=s_flag_say,
    )
    # 插入顺序：[s_flag_say, branch, bm1, bm2, merge_a, ...]（after 同锚点逆序插）
    story_api.update_node(st, bm1, {"goto": merge_a})
    story_api.update_node(st, bm2, {"goto": merge_a})

    # game_flag + branch source=game（else 兜底落顺序下一节点 mergeB）
    node("game_flag", {"flag": "50019", "value": 1, "op": "set"})
    s_gf_say = say(
        "【旁白】官方任务 flag「50019」已写入（game_flag 节点，id 取自 editor_data.game_flags 清单）。",
        mode="narrative",
    )
    merge_b = say(
        "官方检查点分支：Switch 读不到（0）或读到 3 时，会走 else 兜底直接落到这里。",
        "trainee1",
        portrait="nervous1",
        after=s_gf_say,
    )
    bg2 = say(
        "branch（source=game）读到 == 2。", "player", after=s_gf_say
    )
    bg1 = say(
        "branch（source=game）读到官方检查点 Ch_1_1_1_001 == 1。",
        "player",
        after=s_gf_say,
    )
    node(
        "branch",
        {
            "flag": SWITCH_GAME,
            "source": "game",
            "cases": [{"value": 1, "goto": bg1}, {"value": 2, "goto": bg2}],
        },
        after=s_gf_say,
    )
    # 插入顺序：[s_gf_say, branch, merge_b, bg1, bg2, ...]
    story_api.update_node(st, bg1, {"goto": merge_b})
    story_api.update_node(st, bg2, {"goto": merge_b})

    # --- 数值状态类收尾 + 面板 / 等待 / 音乐三变体 ---
    node("enemy", {"op": "team", "enemy": "400", "value": -10})
    node("battle_skill", {"op": "set", "key": "special3", "index": 2})
    node("mission", {"name": "Main", "key": "M0001"})
    node("time", {"op": "round"})
    node("autosave", {"kind": "story", "save_button": 1})
    say(
        "【旁白】敌方队伍削弱、战场技能装载、主线推进 M0001、时间过旬、自动存档"
        "——数值状态十二连全部演完。",
        mode="narrative",
    )
    node("panel", {"panel": "martial", "mode": 0})
    say("武学面板（martial）被强制打开。这是演示，看完记得关。", "player")
    node("wait", {"seconds": 0.8})
    node("music", {"name": "古怪_001", "op": "fadeout", "seconds": 2})
    node("music", {"name": "快樂_001"})
    say("音乐淡出变体（fadeout）——两秒后切新曲。", "brother4")
    node("music", {"name": "快樂_001", "op": "stop"})
    say("立刻 stop——戛然而止。play、fadeout、stop 三变体齐活。", "player")
    node("hide", {"character": "trainee1", "fadeDuration": 0.4})
    say("铺垫完毕！赵师弟，敢不敢掷骰子？三带检定——大成功、成功、失败，三个结局。", "brother4", portrait="laugh1")
    s_last = say(
        "掷！……等等，这骰子怎么是官方检查点 Ch_1_1_1_001？编剧你盗用主线骰子！",
        "player",
    )

    # --- 结尾分支块（全部 after=s_last，逆序插入得到目标数组顺序） ---
    # 目标顺序：[s_last, raw, choice, dice3, big, end2, suc, end1, fail, gv, gsfree]
    gs_free = node("goto_scene", {"scene": "Free"}, after=s_last)
    gv = say("认输保平安。赵活退出赌局，深藏功与名。", "player", after=s_last)
    fail = say("【失败】骰面丢人。四师兄笑到打鸣。", "player", portrait="nervous3", after=s_last)
    end1 = node("end", {}, after=s_last)
    suc = say("【成功】骰面中规中矩。四师兄，你欠我二两银子。", "player", after=s_last)
    end2 = node("end", {"next_script": "second"}, after=s_last)
    big = say("【大成功】骰面六十往上！四师兄，把你炼丹房的丹炉输给我！", "player", portrait="laugh1", after=s_last)
    d3 = story_api.add_dice(
        st,
        DICE_3BAND,
        goto_成功=suc,
        goto_失败=fail,
        goto_大成功=big,
        after=s_last,
    )["id"]
    story_api.add_choice(
        st,
        [
            ("掷！三带检定（官方检查点）", d3),
            ("我不掷，认输直接走", gv),
        ],
        after=s_last,
    )
    node(
        "raw",
        {"code": "-- [raw 逃逸口演示] 原生 Lua 原样插入编译产物（本行仅为注释，任何官方机制都能在这里直接写）"},
        after=s_last,
    )
    story_api.update_node(st, big, {"goto": end2})
    story_api.update_node(st, suc, {"goto": end1})
    story_api.update_node(st, fail, {"goto": gs_free})
    story_api.update_node(st, gv, {"goto": gs_free})
    return st


def build_second(ed: dict) -> dict:
    """第二幕：end.next_script 链式接力 + 2 带骰子 + 自定义死亡文本（坠崖梗）。"""
    st = story_api.new_story("second", "全功能展示·第二幕（坠崖加演）")
    st.pop("mood", None)  # 同 main：顶层不设 mood，演示默认 false

    def say(text, character=None, mode="character", portrait="normal", after=None):
        return story_api.add_say(
            st, text, character=character, mode=mode, portrait=portrait, after=after
        )["id"]

    def node(t, fields=None, after=None):
        return story_api.add_node(st, t, fields, after=after)["id"]

    n1 = story_api.update_node(
        st,
        "n1",
        {
            "text": "【旁白】第二幕——「end.next_script」链式接力成功！\n主剧本一个 end 就把你送来了。",
            "mode": "narrative",
        },
    )
    n1.pop("character", None)
    node("scene", {"view": "cliff_night"})
    node("show", {"character": "player", "position": "M", "portrait": "normal"})
    say("悬崖？！编剧！我大成功赢来的丹炉呢？怎么给我发配到峭壁边上了！", "player")
    node("music", {"name": "陰森_001"})
    s_last = say(
        "四师兄的声音从身后飘来——「赵师弟，再来一局？这回是二带检定，"
        "只有成功和失败，没有大成功。」",
        "player",
        portrait="nervous1",
    )

    # 结尾分支块（after=s_last 逆序插入）：
    # 目标顺序：[s_last, d2, suc2, death_suc, fail2, death_fail]
    death_fail = story_api.add_death(
        st,
        "【自定义死亡文本】检定失败，赵活自己跳了下去。\n"
        "（第二幕完：next=Free 回自由模式。重玩时已读文本会变黄、可快进。）",
        next="Free",
        after=s_last,
    )["id"]
    fail2 = say("【失败】手一抖，骰子掉下悬崖……人也跟着下去了。", "player", portrait="suck2", after=s_last)
    death_suc = story_api.add_death(
        st,
        "【自定义死亡文本】你赢了骰子，却输给了剧本。\n"
        "脚下一滑，赵活坠入万丈深渊。\n"
        "（死亡文本节点：黑屏 + 居中旁白 + 已读系统联动，回标题画面）",
        next="Title",
        after=s_last,
    )["id"]
    suc2 = say(
        "【成功】我稳稳接住了骰子！……然后编剧说：「坠崖是加演的保留节目。」不——！",
        "player",
        portrait="nervous3",
        after=s_last,
    )
    story_api.add_dice(
        st,
        DICE_2BAND,
        goto_成功=suc2,
        goto_失败=fail2,
        goto_大成功="",  # 2 带检查点：无独立大成功档，留空
        after=s_last,
    )
    story_api.update_node(st, suc2, {"goto": death_suc})
    story_api.update_node(st, fail2, {"goto": death_fail})
    return st


# ---------------------------------------------------------------------------
# 自查
# ---------------------------------------------------------------------------
def validate_dice_check(ed: dict, raw: dict[str, str], check_id: str, expect_bands: int) -> None:
    """硬性规则 2：骰子检查点必须在 dice_meta 中、非旅行检查点、调用点文件安全。"""
    meta = ed.get("dice_meta") or {}
    check(check_id in meta, "骰子检查点 %s 在 editor_data.dice_meta 中（缺元数据会致骰子菜单 NRE）" % check_id)
    check(not check_id.startswith("Travel_"), "骰子检查点 %s 不是 Travel_* 旅行检查点" % check_id)
    bands = (meta.get(check_id) or {}).get("bands") or []
    check(len(bands) == expect_bands, "骰子检查点 %s 结果带数 = %d（预期 %d）" % (check_id, len(bands), expect_bands))
    # 官方调用点文件（checkpointmanager.Dice("check" ...)）
    callers = [
        f for f, c in raw.items()
        if 'checkpointmanager.Dice("%s"' % check_id in c
    ]
    check(bool(callers), "骰子检查点 %s 在官方脚本中有调用点：%s" % (check_id, ", ".join(callers)))
    for cf in callers:
        stem = cf[: -len(".lua.txt")]
        is_travel = "travel" in stem.lower()
        check(not is_travel, "调用点文件 %s 不是旅行脚本（travel_*/*_travel*）" % cf)
        refs = [
            f for f, c in raw.items()
            if re.search(
                r'Set(?:Temp|CurrentTravel)Script\([^)]*"%s(_travel)?"' % re.escape(stem), c
            )
        ]
        check(not refs, "调用点文件 %s 未被 SetCurrentTravelScript/SetTempScript 引用（引用自 %s）" % (cf, ", ".join(refs) or "无"))


def validate_catalog(ed: dict, stories: list[dict]) -> None:
    """硬性规则 4/7：所有清单 id 来自 editor_data.json；人物只用 player/brother4/trainee1。"""
    chars = {c["id"]: set(c.get("portraits") or []) for c in ed["characters"]}
    music = {m["id"] for m in ed["music"]}
    views = {v["id"] for v in ed["views"]}
    positions = {p["id"] for p in ed["positions"]}
    stats = {s["id"] for s in ed["stats"]}
    effects = {e["id"] for e in ed["effects"]}
    talents = {t["id"] for t in ed["talents"]}
    game_flags = {g["id"] for g in ed["game_flags"]}
    affinity_chars = set(ed.get("affinity_characters") or [])
    items = {
        "book": {i["id"] for i in ed["items_book"]},
        "misc": {i["id"] for i in ed["items_misc"]},
        "special": {i["id"] for i in ed["items_special"]},
    }
    used_chars: set[str] = set()

    def char_ok(n, story_id):
        c = n["character"]
        if c not in chars:
            return 'story=%s 节点 %s: 人物 "%s" 不在 editor_data.characters' % (story_id, n["id"], c)
        used_chars.add(c)
        if c not in ("player", "brother4", "trainee1"):
            return "story=%s 节点 %s: 人物 \"%s\" 超出建议范围（player/brother4/trainee1）" % (story_id, n["id"], c)
        return None

    bad = []
    for story in stories:
        sid = story["id"]
        for n in story["nodes"]:
            t = n["type"]
            err = None
            if t == "music":
                if n["name"] not in music:
                    err = "音乐 %s 不在 editor_data.music" % n["name"]
            elif t == "effect":
                if n["name"] not in effects:
                    err = "特效 %s 不在 editor_data.effects" % n["name"]
            elif t == "scene":
                if n["view"] not in views:
                    err = "场景 %s 不在 editor_data.views" % n["view"]
            elif t == "show":
                err = char_ok(n, sid)
                if not err and n["position"] not in positions:
                    err = "站位 %s 不在 editor_data.positions" % n["position"]
                if not err and n.get("portrait") not in chars[n["character"]]:
                    err = '表情 "%s" 不是人物 %s 拥有的立绘差分' % (n.get("portrait"), n["character"])
            elif t == "move":
                err = char_ok(n, sid)
                if not err and (n["from"] not in positions or n["to"] not in positions):
                    err = "move 站位不在 editor_data.positions"
            elif t in ("face", "hide", "focus", "offset", "shock", "intro"):
                err = char_ok(n, sid)
            elif t == "say":
                if n.get("character"):
                    err = char_ok(n, sid)
                char = n.get("character")
                if not err and char and n.get("portrait") not in chars[char]:
                    err = '表情 "%s" 不是人物 %s 拥有的立绘差分' % (n.get("portrait"), char)
            elif t in ("stat", "stat_set"):
                if n["key"] not in stats:
                    err = "属性 %s 不在 editor_data.stats" % n["key"]
            elif t == "affinity":
                err = char_ok(n, sid)
                if not err and n["character"] not in affinity_chars:
                    err = "好感度人物 %s 不在 editor_data.affinity_characters" % n["character"]
            elif t == "talent":
                if n["talent"] not in talents:
                    err = "天赋 %s 不在 editor_data.talents" % n["talent"]
            elif t == "item":
                if n["item"] not in items.get(n["kind"], set()):
                    err = "物品 %s 不在 editor_data.items_%s" % (n["item"], n["kind"])
            elif t == "game_flag":
                if n["flag"] not in game_flags:
                    err = "官方 flag %s 不在 editor_data.game_flags（游戏会静默忽略）" % n["flag"]
            elif t == "choice":
                if n.get("dialog") != "Options":
                    err = "choice 皮肤必须是 Options（其它皮肤是自由场景 break 菜单，会崩）"
            if err:
                bad.append("story=%s 节点 %s(%s): %s" % (sid, n["id"], t, err))
    for b in bad:
        check(False, b)
    if not bad:
        print("[通过] 清单校验：全部 music/effect/view/position/character/portrait/stat/talent/item/game_flag 均取自 editor_data.json")
        print("[通过] 人物只使用：%s" % "、".join(sorted(used_chars)))


def validate_structure(ed: dict, raw: dict[str, str], stories: list[dict]) -> None:
    """transition 成对 / 末节点收尾 / mood 缺省 / 39 种节点覆盖 / 官方 Switch 名实证。"""
    all_types: dict[str, int] = {}
    for story in stories:
        sid = story["id"]
        nodes = story["nodes"]
        for i, n in enumerate(nodes):
            all_types[n["type"]] = all_types.get(n["type"], 0) + 1
            t = n["type"]
            if t == "transition":
                if n["phase"] == "in":
                    lifted = any(
                        m.get("type") == "transition" and m.get("phase") == "out"
                        for m in nodes[i + 1:]
                    )
                    check(lifted, "story=%s 节点 %s(transition in) 之后有成对 out（否则黑幕盖满全场）" % (sid, n["id"]))
                else:
                    covered = any(
                        m.get("type") == "transition" and m.get("phase") == "in"
                        for m in nodes[:i]
                    )
                    check(covered, "story=%s 节点 %s(transition out) 之前有成对 in" % (sid, n["id"]))
        last = nodes[-1]
        check(
            last["type"] in ("end", "death", "goto_scene", "raw", "choice", "branch", "dice"),
            "story=%s 末节点 %s(%s) 是合法收尾类型" % (sid, last["id"], last["type"]),
        )
        check("mood" not in story, "story=%s 顶层未设 mood（默认 false=隐藏心情气泡）" % sid)
        # choice 的每个 goto 都指向真实节点
        for n in nodes:
            if n["type"] == "choice":
                for o in n["options"]:
                    check(
                        any(m["id"] == o["goto"] for m in nodes),
                        "story=%s choice 节点 %s 的 goto=%s 指向真实节点" % (sid, n["id"], o["goto"]),
                    )
    # 39 种节点全覆盖（models.NODE_TYPES 即契约全量）
    missing = [t for t in story_api.models.NODE_TYPES if t not in all_types]
    check(not missing, "39 种节点全覆盖（缺失：%s）" % (", ".join(missing) or "无"))
    print("\n--- 节点覆盖清单（%d 种 / 契约 39 种）---" % len(all_types))
    for t in story_api.models.NODE_TYPES:
        cnt = all_types.get(t, 0)
        print("  %-14s %-12s ×%d" % (t, story_api.models.NODE_TYPE_CN.get(t, t), cnt))
    # branch source=game 的 Switch 名必须来自官方脚本实证
    switch_names: set[str] = set()
    for c in raw.values():
        switch_names.update(re.findall(r'checkpointmanager\.Switch\("([^"]+)"\)', c))
    check(SWITCH_GAME in switch_names, "branch source=game 的 Switch 名 %s 来自官方脚本实证" % SWITCH_GAME)


def validate_reachability(stories: list[dict]) -> None:
    """从 start 出发的图可达性自检：不可达节点会被剔除，必须 100%。"""
    no_seq = ("choice", "branch", "dice", "end", "death", "goto_scene")
    for story in stories:
        nodes = story["nodes"]
        edges: dict[str, list[str]] = {}
        for i, n in enumerate(nodes):
            tgt: list[str] = []
            t = n["type"]
            if t == "choice":
                tgt += [o["goto"] for o in n["options"]]
            elif t == "dice":
                o = n["options"][0]
                for k in ("goto_大成功", "goto_成功", "goto_失败"):
                    if o.get(k):
                        tgt.append(o[k])
            elif t == "branch":
                tgt += [c["goto"] for c in n["cases"]]
                src = n.get("source", "mod")
                need_fallback = src == "game" or {c["value"] for c in n["cases"]} != {1, 2}
                if need_fallback and i + 1 < len(nodes):
                    tgt.append(nodes[i + 1]["id"])  # else 兜底
            else:
                if "goto" in n:
                    tgt.append(n["goto"])
                elif t not in no_seq and i + 1 < len(nodes):
                    tgt.append(nodes[i + 1]["id"])
            edges[n["id"]] = tgt
        seen = {story["start"]}
        stack = [story["start"]]
        while stack:
            for nxt in edges.get(stack.pop(), []):
                if nxt not in seen:
                    seen.add(nxt)
                    stack.append(nxt)
        unreachable = [n["id"] for n in nodes if n["id"] not in seen]
        total = len(nodes)
        check(not unreachable, "story=%s 可达性 %d/%d（100%%；不可达：%s）" % (story["id"], total - len(unreachable), total, ", ".join(unreachable) or "无"))


# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------
def main() -> int:
    print("=" * 62)
    print("全功能展示 mod 构建（经 story_api 受控写入）")
    print("=" * 62)
    ed = load_editor_data()
    print("[信息] editor_data.json schema=%s（fallback=False）" % ed.get("schema"))
    raw = read_raw_scripts()
    print("[信息] 官方脚本目录 %s：%d 个文件" % (RAW_SCRIPTS_DIR, len(raw)))

    print("\n--- 1/5 构建剧情（story_api） ---")
    main_story = build_main(ed)
    second_story = build_second(ed)
    stories = [main_story, second_story]
    print("[信息] main：%d 个节点；second：%d 个节点" % (len(main_story["nodes"]), len(second_story["nodes"])))

    print("\n--- 2/5 清单校验（规则 4/7） ---")
    validate_catalog(ed, stories)

    print("\n--- 3/5 骰子检查点验证（规则 2） ---")
    validate_dice_check(ed, raw, DICE_3BAND, expect_bands=3)
    validate_dice_check(ed, raw, DICE_2BAND, expect_bands=2)

    print("\n--- 4/5 结构校验（transition 成对 / 末节点 / mood 缺省 / 39 种覆盖） ---")
    validate_structure(ed, raw, stories)

    print("\n--- 5/5 可达性自检 + story_api 全量校验编译 ---")
    validate_reachability(stories)
    say_death_count = 0
    for story in stories:
        for n in story["nodes"]:
            if n["type"] in ("say", "death"):
                say_death_count += 1
        errors, warnings = story_api.check_story(story)
        check(not errors, "story=%s check_story 无错误（%s）" % (story["id"], "; ".join(errors) or "0 条"))
        for w in warnings:
            check(False, "story=%s 编译警告不应出现：%s" % (story["id"], w))
        lua, cerrs, cwarns = story_api.compile_story(story)
        check(lua is not None and not cerrs, "story=%s compile_story 成功（%s）" % (story["id"], "; ".join(cerrs) or "0 条"))
        for w in cwarns:
            check(False, "story=%s 编译警告不应出现：%s" % (story["id"], w))
    print("[信息] say+death 节点共 %d 个 → 打包后 texts.json 应恰好 %d 条" % (say_death_count, say_death_count))

    print("\n--- 写出产物 ---")
    if ERRORS:
        print("\n共 %d 项自查失败，不写文件。" % len(ERRORS))
        for e in ERRORS:
            print("  " + e)
        return 1
    STORY_DIR.mkdir(parents=True, exist_ok=True)
    (SHOWCASE_DIR / "manifest.json").write_text(
        json.dumps(MANIFEST, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    for story in stories:
        story_api.save_story_json(story, STORY_DIR / ("%s.json" % story["id"]))
        print("[写出] %s" % (STORY_DIR / ("%s.json" % story["id"])))
    print("[写出] %s" % (SHOWCASE_DIR / "manifest.json"))
    print("\n构建成功：全部自查通过（39 种节点、骰子检查点、可达性 100%）。")
    print("下一步：PYTHONPATH=compiler python -m lomc pack samples/showcase -o samples/showcase.lommod")
    return 0


if __name__ == "__main__":
    sys.exit(main())
