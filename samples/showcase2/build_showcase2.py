#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""全节点演示 2.0·九姝篇 构建脚本（samples/showcase2/build_showcase2.py）。

宣传视频用 mod：九位可攻略女主（含未开放攻略的崆峒四姝与上官萤）全员登场，
逐一演示 43 种节点；台词按《活侠传_数据总表.xlsx》人物档案与官方对话脚本
的语气仿写（虞小梅的「阿活～」、夏侯兰的「为师」、上官萤的商会算盘等）。
夏玉莲（girl3，官方连立绘都没有）以「自定义人物介绍卡」+ 占位立绘客串，
顺带演示 intro_source=custom 新功能。

阵容与分工：
  第一幕 main —— 唐默铃(开场/舞台) 叶云裳(转场音效) 龙湘(高价值节点)
                 虞小梅(数值 12 连+mod 分支) 夏侯兰(game 分支) 郁竹(面板/任务)
                 上官萤(介绍卡/choice) 魏菊(三带骰子+收尾三选)
  第二幕 second —— 瑞杏(二带骰子回环/死亡文本/结局卡)
  触发器×3 —— 虞小梅好感≥3 / 下旬郁竹晚锻 / 小师妹默认闲逛(stat+condition 分支)

一切剧情构建都经 story_api（editor/story_api.py，AI 与编辑器共用的受控写
入口），不手写 story JSON / Lua；随后用 CLI 校验/编译/打包：
    editor/.venv/Scripts/python samples/showcase2/build_showcase2.py   # 生成 story+manifest
    cd editor && .venv/Scripts/python story_api.py check ../samples/showcase2/story/main.json
    .venv/Scripts/python story_api.py pack ../samples/showcase2 -o ../samples/全节点演示2.0.lommod
