#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""全功能展示 mod 构建脚本（samples/showcase/build_showcase.py）。

剧本梗概：元叙事喜剧——赵活发现自己被「编剧」折腾，与四师兄逐一吐槽
演出/数值/流程三大类共 43 种节点，最后掷骰子打赌：大成功直接链第二幕；
成功/失败/认输则汇入收尾三选（回自由模式逛练武场 / 睡到下月下旬再逛 /
继续第二幕），成功线四师兄好感 +3，解锁练武场好感事件。第二幕经
end.next_script 链式接力：2 带骰子失败回环重试（不再直接结束），成功
可选自定义死亡文本（910021）或结局卡片（End 920047 + title/desc）。
练武场另有三个条件触发器事件（train_affinity 好感≥3 / train_dusk 下旬 /
train_any 默认闲逛），全部 end 回自由模式。

一切剧情构建都经 story_api（editor/story_api.py，AI 与编辑器共用的
受控写入口），不手写 story JSON / Lua；随后本脚本自带硬性自查：
  1) 43 种节点全覆盖清单；
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
        "演示全部自定义功能：43 种节点、表情差分、已读变黄快进、"
        "自定义死亡文本、心情气泡开关、多剧情链式、战役模式、"
        "练武场时间/好感条件触发器"
    ),
    "entry": "main",
    # 战役化：标题画面开新战役（隔离存档槽）；禁用原版地图事件（只保留本 mod
    # 触发器）；练武场（Center）三个触发器，数组顺序=优先级（取第一个全部命中
    # 的）：好感≥3 → 下旬(旬3) → 默认闲逛。
    "campaign": {
        "new_game": True,
        "disable_official_events": True,
        "triggers": [
            {
                "type": "position",
                "position": "Center",
                "script": "train_affinity",
                "when_affinity": {"character": "brother4", "min": 3},
            },
            {
                "type": "position",
                "position": "Center",
                "script": "train_dusk",
                "when_stage": 3,
            },
            {"type": "position", "position": "Center", "script": "train_any"},
        ],
    },
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
    say(
        "哈哈，我们被一个叫「编剧」的家伙抓来，演出这出《全功能展示》了。",
        "brother4",
        portrait="laugh1",
    )
    say(
        "编剧……所以刚才的旁白、音乐、场景，全是他的手笔？",
        "player",
        portrait="nervous2",
    )
    say("没错。他说这出戏里藏着四十三种节点，一样都不能少。", "brother4")
    node("move", {"character": "player", "from": "M", "to": "L2", "duration": 1.2})
    node("face", {"character": "brother4", "facing": "left"})
    node("focus", {"character": "brother4"})
    say("你别说，这镜头还怪会聚焦的……", "player")
    node("shock", {"character": "brother4", "duration": 0.4})
    say("哇！谁在摇我！", "brother4", portrait="shock")
    node("offset", {"character": "player", "x": 20, "y": -10, "duration": 0.4})
    say("我、我被弹飞了？！", "player", portrait="suck1")
    node("effect", {"name": "Hit_001", "x": 10, "y": -5})
    say(
        "【旁白】shock 震动、offset 位移、effect 特效——三连击。赵活，撑住。",
        mode="narrative",
    )
    node("camera", {"name": "stage-memory", "active": True})
    say(
        "（think·内心独白）这回忆滤镜……我好像看见了自己被编剧反复改稿的人生走马灯。",
        "player",
        mode="think",
    )
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
    say(
        "白屏、地震！flowchart 通用块 flash 和 shake，一次看够！",
        "player",
        portrait="shock",
    )
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
    say(
        "我又回来了！看我的笑脸——show 节点直接加载表情差分。",
        "brother4",
        portrait="laugh1",
    )
    node("show", {"character": "trainee1", "position": "R1", "portrait": "normal"})
    say("那个……我是被编剧临时拉来凑数演示的师弟。", "trainee1")
    say("好家伙，连路人都是编制内的。", "player")
    say("别贫了。下面是数值状态环节，编剧说一共十二种。", "brother4")

    # --- 四个高价值新节点：dim（压暗）/ message（系统提示）/ rotate（旋转）/ dayenv（日夜环境） ---
    node("dim", {"character": "trainee1", "dimmed": True})
    say(
        "【旁白】dim 节点：师弟被压暗成背景板——配角让戏，主角高光。",
        mode="narrative",
    )
    node(
        "message",
        {"text": "【系统提示】message 节点：系统提示条显示原文（DisplayMessageText，不走本地化 key）。"},
    )
    say("系统提示也演了一出——message 原文直出，编剧改词都不怕。", "player")
    node("rotate", {"character": "brother4", "angle": -10, "duration": 0.3})
    say("rotate 节点——我怎么突然歪了？", "brother4", portrait="shock")
    node("rotate", {"character": "brother4", "angle": 10, "duration": 0.3})
    say("又转回来了。角度在前、时长在后，编剧说这是官方参数序。", "player")
    node("dayenv", {"day_type": 1})
    say(
        "【旁白】dayenv 节点：日夜环境切到白天（1=白天 / 2=晚上，官方枚举实证）。",
        mode="narrative",
    )

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
    bg2 = say("branch（source=game）读到 == 2。", "player", after=s_gf_say)
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
    node("stat", {"key": "fate", "delta": 2})
    say(
        "【旁白】命运 +2：一路行善攒下的命运点已入账——掷骰子时，它可以逆天改命。",
        mode="narrative",
    )
    say(
        "铺垫完毕！赵师弟，敢不敢掷骰子？三带检定——大成功、成功、失败，三个结局。"
        "对了，听说多行善事能攒命运点，掷骰子时可以逆天改命——你刚才那两点，可别浪费了。",
        "brother4",
        portrait="laugh1",
    )
    s_last = say(
        "掷！……等等，这骰子怎么是官方检查点 Ch_1_1_1_001？编剧你盗用主线骰子！",
        "player",
    )

    # --- 结尾分支块（全部 after=s_last，逆序插入得到目标数组顺序） ---
    # 目标顺序：[s_last, raw, choice, d3, big, suc, aff, fin_c, end2,
    #            time_set, fail, gv, gsfree]
    # 大成功 → 直接链第二幕；成功/失败/认输 → 汇入收尾三选；成功线四师兄
    # 好感 +3（解锁练武场 when_affinity 事件 train_affinity）；「睡到下月
    # 下旬」演示时间条件触发器 train_dusk（when_stage=3，回自由模式后
    # 点练功场可见）。
    gs_free = node("goto_scene", {"scene": "Free"}, after=s_last)
    gv = say("认输保平安。赵活退出赌局，深藏功与名。", "player", after=s_last)
    fail = say(
        "【失败】骰面丢人。四师兄笑到打鸣。",
        "player",
        portrait="nervous3",
        after=s_last,
    )
    time_set = node(
        "time", {"op": "set", "year": 1, "month": 4, "stage": 3}, after=s_last
    )
    end2 = node("end", {"next_script": "second"}, after=s_last)
    fin_c = story_api.add_choice(
        st,
        [
            ("回自由模式逛逛练武场", gs_free),
            ("睡到下月下旬再去（演示时间条件触发器）", time_set),
            ("继续第二幕（坠崖加演）", end2),
        ],
        after=s_last,
    )["id"]
    aff = node("affinity", {"character": "brother4", "delta": 3}, after=s_last)
    suc = say(
        "【成功】骰面中规中矩。四师兄，你欠我二两银子——他嘴上认输，心里服气。",
        "player",
        after=s_last,
    )
    big = say(
        "【大成功】骰面六十往上！四师兄，把你炼丹房的丹炉输给我！",
        "player",
        portrait="laugh1",
        after=s_last,
    )
    d3 = story_api.add_dice(
        st,
        DICE_3BAND,
        goto_成功=suc,
        goto_失败=fail,
        goto_大成功=big,
        # band_texts 逐带覆写骰子菜单选项文本（顺序=官方结果带展示顺序：
        # 最差带 → 最优带），条数必须等于检查点带数（3）
        band_texts=[
            "骰面三十往下——赵活的丢人时刻",
            "骰面中规中矩，四师兄捏把汗",
            "骰面六十往上——丹炉拿来！",
        ],
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
        {
            "code": "-- [raw 逃逸口演示] 原生 Lua 原样插入编译产物（本行仅为注释，任何官方机制都能在这里直接写）"
        },
        after=s_last,
    )
    story_api.update_node(st, big, {"goto": end2})
    story_api.update_node(st, suc, {"goto": aff})
    story_api.update_node(st, aff, {"goto": fin_c})
    story_api.update_node(st, fail, {"goto": fin_c})
    story_api.update_node(st, gv, {"goto": fin_c})
    story_api.update_node(st, time_set, {"goto": gs_free})
    return st


def build_second(ed: dict) -> dict:
    """第二幕：end.next_script 链式接力 + 2 带骰子回环重试 + 死亡/结局卡片演示。

    2 带骰子失败不再直接坠崖：失败 → 四师兄安慰 → 二选一（回环重试 /
    认命坠崖）；成功 → 二选一（自定义死亡文本 910021 演示 / 结局卡片演示：
    goto_scene End 920047 + title/desc，运行时 mod_set_ending_text 由官方
    结局画面绘制）。
    """
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
        "只有成功和失败，没有大成功。而且……这次没有别的结局，只有坠崖。」",
        "player",
        portrait="nervous1",
    )

    # --- 失败支线：不直接坠崖——安慰后二选一，可回环重试（goto 回 s_last 骰子前） ---
    fail2 = say(
        "【失败】手一抖，骰子掉下悬崖……人也差点跟着下去了。",
        "player",
        portrait="suck2",
        after=s_last,
    )
    f_say = say(
        "四师兄探出头来：「别急别急！掉的是骰子，不是你。师兄我再给你一次机会——」",
        "player",
        portrait="nervous2",
        after=fail2,
    )
    death_fail = story_api.add_death(
        st,
        "刻意的游戏设计",
        death_id="910021",  # mod 专属 id（9+官方 10021 乱战中被践踏而死）
        next="Free",
        title="少侠留步",
        after=f_say,
    )["id"]
    story_api.add_choice(
        st,
        [
            ("再来一局！谁怕谁", s_last),
            ("不玩了，认命坠崖", death_fail),
        ],
        after=f_say,
    )

    # --- 成功支线：二选一演示死亡文本（910021）与结局卡片（End 920047） ---
    suc2 = say(
        "【成功】我稳稳接住了骰子！……然后编剧说：「恭喜通关，请选择你的谢幕方式。」",
        "player",
        portrait="nervous3",
        after=death_fail,
    )
    death_suc = story_api.add_death(
        st,
        "刻意的游戏设计",
        death_id="910021",  # mod 专属 id（9+官方 10021 乱战中被践踏而死）
        next="Title",
        title="坠崖谢幕",
        after=suc2,
    )["id"]
    end_demo = node(
        "goto_scene",
        {
            "scene": "End",
            "key": "920047",  # mod 专属结局 id（9+官方 20047 武林传奇）
            "next": "Story",
            "title": "全功能展示·武林传奇",
            "desc": "耕阳读书斋将你的事迹编撰成书，发行于市。\n"
            "唐门活侠——一个被编剧折腾了三幕的倒霉蛋——大名一朝传遍中原。\n"
            "（结局卡片：title/desc 由 mod_set_ending_text 交给官方结局画面绘制。）",
        },
        after=suc2,
    )
    story_api.add_choice(
        st,
        [
            ("坠崖谢幕（死亡演示 910021）", death_suc),
            ("直接播结局卡（End 920047）", end_demo),
        ],
        after=suc2,
    )

    # 2 带骰子：成功→suc2，失败→fail2（失败回环重试，不再直接结束）；
    # band_texts 逐带覆写（2 条：最差带 → 最优带）
    story_api.add_dice(
        st,
        DICE_2BAND,
        goto_成功=suc2,
        goto_失败=fail2,
        goto_大成功="",  # 2 带检查点：无独立大成功档，留空
        band_texts=[
            "手一抖，骰子向悬崖飞去——",
            "稳如老狗，骰子稳稳接住！",
        ],
        after=s_last,
    )
    return st