"""

# pyright: reportMissingImports=false
from __future__ import annotations

import json
import sys
from pathlib import Path

for _stream in (sys.stdout, sys.stderr):
    _reconfigure = getattr(_stream, "reconfigure", None)
    if _reconfigure is not None:
        _reconfigure(encoding="utf-8", errors="replace")

PROJECT_ROOT = Path(__file__).resolve().parents[2]
EDITOR_DIR = PROJECT_ROOT / "editor"
if str(EDITOR_DIR) not in sys.path:
    sys.path.insert(0, str(EDITOR_DIR))
import models  # type: ignore[reportMissingImports]  # noqa: E402
import story_api  # type: ignore[reportMissingImports]  # noqa: E402

SHOWCASE_DIR = Path(__file__).resolve().parent
STORY_DIR = SHOWCASE_DIR / "story"

MOD_ID = "showcase2"
MANIFEST = {
    "format": 1,
    "id": MOD_ID,
    "name": "全节点演示2.0·九姝篇",
    "version": "2.0.0",
    "author": "lom_modkit",
    "description": (
        "宣传视频用：九位可攻略女主（含未开放攻略的崆峒四姝、上官萤与"
        "连立绘都没有的夏玉莲）全员登场，演示全部 43 种节点、表情差分、"
        "自定义人物介绍卡、三向骰子、分支、战役与练武场条件触发器"
    ),
    "entry": "main",
    "campaign": {
        "new_game": True,
        "disable_official_events": True,
        "triggers": [
            {
                "type": "position",
                "position": "Center",
                "script": "train_affinity",
                "when_affinity": {"character": "girl5", "min": 3},
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

# 与 1.0 相同的官方骰子检查点（1.0 已实证安全）
DICE_3BAND = "Ch_1_1_1_001"  # 3 带：<=30 / <=60 / >60
DICE_2BAND = "Talk_Brother4_02_01_01_001"  # 2 带：<50 / >=50
SWITCH_GAME = "Ch_1_1_1_001"  # branch source=game 官方 Switch 名

ALL_TYPES = [
    "music", "sound", "scene", "show", "move", "face", "hide", "focus",
    "offset", "say", "choice", "shock", "mask", "intro", "effect",
    "transition", "camera", "block", "cg", "dim", "message", "rotate",
    "dayenv", "stat", "stat_set", "affinity", "talent", "item", "flag",
    "game_flag", "enemy", "battle_skill", "mission", "time", "autosave",
    "branch", "dice", "goto_scene", "panel", "wait", "end", "death", "raw",
]


def _helpers(st):
    def say(text, character=None, mode="character", portrait="normal", after=None):
        return story_api.add_say(
            st, text, character=character, mode=mode, portrait=portrait, after=after
        )["id"]

    def node(t, fields=None, after=None):
        return story_api.add_node(st, t, fields, after=after)["id"]

    return say, node


def build_main(ed: dict) -> dict:
    """第一幕：八位女主接力，演出/数值/流程全演示，末尾三带骰子定去路。"""
    st = story_api.new_story("main", "全节点演示2.0·九姝篇 第一幕")
    st.pop("mood", None)  # 顶层不设 mood（默认 false=隐藏官方心情气泡）
    say, node = _helpers(st)

    # --- 开场：唐默铃（小师妹）——音乐/场景/出场/舞台动作 ---
    # new_story 模板自带 n1=show/n2=say：删掉 n1，旁白先行，切景后再登场
    story_api.delete_node(st, "n1")
    n2 = story_api.update_node(
        st,
        "n2",
        {
            "text": "唐门校场，风和日丽。",
            "mode": "narrative",
        },
    )
    n2.pop("character", None)
    st["start"] = "n2"
    say("赵活今日眼皮直跳——总觉得今天要出事。", mode="narrative")
    node("music", {"name": "古怪_001"})
    node("scene", {"view": "center"})
    node("show", {"character": "player", "position": "M", "portrait": "normal"})
    say("宣传视频？拍我干嘛，我这张脸又卖不出去……", "player", portrait="nervous1")
    node("show", {"character": "sister1", "position": "R1", "facing": "left", "portrait": "normal"})
    say("……师兄，早。", "sister1")
    say("小师妹？！连你也被抓来演示了？", "player", portrait="shock")
    say("嗯。", "sister1", portrait="close_eye_laugh")
    say("编剧说，女主先行。", "sister1", portrait="close_eye_laugh")
    say("小师妹还是老样子，三句话不超过十个字。", "player", mode="think")
    say("校场中央，日影偏了一寸。", mode="center")
    node("move", {"character": "player", "from": "M", "to": "L2", "duration": 1.2})
    node("face", {"character": "sister1", "facing": "left"})
    node("focus", {"character": "sister1"})
    say("赵活往旁边挪了一步，镜头跟着小师妹转过去。", mode="narrative")
    say("师兄，你挡我折纸了。", "sister1")
    node("shock", {"character": "sister1", "duration": 0.4})
    say("……哇。", "sister1", portrait="shock")
    say("连被摇都面无表情吗？！", "player", portrait="nervous2")
    node("offset", {"character": "player", "x": 20, "y": -10, "duration": 0.4})
    say("我、我又被弹飞了？！", "player", portrait="suck1")
    node("effect", {"name": "Hit_001", "x": 10, "y": -5})
    node("camera", {"name": "stage-memory", "active": True})
    say("回忆滤镜一开，这张丑脸都朦胧出了几分诗意。", "player", mode="think")
    node("camera", {"name": "stage-memory", "active": False})
    say("……我去折纸了。", "sister1")
    say("师兄，加油。", "sister1")

    # --- 叶云裳：转场/音效/通用块/字卡/官方介绍卡/遮罩 ---
    node("transition", {"phase": "in", "dir": "lr"})
    node("hide", {"character": "sister1", "fadeDuration": 0.3})  # 黑幕中退场
    node("scene", {"view": "backmountain2"})
    node("transition", {"phase": "out", "dir": "lr"})
    node("show", {"character": "girl2", "position": "R1", "facing": "left", "portrait": "laugh1"})
    say("赵哥哥～黑幕一拉一合，我们就从校场瞬移到后山啦。", "girl2", portrait="laugh1")
    say("云裳身子弱，慢些走……不对，你怎么比我还精神？", "player")
    say("人家一高兴，发烧都退了三分。对了对了，听——", "girl2")
    node("sound", {"name": "鈴鐺_001"})
    say("铃铛声？小师妹的铃铛从来不响，这串是哪个的？", "player", portrait="doubt")
    node("sound", {"name": "鳥_001", "kind": "env"})
    say("环境音起。鸟鸣山更幽，适合野餐，不适合练武。", mode="narrative")
    node("sound", {"name": "鳥_001", "kind": "env", "op": "fadeout", "seconds": 1})
    node("block", {"flowchart": "common", "name": "flash"})
    node("block", {"flowchart": "common", "name": "shake"})
    say("白屏加地震！通用块 flash、shake——嘻嘻，好玩吧？", "girl2", portrait="laugh2")
    node("cg", {"action": "show", "kind": "title", "key": "P_Back_01"})
    say("标题字卡高悬——这一景，名唤「後山」。", mode="center")
    say("接下来是人家的官方资料卡，睁大眼睛看好了。", "girl2")
    node("intro", {"character": "girl2"})
    say("点苍明珠，叶云裳——虽然走快两步就喘，但可爱是不打折的。", "girl2", portrait="shy")
    node("mask", {"show": True})
    say("（mask 遮罩下的独白）……其实人家下山，是想看看山下的糖葫芦。", "girl2", mode="think")
    node("mask", {"show": False})
    node("hide", {"character": "girl2", "fadeDuration": 0.5})
    say("云裳消失了？！", "player", portrait="shock")
    node("show", {"character": "girl2", "position": "L1", "portrait": "laugh1"})
    say("骗你的～hide 再 show，人家换个表情就回来了。", "girl2", portrait="laugh1")
    node("hide", {"character": "girl2", "fadeDuration": 0.4})  # 叶云裳退场

    # --- 龙湘：dim/message/rotate/dayenv ---
    node("show", {"character": "girl4", "position": "R1", "facing": "left", "portrait": "normal"})
    say("让一让！轮到我了——在下锦香宫龙湘，剑法……大概、可能、还算不错。", "girl4")
    say("湘姐客气。听说你昨天又把问路的骗去了后山深处？", "player")
    say("那是他们自己不认路！看招——", "girl4", portrait="angry1")
    node("dim", {"character": "player", "dimmed": True})
    say("dim 节点——弟，你被压暗了。主角让戏，女侠高光。", "girl4", portrait="laugh1")
    node(
        "message",
        {"text": "【系统提示】message 节点：系统提示条原文直出（DisplayMessageText）。"},
    )
    say("系统提示条也归我们调用——这可是宣传视频的重点。", "girl4")
    node("rotate", {"character": "girl4", "angle": -10, "duration": 0.3})
    say("rotate？！我的剑歪了——不是我走位歪，是节点歪！", "girl4", portrait="shock")
    node("rotate", {"character": "girl4", "angle": 10, "duration": 0.3})
    say("又正回来了。角度在前、时长在后，官方参数序。", "player")
    node("dayenv", {"day_type": 1})
    say("日夜一转，校场上的影子矮了一截。", mode="narrative")
    node("dim", {"character": "player", "dimmed": False})
    node("hide", {"character": "girl4", "fadeDuration": 0.4})

    # --- 虞小梅：数值状态 12 连 + flag + branch(source=mod) ---
    node("show", {"character": "girl5", "position": "R1", "facing": "left", "portrait": "laugh1"})
    say("阿活～到我了到我了！本梅给你带了好多礼物哦。", "girl5", portrait="laugh1")
    say("小梅，你笑得这么甜，我怎么有点发毛……", "player", portrait="nervous1")
    node("stat", {"key": "mental", "delta": 5})
    node("stat", {"key": "money", "delta": 50})
    node("stat_set", {"key": "talking", "value": 66})
    node("affinity", {"character": "girl5", "delta": 1})
    node("talent", {"talent": "1010", "level": 1})
    node("item", {"kind": "misc", "item": "2001", "count": 2})
    node("item", {"kind": "book", "item": "2001", "count": 1})
    node("item", {"kind": "misc", "item": "2001", "count": 1, "remove": True})
    node("item", {"kind": "special", "item": "2001", "count": 1})
    say(
        "心相、银两、嘴力、好感、天赋，黄酒两瓶、剑谱一本、回收一瓶、"
        "再送你个机巧小玩意儿——一共十二连，数清楚了吗？",
        "girl5",
        portrait="laugh2",
    )
    say("回收的那瓶，你没下毒吧？", "player", portrait="doubt")
    say("讨厌，人家是那种人吗？（笑）火闪电，你说是不是？", "girl5", portrait="laugh1")
    node("flag", {"flag": "SHOWCASE2_MET_GIRLS"})
    s_flag_say = say(
        "一枚看不见的印记落下——「见过她们了」。",
        mode="narrative",
    )

    merge_a = say("【汇合点】mod 分支两条路在此相会。", "girl5", after=s_flag_say)
    bm2 = say(
        "branch（source=mod）case 2：flag 未设置才会来这（静态可达，演出中走不到）。",
        "player",
        after=s_flag_say,
    )
    bm1 = say(
        "branch（source=mod）case 1：flag 已设置——此路必通，跟紧本梅哦。",
        "girl5",
        after=s_flag_say,
    )
    node(
        "branch",
        {
            "flag": "SHOWCASE2_MET_GIRLS",
            "source": "mod",
            "cases": [{"value": 1, "goto": bm1}, {"value": 2, "goto": bm2}],
        },
        after=s_flag_say,
    )
    story_api.update_node(st, bm1, {"goto": merge_a})
    story_api.update_node(st, bm2, {"goto": merge_a})
    node("hide", {"character": "girl5", "fadeDuration": 0.4})  # 虞小梅退场

    # --- 夏侯兰：game_flag + branch(source=game) ---
    node("show", {"character": "girl6", "position": "R1", "facing": "left", "portrait": "normal"})
    say("吵吵闹闹，成何体统。弟子，轮到为师讲课了。", "girl6")
    say("师父？！您怎么也来了……", "player", portrait="nervous2")
    say("哼，宣传片少了为师，像什么话。看仔细，这是官方任务旗标。", "girl6", portrait="angry1")
    node("game_flag", {"flag": "50019", "value": 1, "op": "set"})
    s_gf_say = say(
        "官方名册上，多了一笔为师的课业印记。",
        mode="narrative",
    )
    merge_b = say(
        "官方检查点分支：读不到或读到 3，走 else 兜底落到这里。孺子可教。",
        "girl6",
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
    story_api.update_node(st, bg1, {"goto": merge_b})
    story_api.update_node(st, bg2, {"goto": merge_b})
    say("branch 四种来源，为师今日先讲两种；剩下两种，去练武场触发器里找。", "girl6")
    node("hide", {"character": "girl6", "fadeDuration": 0.4})

    # --- 郁竹：enemy/battle_skill/mission/time/autosave/panel/wait/music 变体 ---
    node("show", {"character": "girl7", "position": "R1", "facing": "left", "portrait": "normal"})
    say("……别盯着我看，赵郎。我眯着眼不是瞪你，是眼力不好。", "girl7", portrait="angry1")
    say("小竹！听说你新打了一把好刀？", "player")
    say("嗯。给你的。……别多想，是顺手打的，不是特意。", "girl7", portrait="shy")
    node("enemy", {"op": "team", "enemy": "400", "value": -10})
    node("battle_skill", {"op": "set", "key": "special3", "index": 2})
    node("mission", {"name": "Main", "key": "M0001"})
    node("time", {"op": "round"})
    node("autosave", {"kind": "story", "save_button": 1})
    say(
        "锻造锤一挥：敌势削弱、战场技能上膛、主线往前一格、时辰过旬，顺手存了一档。",
        mode="narrative",
    )
    node("panel", {"panel": "martial", "mode": 0})
    say("武学面板被强制打开。看完记得关——不然铁铺要着火。", "girl7", portrait="nervous1")
    node("wait", {"seconds": 0.8})
    node("music", {"name": "古怪_001", "op": "fadeout", "seconds": 2})
    node("music", {"name": "快樂_001"})
    say("音乐淡出再切新曲，跟拉风箱一个道理，得匀。", "girl7")
    node("music", {"name": "快樂_001", "op": "stop"})
    say("立刻 stop——戛然而止。play、fadeout、stop 三变体齐活。", "player")
    node("hide", {"character": "girl7", "fadeDuration": 0.4})

    # --- 上官萤：choice 皮肤 + 自定义人物介绍卡（夏玉莲客串） ---
    node("show", {"character": "girl9", "position": "R1", "facing": "left", "portrait": "normal"})
    say("都让让。赵郎，前面那些都是花销，接下来该算算账了。", "girl9")
    say("萤儿！连宣传片你都要谈生意？", "player", portrait="nervous1")
    say(
        "当然。choice 选项菜单、介绍卡，哪样不是门面？我们上官家的门面，从不马虎。",
        "girl9",
        portrait="laugh1",
    )
    say("对了，给诸位介绍一位……特别的客人。", "girl9")
    node(
        "intro",
        {
            "intro_source": "custom",
            "title": "传说中的第十位女主",
            "name": "夏玉莲（暂）",
            "text": "官方数据总表里有名有姓，却连一张立绘都没有的隐藏人物。\n"
            "尚未实装，身价待定——这张介绍卡与立绘，皆由 mod 自定义。",
            "image": "assets/xia_yulian.png",
        },
    )
    say(
        "看见没？custom 介绍卡：称号、姓名、正文、立绘全可自定义。"
        "未实装的女主，也能先登场造势。",
        "girl9",
        portrait="revel",
    )
    say("（连不存在的人都能请上台……上官家的手段，可怕。）", "player", mode="think")
    node("stat", {"key": "fate", "delta": 2})
    say("行善攒下两点命运。掷骰子时，它可以逆天改命。", mode="narrative")
    node("hide", {"character": "girl9", "fadeDuration": 0.4})  # 上官萤退场

    # --- 魏菊：三带骰子 + 收尾三选 ---
    node("show", {"character": "girl8", "position": "R1", "facing": "left", "portrait": "normal"})
    say("诸位闹够了，该收个尾了。", "girl8")
    say("赵郎，小菊出个对子助兴。", "girl8")
    say("小菊出题？我对不上来……", "player", portrait="nervous2")
    say("无妨。胜负交给骰子。", "girl8", portrait="laugh1")
    s_last = say("赵郎，请。", "girl8", portrait="laugh2")

    # --- 结尾分支块（全部 after=s_last，逆序插入得到目标数组顺序） ---
    gs_free = node("goto_scene", {"scene": "Free"}, after=s_last)
    gv = say("我不掷了。告辞。", "player", after=s_last)
    fail_ju = say("……对不上，也挺有趣。", "girl8", portrait="laugh1", after=s_last)
    fail_me = say("……我对不上。", "player", portrait="suck2", after=s_last)
    fail_lot = say("签词：「擀面杖吹火，一窍不通。」", mode="narrative", after=s_last)
    time_set = node(
        "time", {"op": "set", "year": 1, "month": 4, "stage": 3}, after=s_last
    )
    end2 = node("end", {"next_script": "second"}, after=s_last)
    fin_c = story_api.add_choice(
        st,
        [
            ("去练武场看看", gs_free),
            ("先睡到下月下旬", time_set),
            ("继续第二幕", end2),
        ],
        after=s_last,
    )["id"]
    aff = node("affinity", {"character": "girl5", "delta": 3}, after=s_last)
    suc_note = say("一旁的小梅多看了你一眼。", mode="narrative", after=s_last)
    suc = say("稳稳当当，不算难看。", "girl8", portrait="laugh1", after=s_last)
    suc_lot = say("签词：「平平仄仄，稳稳当当。」", mode="narrative", after=s_last)
    big = say("第二幕，杏花仙有请。", "girl8", portrait="laugh2", after=s_last)
    big_ju = say("好。这一卦乾坤已定。", "girl8", portrait="laugh2", after=s_last)
    big_lot = say("签词：「一掷乾坤定，红颜尽展颜。」", mode="narrative", after=s_last)
    d3 = story_api.add_dice(
        st,
        DICE_3BAND,
        goto_成功=suc_lot,
        goto_失败=fail_lot,
        goto_大成功=big_lot,
        band_texts=[
            "擀面杖吹火——一窍不通",
            "平平仄仄——稳稳当当",
            "一掷乾坤定——红颜尽展颜",
        ],
        after=s_last,
    )["id"]
    story_api.add_choice(
        st,
        [
            ("掷骰子", d3),
            ("不掷了", gv),
        ],
        after=s_last,
    )
    node(
        "raw",
        {
            "code": "-- [raw 逃逸口演示] 原生 Lua 原样插入编译产物（本行仅为注释）"
        },
        after=s_last,
    )

    # 切场前下场：直接挂到数组末尾，不走 add_node（避免线性防线误补 show）
    def _bare_hide(goto: str) -> str:
        nid = models.make_node_id(st)
        st.setdefault("nodes", []).append(
            {
                "id": nid,
                "type": "hide",
                "character": "girl8",
                "fadeDuration": 0.3,
                "goto": goto,
            }
        )
        return nid

    hide_second = _bare_hide(end2)
    hide_free = _bare_hide(gs_free)
    story_api.update_node(st, big_lot, {"goto": big_ju})
    story_api.update_node(st, big_ju, {"goto": big})
    story_api.update_node(st, big, {"goto": hide_second})
    story_api.update_node(st, suc_lot, {"goto": suc})
    story_api.update_node(st, suc, {"goto": suc_note})
    story_api.update_node(st, suc_note, {"goto": aff})
    story_api.update_node(st, aff, {"goto": fin_c})
    story_api.update_node(st, fail_lot, {"goto": fail_me})
    story_api.update_node(st, fail_me, {"goto": fail_ju})
    story_api.update_node(st, fail_ju, {"goto": fin_c})
    story_api.update_node(st, gv, {"goto": fin_c})
    story_api.update_node(st, time_set, {"goto": hide_free})
    for n in st["nodes"]:
        if n.get("id") == fin_c:
            for opt in n.get("options") or []:
                if opt.get("goto") == gs_free:
                    opt["goto"] = hide_free
                elif opt.get("goto") == end2:
                    opt["goto"] = hide_second
    return st


def build_second(ed: dict) -> dict:
    """第二幕：瑞杏（杏花仙）主持——二带骰子回环 + 死亡文本 + 结局卡。"""
    st = story_api.new_story("second", "全节点演示2.0·第二幕（杏花仙加演）")
    st.pop("mood", None)
    say, node = _helpers(st)

    story_api.delete_node(st, "n1")  # 模板自带 show 登场改为后置
    n2 = story_api.update_node(
        st,
        "n2",
        {
            "text": "第二幕。主剧本一个收尾，就把你送到了这里。",
            "mode": "narrative",
        },
    )
    n2.pop("character", None)
    st["start"] = "n2"
    node("scene", {"view": "cliff_night"})
    node("show", {"character": "player", "position": "M", "portrait": "normal"})
    node("music", {"name": "陰森_001"})
    say("悬崖？！小菊的上签不是说我乾坤已定吗，怎么定到悬崖边上了！", "player")
    node("show", {"character": "girl1", "position": "R1", "facing": "left", "portrait": "laugh1"})
    say("呵呵……赵君，骰子掉进深谷，可就捡不回来了。", "girl1", portrait="laugh1")
    say("杏、杏花仙？！你什么时候站在那里的？", "player", portrait="shock")
    say("本宫一直都在。来，最后一局——二带检定，只有成败，没有大成功。", "girl1", portrait="special1")
    s_last = say(
        "（她笑得越温柔，我越觉得这悬崖深不见底……）",
        "player",
        mode="think",
        portrait="nervous1",
    )

    # --- 失败支线：安慰后二选一，可回环重试 ---
    fail2 = say(
        "【失败】手一抖，骰子打着旋儿坠下深谷……",
        "player",
        portrait="suck2",
        after=s_last,
    )
    f_say = say(
        "哎呀。掉的是骰子，不是你——要再来一局吗？本宫陪你。",
        "girl1",
        portrait="laugh2",
        after=fail2,
    )
    death_fail = story_api.add_death(
        st,
        "杏花树下，一枕黄粱",  # 自定义死亡文本
        death_id="910021",
        next="Title",
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

    # --- 成功支线：二选一演示死亡文本与结局卡片 ---
    suc2 = say(
        "【成功】我稳稳接住了骰子！杏花仙鼓掌：「恭喜通关，请选择你的谢幕方式。」",
        "player",
        portrait="laugh1",
        after=death_fail,
    )
    death_suc = story_api.add_death(
        st,
        "杏花树下，一枕黄粱",
        death_id="910021",
        next="Title",
        title="坠崖谢幕",
        after=suc2,
    )["id"]
    end_demo = node(
        "goto_scene",
        {
            "scene": "End",
            "key": "920047",  # mod 专属结局 id（9+官方 20047 武林传奇）
            "next": "Title",
            "title": "九姝篇·武林传奇",
            "desc": "耕阳读书斋将你的事迹编撰成书，发行于市。\n"
            "唐门活侠与九位女侠的宣传佳话，一朝传遍中原。",
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

    story_api.add_dice(
        st,
        DICE_2BAND,
        goto_成功=suc2,
        goto_失败=fail2,
        goto_大成功="",
        band_texts=[
            "手一抖，骰子向深谷飞去——（失败）",
            "稳如泰山，骰子稳稳接住！（成功）",
        ],
        after=s_last,
    )
    return st


def build_train_affinity(ed: dict) -> dict:
    """练武场·虞小梅好感≥3 事件（when_affinity 触发器）：她拉你去散步。"""
    st = story_api.new_story("train_affinity", "练武场·小梅的邀约（好感≥3）")
    st.pop("mood", None)
    say, node = _helpers(st)

    story_api.delete_node(st, "n1")  # 模板自带 show 登场改为后置
    n2 = story_api.update_node(
        st,
        "n2",
        {
            "text": "练武场。虞小梅提着裙摆，笑吟吟地小跑过来。",
            "mode": "narrative",
        },
    )
    n2.pop("character", None)
    st["start"] = "n2"
    node("scene", {"view": "center_evening"})
    node("show", {"character": "player", "position": "L2", "portrait": "normal"})
    node("show", {"character": "girl5", "position": "R1", "facing": "left", "portrait": "laugh1"})
    s_g5 = say("阿活～好感度攒够三点啦！奖励你陪本梅去后山散步。", "girl5", portrait="laugh1")
    s_p = say("（小梅今天心情这么好，葫芦里卖的什么药……）", "player", mode="think", portrait="doubt")
    go = say(
        "太好啦！走吧走吧，火闪电已经在前面带路了。（好感 +1）",
        "girl5",
        portrait="laugh2",
        after=s_p,
    )
    busy = say(
        "哎，好吧。那本梅就在这儿看你练拳，一样开心。",
        "girl5",
        portrait="nervous1",
        after=s_p,
    )
    aff = node("affinity", {"character": "girl5", "delta": 1}, after=go)
    end1 = node("end", {}, after=busy)
    story_api.add_choice(
        st,
        [
            ("陪她去（盛情难却）", go),
            ("下次吧，今日功课未了", busy),
        ],
        after=s_p,
    )
    story_api.update_node(st, aff, {"goto": end1})
    story_api.update_node(st, busy, {"goto": end1})
    return st


def build_train_dusk(ed: dict) -> dict:
    """练武场·下旬晚锻事件（when_stage=3 触发器）：郁竹在打铁。"""
    st = story_api.new_story("train_dusk", "练武场·下旬晚锻（旬=3）")
    st.pop("mood", None)
    say, node = _helpers(st)

    story_api.delete_node(st, "n1")  # 模板自带 show 登场改为后置
    n2 = story_api.update_node(
        st,
        "n2",
        {
            "text": "下旬的夜晚，练武场一角叮当作响——郁竹还在打铁。",
            "mode": "narrative",
        },
    )
    n2.pop("character", None)
    st["start"] = "n2"
    node("scene", {"view": "center_night"})
    node("show", {"character": "girl7", "position": "L1", "portrait": "normal"})
    node("show", {"character": "player", "position": "R1", "facing": "left", "portrait": "normal"})
    s_t = say("……看什么看，赵郎。晚上没人，我才来打会儿铁。", "girl7", portrait="angry1")
    s_p = say("小竹，下旬了还不休息？你这锤子比我的剑都沉。", "player")
    watch = say(
        "【旁白】你坐在一旁看她抡锤。火星四溅，倒也好看。",
        mode="narrative",
        after=s_p,
    )
    help_ = say(
        "给、给你搭把手……哇这锤子真沉！（心相 +2）",
        "player",
        portrait="suck1",
        after=s_p,
    )
    stat_n = node("stat", {"key": "mental", "delta": 2, "waitDisplay": False}, after=help_)
    end1 = node("end", {}, after=watch)
    story_api.add_choice(
        st,
        [
            ("坐着看会儿", watch),
            ("上去搭把手", help_),
        ],
        after=s_p,
    )
    story_api.update_node(st, stat_n, {"goto": end1})
    story_api.update_node(st, watch, {"goto": end1})
    return st


def build_train_any(ed: dict) -> dict:
    """练武场·默认闲逛事件（无条件触发器）：小师妹 + branch stat/condition 演示。"""
    st = story_api.new_story("train_any", "练武场·闲逛")
    st.pop("mood", None)
    say, node = _helpers(st)

    story_api.delete_node(st, "n1")  # 模板自带 show 登场改为后置
    n2 = story_api.update_node(
        st,
        "n2",
        {
            "text": "练武场今日没有旁的事。小师妹蹲在廊下折纸。",
            "mode": "narrative",
        },
    )
    n2.pop("character", None)
    st["start"] = "n2"
    node("scene", {"view": "center"})
    node("show", {"character": "sister1", "position": "L1", "portrait": "normal"})
    node("show", {"character": "player", "position": "R1", "facing": "left", "portrait": "normal"})
    say("……师兄。", "sister1")
    s_t = say("（陪她待会儿，还是自己活动活动？）", "player", mode="think")

    # 目标数组顺序（after=s_t 逆序插入）：
    # [s_t, b_stat, b_cond, b_high, b_true, b_false, choice, paper, stat_n, stance, end1]
    end1 = node("end", {}, after=s_t)
    stance = say(
        "【旁白】你扎了半个时辰马步。腿很酸，心相很满。（心相 +1）",
        mode="narrative",
        after=s_t,
    )
    stat_n = node("stat", {"key": "mental", "delta": 1, "waitDisplay": False}, after=stance)
    paper = say(
        "小师妹把折好的纸鹤放进你掌心。……今天会是很好的一天。",
        "sister1",
        portrait="close_eye_laugh",
        after=s_t,
    )
    choice = story_api.add_choice(
        st,
        [
            ("陪她折纸", paper),
            ("扎个马步，练练基础功", stance),
        ],
        after=s_t,
    )["id"]
    b_false = say(
        "【condition 假】官方条件检查点 S0030_01_001 摇头了——branch（source=condition）假分支。",
        "player",
        after=s_t,
    )
    b_true = say(
        "【condition 真】官方条件检查点 S0030_01_001 点头了——branch（source=condition）真分支。",
        "player",
        after=s_t,
    )
    b_high = say(
        "【心相≥50 事件】心情正好，多打两套拳——branch（source=stat）数值分支。",
        "player",
        after=s_t,
    )
    b_cond = node(
        "branch",
        {
            "source": "condition",
            "flag": "S0030_01_001",
            "cases": [{"value": 1, "goto": b_true}, {"value": 2, "goto": b_false}],
        },
        after=s_t,
    )
    b_stat = node(
        "branch",
        {
            "source": "stat",
            "stat": "mental",
            "cases": [{"op": ">=", "value": 50, "goto": b_high}],
        },
        after=s_t,
    )
    story_api.update_node(st, b_high, {"goto": choice})
    story_api.update_node(st, b_true, {"goto": choice})
    story_api.update_node(st, b_false, {"goto": choice})
    story_api.update_node(st, paper, {"goto": end1})
    story_api.update_node(st, stat_n, {"goto": end1})
    return st


# ---------------------------------------------------------------------------
# 构建与自查
# ---------------------------------------------------------------------------
# 引用人物时必须已在台上的节点类型（否则游戏 LoadCharacterPortrait /
# CharacterPlaceholder.Get 抛异常 → 剧情协程死 → 黑屏/冻结）
_STAGE_REQUIRED = (
    "say", "move", "face", "hide", "focus", "offset", "shock", "dim", "rotate",
)


def check_stage_safety(st: dict) -> list[str]:
    """逐节点推演：引用人物的节点，其作用对象在该节点生效前必须已在台上。"""
    import preview  # noqa: E402  （editor/，纯推演函数，不需要 Qt 事件循环）

    problems: list[str] = []
    for n in st.get("nodes", []):
        if n.get("type") not in _STAGE_REQUIRED:
            continue
        cid = n.get("character")
        if not cid:
            continue  # narrative/center 旁白无人物
        state = preview.simulate_stage(st, n["id"], include_target=False)
        if not state.get("reached"):
            continue  # 默认路线走不到的分支，不能拿走完后的台上状态来误判
        if cid not in (state.get("actors") or {}):
            problems.append(
                '节点 %s(%s): 人物 "%s" 此时不在台上（缺 show 或已被 hide）'
                % (n["id"], n["type"], cid)
            )
    return problems


def main() -> None:
    ed, is_fallback = story_api.load_editor_data()
    if is_fallback:
        raise SystemExit("data/editor_data.json 不可用（走了兜底数据），拒绝构建")

    builders = {
        "main": build_main,
        "second": build_second,
        "train_affinity": build_train_affinity,
        "train_dusk": build_train_dusk,
        "train_any": build_train_any,
    }
    STORY_DIR.mkdir(parents=True, exist_ok=True)
    used: set[str] = set()
    for story_id, builder in builders.items():
        st = builder(ed)
        errors, warnings = story_api.check_story(st)
        for w in warnings:
            print("[警告] %s: %s" % (story_id, w))
        stage_problems = check_stage_safety(st)
        for p in stage_problems:
            print("[失败] %s: %s" % (story_id, p))
        errors = errors + stage_problems
        if errors:
            raise SystemExit("校验失败 %s:\n%s" % (story_id, "\n".join(errors)))
        for n in st["nodes"]:
            used.add(n["type"])
        story_api.save_story_json(st, STORY_DIR / ("%s.json" % story_id))
        print("[通过] story/%s.json（%d 节点）" % (story_id, len(st["nodes"])))

    missing = [t for t in ALL_TYPES if t not in used]
    if missing:
        raise SystemExit("43 种节点未全覆盖，缺：%s" % ", ".join(missing))
    print("[通过] 43 种节点全覆盖")

    (SHOWCASE_DIR / "manifest.json").write_text(
        json.dumps(MANIFEST, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print("[通过] manifest.json（id=%s）" % MOD_ID)
    print("构建完成。接下来用 CLI：check / compile / pack。")


if __name__ == "__main__":
    main()