def build_train_affinity(ed: dict) -> dict:
    """练武场·四师兄好感≥3 特殊事件（when_affinity 触发器）：他主动找你吹牛。

    结尾 affinity +1 呼应「好感涨」——每来听一次，好感再涨，事件常来常新。
    """
    st = story_api.new_story("train_affinity", "练武场·四师兄吹牛会（好感≥3）")
    st.pop("mood", None)

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
            "text": "【旁白】练武场。四师兄见你路过，眼睛一亮，蹿了过来。",
            "mode": "narrative",
        },
    )
    n1.pop("character", None)
    node("scene", {"view": "center_evening"})
    # 修复练武场卡死：s_p（player 内心独白）引用 player，必须先 show 上台——
    # 未上台就 say/show 时 LoadCharacterPortrait 抛 KeyNotFoundException → 对话冻结
    node("show", {"character": "player", "position": "L2", "portrait": "normal"})
    node("show", {"character": "brother4", "position": "R1", "portrait": "laugh1"})
    s_b4 = say(
        "赵师弟！来得好不如来得巧！师兄我昨日新悟出一招「飞星赶月」，"
        "今日心情好，免费讲给你听！",
        "brother4",
        portrait="laugh1",
    )
    s_p = say(
        "（四师兄主动找我吹牛？太阳打西边出来了……）",
        "player",
        mode="think",
        portrait="doubt",
    )
    interrupt = say(
        "啊？！我、我这就去擦！赵师弟你先忙，咱们改日再论剑！",
        "brother4",
        portrait="nervous2",
        after=s_p,
    )
    end1 = node("end", {}, after=interrupt)
    aff = node("affinity", {"character": "brother4", "delta": 1}, after=s_p)
    listen = say(
        "听好了！这一招讲究腰马合一、气沉丹田——那晚我梦游追月，"
        "一脚踩空，人没飞起来，倒把厨房的瓦踩出个人形窟窿！",
        "brother4",
        portrait="laugh2",
        after=s_p,
    )
    story_api.add_choice(
        st,
        [
            ("愿闻其详", listen),
            ("师兄，掌门的供桌擦完了？", interrupt),
        ],
        after=s_p,
    )
    # 收尾：吹牛听得四师兄心花怒放，好感 +1（affinity 节点）后回自由模式
    story_api.update_node(st, aff, {"goto": end1})
    return st


def build_train_dusk(ed: dict) -> dict:
    """练武场·下旬晚练事件（when_stage=3 触发器）：撞见弟子们摸鱼。

    时间条件演示：只有旬=3（下旬）点击练武场才命中本脚本；其它旬走
    train_any 默认闲逛（或好感达标时的 train_affinity）。
    """
    st = story_api.new_story("train_dusk", "练武场·下旬晚练（旬=3）")
    st.pop("mood", None)

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
            "text": "【旁白】下旬的夜晚，你信步走到练武场——本该苦练的弟子们"
            "正三三两两开小差。",
            "mode": "narrative",
        },
    )
    n1.pop("character", None)
    node("scene", {"view": "center_night"})
    node("show", {"character": "trainee1", "position": "L1", "portrait": "normal"})
    # 修复练武场卡死：s_p（player 对话）引用 player，必须先 show 上台
    node("show", {"character": "player", "position": "R1", "portrait": "normal"})
    s_t = say(
        "赵、赵师兄！这么晚您怎么来了！",
        "trainee1",
        portrait="nervous1",
    )
    s_p = say("路过。倒是你们，晚课练完了？", "player")
    caught = say(
        "【旁白】你佯装没看见，踱去了伙房方向。弟子们长出一口气。",
        mode="narrative",
        after=s_p,
    )
    practice = say(
        "多谢赵师兄陪练！我们这就认真练！",
        "trainee1",
        portrait="laugh1",
        after=caught,
    )
    stat_n = node(
        "stat",
        {"key": "mental", "delta": 2, "waitDisplay": False},
        after=practice,
    )
    end1 = node("end", {}, after=stat_n)
    story_api.add_choice(
        st,
        [
            ("睁一只眼闭一只眼", caught),
            ("陪他们再练一轮", practice),
        ],
        after=s_p,
    )
    story_api.update_node(st, caught, {"goto": end1})
    return st


def build_train_any(ed: dict) -> dict:
    """练武场·默认闲逛事件（无条件触发器，优先级最低）。

    任意时间点击练武场都会命中（除非被前两个条件触发器抢先）。
    含两个新 branch 来源演示：source=stat（心相≥50 数值分支，else 落顺序
    下一节点）与 source=condition（官方条件检查点 S0030_01_001，真/假两路）。
    """
    st = story_api.new_story("train_any", "练武场·闲逛")
    st.pop("mood", None)

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
            "text": "【旁白】练武场今日没有旁的事。日头正好，正是闲逛的好时候。",
            "mode": "narrative",
        },
    )
    n1.pop("character", None)
    node("scene", {"view": "center"})
    node("show", {"character": "player", "position": "M", "portrait": "normal"})
    s_t = say("（来都来了，活动活动筋骨。）", "player", mode="think")

    # --- 目标数组顺序（after=s_t 逆序插入）：
    # [s_t, b_stat, b_cond, b_high, b_true, b_false, choice, stance, stat_n, wall, end1]
    end1 = node("end", {}, after=s_t)
    wall = say(
        "【旁白】你蹲在墙根，看弟子们把一套唐门刀法舞得虎虎生风。",
        mode="narrative",
        after=s_t,
    )
    stat_n = node(
        "stat",
        {"key": "mental", "delta": 1, "waitDisplay": False},
        after=s_t,
    )
    stance = say(
        "【旁白】你扎了半个时辰马步。腿很酸，心相很满。",
        mode="narrative",
        after=s_t,
    )
    choice = story_api.add_choice(
        st,
        [
            ("扎个马步，练练基础功", stance),
            ("蹲在墙根看人练功", wall),
        ],
        after=s_t,
    )["id"]
    b_false = say(
        "【condition 假】官方条件检查点 S0030_01_001 摇头了——"
        "branch（source=condition）假分支。",
        "player",
        after=s_t,
    )
    b_true = say(
        "【condition 真】官方条件检查点 S0030_01_001 点头了——"
        "branch（source=condition）真分支。",
        "player",
        after=s_t,
    )
    b_high = say(
        "【心相≥50 事件】心情正好，你顺手多打了两套拳——"
        "branch（source=stat）读到心相过关，编剧说这叫数值分支。",
        "player",
        after=s_t,
    )
    b_cond = node(
        "branch",
        {
            "source": "condition",
            "flag": "S0030_01_001",  # 官方条件检查点名（S0030_01.lua.txt 实证）
            "cases": [{"value": 1, "goto": b_true}, {"value": 2, "goto": b_false}],
        },
        after=s_t,
    )
    b_stat = node(
        "branch",
        {
            "source": "stat",
            "stat": "mental",  # 心相（editor_data stats 清单）
            "cases": [{"op": ">=", "value": 50, "goto": b_high}],
        },
        after=s_t,
    )
    story_api.update_node(st, b_high, {"goto": choice})
    story_api.update_node(st, b_true, {"goto": choice})
    story_api.update_node(st, b_false, {"goto": choice})
    story_api.update_node(st, stat_n, {"goto": end1})
    return st


# ---------------------------------------------------------------------------
# 自查
# ---------------------------------------------------------------------------
def validate_dice_check(
    ed: dict, raw: dict[str, str], check_id: str, expect_bands: int
) -> None:
    """硬性规则 2：骰子检查点必须在 dice_meta 中、非旅行检查点、调用点文件安全。"""
    meta = ed.get("dice_meta") or {}
    check(
        check_id in meta,
        "骰子检查点 %s 在 editor_data.dice_meta 中（缺元数据会致骰子菜单 NRE）"
        % check_id,
    )
    check(
        not check_id.startswith("Travel_"),
        "骰子检查点 %s 不是 Travel_* 旅行检查点" % check_id,
    )
    bands = (meta.get(check_id) or {}).get("bands") or []
    check(
        len(bands) == expect_bands,
        "骰子检查点 %s 结果带数 = %d（预期 %d）" % (check_id, len(bands), expect_bands),
    )
    # 官方调用点文件（checkpointmanager.Dice("check" ...)）
    callers = [
        f for f, c in raw.items() if 'checkpointmanager.Dice("%s"' % check_id in c
    ]
    check(
        bool(callers),
        "骰子检查点 %s 在官方脚本中有调用点：%s" % (check_id, ", ".join(callers)),
    )
    for cf in callers:
        stem = cf[: -len(".lua.txt")]
        is_travel = "travel" in stem.lower()
        check(not is_travel, "调用点文件 %s 不是旅行脚本（travel_*/*_travel*）" % cf)
        refs = [
            f
            for f, c in raw.items()
            if re.search(
                r'Set(?:Temp|CurrentTravel)Script\([^)]*"%s(_travel)?"'
                % re.escape(stem),
                c,
            )
        ]
        check(
            not refs,
            "调用点文件 %s 未被 SetCurrentTravelScript/SetTempScript 引用（引用自 %s）"
            % (cf, ", ".join(refs) or "无"),
        )


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
            return 'story=%s 节点 %s: 人物 "%s" 不在 editor_data.characters' % (
                story_id,
                n["id"],
                c,
            )
        used_chars.add(c)
        if c not in ("player", "brother4", "trainee1"):
            return (
                'story=%s 节点 %s: 人物 "%s" 超出建议范围（player/brother4/trainee1）'
                % (story_id, n["id"], c)
            )
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
                    err = '表情 "%s" 不是人物 %s 拥有的立绘差分' % (
                        n.get("portrait"),
                        n["character"],
                    )
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
                    err = '表情 "%s" 不是人物 %s 拥有的立绘差分' % (
                        n.get("portrait"),
                        char,
                    )
            elif t in ("stat", "stat_set"):
                if n["key"] not in stats:
                    err = "属性 %s 不在 editor_data.stats" % n["key"]
            elif t == "affinity":
                err = char_ok(n, sid)
                if not err and n["character"] not in affinity_chars:
                    err = (
                        "好感度人物 %s 不在 editor_data.affinity_characters"
                        % n["character"]
                    )
            elif t == "talent":
                if n["talent"] not in talents:
                    err = "天赋 %s 不在 editor_data.talents" % n["talent"]
            elif t == "item":
                if n["item"] not in items.get(n["kind"], set()):
                    err = "物品 %s 不在 editor_data.items_%s" % (n["item"], n["kind"])
            elif t == "game_flag":
                if n["flag"] not in game_flags:
                    err = (
                        "官方 flag %s 不在 editor_data.game_flags（游戏会静默忽略）"
                        % n["flag"]
                    )
            elif t == "choice":
                if n.get("dialog") != "Options":
                    err = "choice 皮肤必须是 Options（其它皮肤是自由场景 break 菜单，会崩）"
            if err:
                bad.append("story=%s 节点 %s(%s): %s" % (sid, n["id"], t, err))
    for b in bad:
        check(False, b)
    if not bad:
        print(
            "[通过] 清单校验：全部 music/effect/view/position/character/portrait/stat/talent/item/game_flag 均取自 editor_data.json"
        )
        print("[通过] 人物只使用：%s" % "、".join(sorted(used_chars)))


def validate_structure(ed: dict, raw: dict[str, str], stories: list[dict]) -> None:
    """transition 成对 / 末节点收尾 / mood 缺省 / 43 种节点覆盖 / 官方 Switch 名实证。"""
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
                        for m in nodes[i + 1 :]
                    )
                    check(
                        lifted,
                        "story=%s 节点 %s(transition in) 之后有成对 out（否则黑幕盖满全场）"
                        % (sid, n["id"]),
                    )
                else:
                    covered = any(
                        m.get("type") == "transition" and m.get("phase") == "in"
                        for m in nodes[:i]
                    )
                    check(
                        covered,
                        "story=%s 节点 %s(transition out) 之前有成对 in"
                        % (sid, n["id"]),
                    )
        last = nodes[-1]
        check(
            last["type"]
            in ("end", "death", "goto_scene", "raw", "choice", "branch", "dice"),
            "story=%s 末节点 %s(%s) 是合法收尾类型" % (sid, last["id"], last["type"]),
        )
        check(
            "mood" not in story,
            "story=%s 顶层未设 mood（默认 false=隐藏心情气泡）" % sid,
        )
        # choice 的每个 goto 都指向真实节点
        for n in nodes:
            if n["type"] == "choice":
                for o in n["options"]:
                    check(
                        any(m["id"] == o["goto"] for m in nodes),
                        "story=%s choice 节点 %s 的 goto=%s 指向真实节点"
                        % (sid, n["id"], o["goto"]),
                    )
        # 练武场卡死修复的防回归自检：say/show 引用的角色必须先 show 上台。
        # 未上台就 LoadCharacterPortrait 会抛 KeyNotFoundException → 对话冻结。
        # （graph 遍历按控制流顺序：无 goto 的非跳转节点隐式流向下一个）
        on_stage: set[str] = set()
        for i, n in enumerate(nodes):
            t = n["type"]
            if t == "show" and n.get("character"):
                on_stage.add(n["character"])
            elif t == "hide" and n.get("character"):
                on_stage.discard(n["character"])
            elif t in ("say", "shock", "focus", "offset", "move", "face", "intro", "rotate", "dim") and n.get("character"):
                check(
                    n["character"] in on_stage,
                    "story=%s 节点 %s(%s): 角色 %s 已上台（先 show 后引用，防 LoadCharacterPortrait KeyNotFound 冻结）"
                    % (sid, n["id"], t, n["character"]),
                )
            if t in ("choice", "branch", "dice", "end", "death", "goto_scene"):
                on_stage = set(on_stage)  # 汇合点：控制流分叉后无法静态推断，宽限处理
    # 43 种节点全覆盖（models.NODE_TYPES 即契约全量）
    missing = [t for t in story_api.models.NODE_TYPES if t not in all_types]
    check(not missing, "43 种节点全覆盖（缺失：%s）" % (", ".join(missing) or "无"))
    print("\n--- 节点覆盖清单（%d 种 / 契约 43 种）---" % len(all_types))
    for t in story_api.models.NODE_TYPES:
        cnt = all_types.get(t, 0)
        print("  %-14s %-12s ×%d" % (t, story_api.models.NODE_TYPE_CN.get(t, t), cnt))
    # branch source=game 的 Switch 名必须来自官方脚本实证
    switch_names: set[str] = set()
    for c in raw.values():
        switch_names.update(re.findall(r'checkpointmanager\.Switch\("([^"]+)"\)', c))
    check(
        SWITCH_GAME in switch_names,
        "branch source=game 的 Switch 名 %s 来自官方脚本实证" % SWITCH_GAME,
    )


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
                need_fallback = src == "game" or {c["value"] for c in n["cases"]} != {
                    1,
                    2,
                }
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
        check(
            not unreachable,
            "story=%s 可达性 %d/%d（100%%；不可达：%s）"
            % (
                story["id"],
                total - len(unreachable),
                total,
                ", ".join(unreachable) or "无",
            ),
        )


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
    train_affinity_story = build_train_affinity(ed)
    train_dusk_story = build_train_dusk(ed)
    train_any_story = build_train_any(ed)
    stories = [
        main_story,
        second_story,
        train_affinity_story,
        train_dusk_story,
        train_any_story,
    ]
    print(
        "[信息] main：%d 个节点；second：%d 个节点；"
        "train_affinity：%d；train_dusk：%d；train_any：%d"
        % (
            len(main_story["nodes"]),
            len(second_story["nodes"]),
            len(train_affinity_story["nodes"]),
            len(train_dusk_story["nodes"]),
            len(train_any_story["nodes"]),
        )
    )

    print("\n--- 2/5 清单校验（规则 4/7） ---")
    validate_catalog(ed, stories)

    print("\n--- 3/5 骰子检查点验证（规则 2） ---")
    validate_dice_check(ed, raw, DICE_3BAND, expect_bands=3)
    validate_dice_check(ed, raw, DICE_2BAND, expect_bands=2)

    print("\n--- 4/5 结构校验（transition 成对 / 末节点 / mood 缺省 / 43 种覆盖） ---")
    validate_structure(ed, raw, stories)

    print("\n--- 5/5 可达性自检 + story_api 全量校验编译 ---")
    validate_reachability(stories)
    say_count = 0
    for story in stories:
        for n in story["nodes"]:
            # texts.json 只收 say：death 文本由 mod_set_death_text 直接进 Lua
            if n["type"] == "say":
                say_count += 1
        errors, warnings = story_api.check_story(story)
        check(
            not errors,
            "story=%s check_story 无错误（%s）"
            % (story["id"], "; ".join(errors) or "0 条"),
        )
        for w in warnings:
            check(False, "story=%s 编译警告不应出现：%s" % (story["id"], w))
        lua, cerrs, cwarns = story_api.compile_story(story)
        check(
            lua is not None and not cerrs,
            "story=%s compile_story 成功（%s）"
            % (story["id"], "; ".join(cerrs) or "0 条"),
        )
        for w in cwarns:
            check(False, "story=%s 编译警告不应出现：%s" % (story["id"], w))
    print(
        "[信息] say 节点共 %d 个 → 打包后 texts.json 应恰好 %d 条（death 文本不进 texts.json）"
        % (say_count, say_count)
    )

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
    print(
        "\n构建成功：全部自查通过（43 种节点、骰子检查点、可达性 100%、5 个剧情脚本）。"
    )
    print(
        "下一步：PYTHONPATH=compiler python -m lomc pack samples/showcase -o samples/showcase.lommod"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
