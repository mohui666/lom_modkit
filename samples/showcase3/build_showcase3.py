#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""全节点样例 3.0 构建器。

Story 只能经 editor/story_api.py 的受控 API 创建和修改；生成后再由
story_api CLI 执行 check / compile / pack。构建器会硬性检查当前 models
契约中的全部节点类型均至少出现一次，避免节点扩充后样例静默过期。
"""

from __future__ import annotations

import json
import shutil
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
SOURCE_ASSETS = PROJECT_ROOT / "samples" / "feature_showcase" / "assets" / "user"

MANIFEST = {
    "format": 1,
    "package_format": 1,
    "story_schema": 1,
    "content_schema": 1,
    "min_host_version": "0.6.0",
    "tested_host_version": "0.6.0",
    "id": "showcase3",
    "name": "全节点样例3.0·六十三节点实机验收",
    "version": "3.0.0",
    "author": "lom_modkit",
    "description": (
        "面向手动验收的 63 节点完整样例：用户图片/角色/音频、演出、"
        "数值与检定、MOD 任务/持久变量、奖励/商店、原版 Combat/Battle "
        "编排、死亡与安全返回。"
    ),
    "entry": "main",
}

COMBAT_KEY = "5102_01"
BATTLE_KEY = "0000"


def _blank_story(story_id: str, title: str) -> dict:
    story = story_api.new_story(story_id, title)
    for item in list(story.get("nodes") or []):
        story_api.delete_node(story, item["id"])
    story.pop("start", None)
    return story


def _node(story: dict, node_type: str, fields: dict | None = None, after=None) -> str:
    return story_api.add_node(story, node_type, fields or {}, after=after)["id"]


def _say(
    story: dict,
    text: str,
    character: str | None = None,
    mode: str = "character",
    portrait: str = "normal",
    voice: str | None = None,
    after=None,
) -> str:
    return story_api.add_say(
        story,
        text,
        character=character,
        mode=mode,
        portrait=portrait,
        voice=voice,
        after=after,
    )["id"]


def _set_start(story: dict, node_id: str) -> None:
    story_api.set_start(story, node_id)


def _join_check(
    story: dict,
    node_type: str,
    fields: dict,
    success_text: str,
    failure_text: str,
) -> str:
    """添加一个二向检定并汇合，返回汇合节点。"""
    check_id = _node(story, node_type, dict(fields, success="", failure=""))
    success_id = _say(story, success_text, mode="narrative")
    failure_id = _say(story, failure_text, mode="narrative")
    merge_id = _node(story, "message", {"text": "检定分支已汇合，继续下一项。"})
    story_api.update_node(
        story, check_id, {"success": success_id, "failure": failure_id}
    )
    story_api.update_node(story, success_id, {"goto": merge_id})
    story_api.update_node(story, failure_id, {"goto": merge_id})
    return merge_id


def build_main() -> dict:
    story = _blank_story("main", "3.0 第一章：演出与用户内容")
    start = _node(
        story,
        "message",
        {"text": "【3.0/1】演出、对白栏防裁剪水印与三类用户内容测试开始。"},
    )
    _set_start(story, start)
    _node(story, "music", {"name": "user:showcase.lantern_theme"})
    _node(
        story,
        "sound",
        {"name": "user:showcase.lantern_chime", "kind": "sound", "op": "play"},
    )
    _node(story, "scene", {"view": "center"})
    _node(
        story,
        "background",
        {"action": "show", "image": "user:showcase.courier_station", "fade": 0.5},
    )
    _node(
        story,
        "custom_cg",
        {
            "action": "show",
            "image": "user:showcase.departure_cg",
            "fade": 0.3,
            "scale": 68,
            "x": 0,
            "y": 0,
        },
    )
    _node(story, "custom_cg", {"action": "hide", "fade": 0.3})
    _node(
        story,
        "overlay",
        {
            "action": "show",
            "slot": "showcase_lantern",
            "image": "user:showcase.lantern_overlay",
            "position": "top_right",
            "scale": 34,
            "opacity": 88,
            "layer": "front",
            "fade": 0.25,
        },
    )
    _node(
        story,
        "show",
        {"character": "player", "position": "L1", "portrait": "normal"},
    )
    _node(
        story,
        "show",
        {
            "character": "user:showcase.lin_deng",
            "position": "R1",
            "portrait": "normal",
            "facing": "left",
        },
    )
    _say(
        story,
        "我是用户角色林灯。这句对白同时播放用户语音；请检查对白栏里是否有重复的低透明度 MOD 水印。",
        "user:showcase.lin_deng",
        voice="user:showcase.lin_greeting",
    )
    _node(
        story,
        "move",
        {"character": "player", "from": "L1", "to": "M", "duration": 0.6},
    )
    _node(story, "face", {"character": "player", "facing": "right"})
    _node(story, "focus", {"character": "user:showcase.lin_deng"})
    _node(
        story,
        "offset",
        {"character": "player", "x": 12, "y": -4, "duration": 0.25},
    )
    _node(story, "shock", {"character": "player", "duration": 0.35})
    _node(story, "dim", {"character": "player", "dimmed": True})
    _node(
        story,
        "rotate",
        {"character": "user:showcase.lin_deng", "angle": -6, "duration": 0.2},
    )
    _node(
        story,
        "rotate",
        {"character": "user:showcase.lin_deng", "angle": 6, "duration": 0.2},
    )
    _node(story, "dim", {"character": "player", "dimmed": False})
    _say(story, "旁白、人物、语音与舞台动作均已执行。", mode="narrative")
    _node(story, "mask", {"show": True})
    _say(story, "遮罩中的文字仍应显示玩家内容披露。", mode="center")
    _node(story, "mask", {"show": False})
    _node(story, "intro", {"intro_source": "official", "character": "sister1"})
    _node(story, "effect", {"name": "Hit_001", "x": 0, "y": 0})
    _node(story, "transition", {"phase": "in", "dir": "lr"})
    _node(story, "transition", {"phase": "out", "dir": "lr"})
    _node(story, "camera", {"name": "stage-memory", "active": True})
    _node(story, "camera", {"name": "stage-memory", "active": False})
    _node(story, "block", {"flowchart": "common", "name": "flash", "vars": []})
    # DisplayTitle 不持有 Addressables handle，适合在全节点样例中安全演示官方 cg
    # 接口；不能在没有先 ShowPicture 的情况下直接 HidePicture，原版会释放无效
    # handle 并抛出 Attempting to use an invalid operation handle。
    _node(story, "cg", {"action": "show", "kind": "title", "key": "全节点样例 3.0"})
    _node(story, "dayenv", {"day_type": 1})
    _node(story, "wait", {"seconds": 0.35})

    choice_anchor = _say(story, "下面弹出 choice。两个选项都会汇合。", mode="narrative")
    choice_merge = _say(story, "choice 已返回。", mode="narrative")
    story_api.add_choice(
        story,
        [("选项 A：继续", choice_merge), ("选项 B：也继续", choice_merge)],
        after=choice_anchor,
    )

    _node(story, "flag", {"flag": "SHOWCASE3_BRANCH"})
    branch_anchor = _say(story, "下面执行 mod flag 分支。", mode="narrative")
    branch_yes = _say(story, "branch：flag 已设置。", mode="narrative")
    branch_no = _say(story, "branch：flag 未设置。", mode="narrative")
    branch_merge = _node(story, "message", {"text": "branch 已汇合。"})
    _node(
        story,
        "branch",
        {
            "source": "mod",
            "flag": "SHOWCASE3_BRANCH",
            "cases": [
                {"value": 1, "goto": branch_yes},
                {"value": 2, "goto": branch_no},
            ],
        },
        after=branch_anchor,
    )
    story_api.update_node(story, branch_yes, {"goto": branch_merge})
    story_api.update_node(story, branch_no, {"goto": branch_merge})

    dice_anchor = _say(story, "下面调用自定义四段骰子。范围、标题、加值、每档文字和去向都可直接编辑。", mode="narrative")
    dice_best = _say(story, "骰子结果：大成功。", mode="narrative")
    dice_success = _say(story, "骰子结果：成功。", mode="narrative")
    dice_partial = _say(story, "骰子结果：勉强成功。", mode="narrative")
    dice_failure = _say(story, "骰子结果：失败。", mode="narrative")
    dice_merge = _node(story, "message", {"text": "骰子分支已汇合。"})
    story_api.add_dice(
        story,
        99,
        "灯影下的命运检定",
        [
            {"upper": 24, "text": "失手", "goto": dice_failure},
            {"upper": 49, "text": "勉强成功", "goto": dice_partial},
            {"upper": 79, "text": "成功", "goto": dice_success},
            {"text": "大成功", "goto": dice_best},
        ],
        bonus=5,
        bonus_name="准备充分",
        bonus_status="固定加值",
        after=dice_anchor,
    )
    for target in (dice_best, dice_success, dice_partial, dice_failure):
        story_api.update_node(story, target, {"goto": dice_merge})

    _node(story, "raw", {"code": 'local showcase3_raw_marker = "ok"\n'})
    _node(
        story,
        "hide",
        {"character": "user:showcase.lin_deng", "fadeDuration": 0.25},
    )
    _node(story, "overlay", {"action": "hide", "slot": "showcase_lantern", "fade": 0.2})
    _node(story, "background", {"action": "clear"})
    _node(story, "end", {"next_script": "gameplay"})
    return story


def build_gameplay() -> dict:
    story = _blank_story("gameplay", "3.0 第二章：数值、检定、任务与商店")
    start = _node(story, "message", {"text": "【3.0/2】Gameplay 组合节点开始。"})
    _set_start(story, start)
    _node(story, "stat", {"key": "mental", "delta": 1, "waitDisplay": False})
    _node(story, "stat_set", {"key": "talking", "value": 50, "update": False})
    _node(story, "affinity", {"character": "sister1", "delta": 1})
    _node(story, "talent", {"talent": "1010", "level": 1})
    _node(story, "item", {"kind": "misc", "item": "2001", "count": 1})
    _node(story, "game_flag", {"flag": "50019", "value": 1, "op": "set"})
    _node(story, "enemy", {"op": "team", "enemy": "400", "value": -10, "display": 1})
    _node(story, "battle_skill", {"op": "set", "key": "special3", "index": 2})
    _node(
        story,
        "reward",
        {
            "entries": [
                {"kind": "stat", "key": "money", "amount": 25},
                {"kind": "affinity", "key": "sister1", "amount": 1},
                {"kind": "item", "category": "book", "key": "1010", "amount": 1},
                {"kind": "flag", "key": "SHOWCASE3_REWARDED"},
            ]
        },
    )
    _node(
        story,
        "result_screen",
        {
            "title": "3.0 奖励结算",
            "text": "应显示提示并发放心相 +1。",
            "entries": [{"kind": "stat", "key": "mental", "amount": 1}],
        },
    )
    _node(
        story,
        "custom_shop",
        {
            "discount": 0,
            "items": [
                {"category": "misc", "item": "2001", "count": 1},
                {"category": "book", "item": "1010", "count": 1},
            ],
        },
    )
    _join_check(
        story,
        "stat_check",
        {"key": "mental", "op": ">=", "value": -999},
        "stat_check 成功。",
        "stat_check 失败。",
    )
    _join_check(
        story,
        "affinity_check",
        {"character": "sister1", "op": ">=", "value": -999},
        "affinity_check 成功。",
        "affinity_check 失败。",
    )
    _join_check(
        story,
        "item_check",
        {"category": "misc", "item": "2001", "invert": False},
        "item_check：持有黄酒。",
        "item_check：没有黄酒。",
    )
    _join_check(
        story,
        "talent_check",
        {"talent": "1010", "op": ">=", "value": 0},
        "talent_check 成功。",
        "talent_check 失败。",
    )
    _join_check(
        story,
        "flag_check",
        {"source": "mod", "flag": "SHOWCASE3_REWARDED", "invert": False},
        "flag_check：奖励 flag 已设置。",
        "flag_check：奖励 flag 未设置。",
    )
    _node(
        story,
        "mod_quest",
        {"quest": "showcase3_manual", "op": "start", "message": "接受 3.0 手测任务"},
    )
    _join_check(
        story,
        "quest_check",
        {"quest": "showcase3_manual", "state": "active"},
        "quest_check：任务 active。",
        "quest_check：任务状态异常。",
    )
    _node(story, "persistent_var", {"key": "showcase3_runs", "op": "add", "value": 1})
    _join_check(
        story,
        "persistent_check",
        {"key": "showcase3_runs", "op": ">=", "value": 1},
        "persistent_check：累计运行次数至少为 1。",
        "persistent_check：持久变量没有保存。",
    )

    activity_id = _node(
        story,
        "activity",
        {
            "kind": "training",
            "message": "activity：进行一次练武并推进一旬",
            "stat": "mental",
            "op": ">=",
            "value": -999,
            "time": "round",
            "success_rewards": [
                {"kind": "stat", "key": "mental", "amount": 1},
                {"kind": "flag", "key": "SHOWCASE3_ACTIVITY"},
            ],
            "failure_rewards": [{"kind": "stat", "key": "mental", "amount": -1}],
            "success": "",
            "failure": "",
        },
    )
    activity_ok = _say(story, "activity 成功路径。", mode="narrative")
    activity_bad = _say(story, "activity 失败路径。", mode="narrative")
    activity_merge = _node(story, "message", {"text": "activity 已汇合。"})
    story_api.update_node(
        story, activity_id, {"success": activity_ok, "failure": activity_bad}
    )
    story_api.update_node(story, activity_ok, {"goto": activity_merge})
    story_api.update_node(story, activity_bad, {"goto": activity_merge})

    _node(story, "mission", {"name": "Main", "key": "M0001"})
    _node(story, "time", {"op": "round"})
    _node(story, "autosave", {"kind": "story", "save_button": 1})
    _node(story, "panel", {"panel": "martial", "mode": 0})
    _node(story, "mod_quest", {"quest": "showcase3_manual", "op": "complete", "message": "3.0 手测任务完成"})
    _node(story, "end", {"next_script": "combat_demo"})
    return story


def build_combat() -> dict:
    story = _blank_story("combat_demo", "3.0 第三章：原版 Combat 编排")
    start = _node(story, "message", {"text": "【3.0/3】可测试原版 Combat，也可跳过。"})
    _set_start(story, start)
    fight = _node(
        story,
        "combat",
        {
            "key": COMBAT_KEY,
            "max_health": 300, "health": 300,
            "max_stamina": 120, "stamina": 120,
            "strength": 20, "internal": 20, "dexterity": 20, "talking": 20,
            "defence": 20, "sword": 20, "fist": 20,
            "martial_weapon": 20, "mental": 20,
            "talents": [],
            "talk_rate": 0.1, "attack_rate": 0.5, "weapon_rate": 0.1,
            "ultimate_rate": 0.05, "block_rate": 0.25,
            "win": "", "lose": "",
        },
    )
    inspect = _node(
        story,
        "battle_result",
        {"kind": "combat", "win": "", "lose": ""},
    )
    won = _say(story, "Combat 结果：win。", mode="narrative")
    lost = _say(story, "Combat 结果：lose。", mode="narrative")
    result = _node(
        story,
        "result_screen",
        {
            "title": "Combat 验收结束",
            "text": "胜负结果已由 Host 验证并返回 Story。",
            "entries": [{"kind": "stat", "key": "mental", "amount": 1}],
        },
    )
    finish = _node(story, "end", {"next_script": "battle_demo"})
    skip = _say(story, "已跳过 Combat。", mode="narrative", after=start)
    story_api.add_choice(
        story,
        [("进入原版 Combat", fight), ("跳过 Combat", skip)],
        after=start,
    )
    story_api.update_node(story, fight, {"win": inspect, "lose": inspect})
    story_api.update_node(story, inspect, {"win": won, "lose": lost})
    story_api.update_node(story, won, {"goto": result})
    story_api.update_node(story, lost, {"goto": result})
    story_api.update_node(story, result, {"goto": finish})
    story_api.update_node(story, skip, {"goto": finish})
    return story


def build_battle() -> dict:
    story = _blank_story("battle_demo", "3.0 第四章：原版 Battle 编排")
    start = _node(
        story,
        "message",
        {"text": "【3.0/4】Battle 失败中的 PlayerDie 可能按原版进入重试/标题；可选择跳过。"},
    )
    _set_start(story, start)
    setup = _node(
        story,
        "battle_setup",
        {
            "enemy": "400", "team": 0, "level": 1, "people": 1,
            "display": 1,
        },
    )
    war = _node(
        story,
        "battle",
        {
            "key": BATTLE_KEY,
            "friend_roster": BATTLE_KEY,
            "enemy_roster": BATTLE_KEY,
            "neutral_roster": BATTLE_KEY,
            "friend_people": 3, "enemy_people": 3, "neutral_people": 0,
            "friend_health": 300, "enemy_health": 300, "neutral_health": 300,
            "reset_skills": True,
            "skills": [{"key": "special3", "index": 2, "active": 1}],
            "win": "", "lose": "",
        },
    )
    inspect = _node(
        story,
        "battle_result",
        {"kind": "battle", "win": "", "lose": ""},
    )
    won = _say(story, "Battle 结果：FriendWin。", mode="narrative")
    lost = _say(story, "Battle 结果：EnemyWin。", mode="narrative")
    finish = _node(story, "end", {"next_script": "finale"})
    skip = _say(story, "已跳过 Battle。", mode="narrative", after=start)
    story_api.add_choice(
        story,
        [("进入原版 Battle", setup), ("跳过 Battle", skip)],
        after=start,
    )
    story_api.update_node(story, war, {"win": inspect, "lose": inspect})
    story_api.update_node(story, inspect, {"win": won, "lose": lost})
    story_api.update_node(story, won, {"goto": finish})
    story_api.update_node(story, lost, {"goto": finish})
    story_api.update_node(story, skip, {"goto": finish})
    return story


def build_finale() -> dict:
    story = _blank_story("finale", "3.0 终章：死亡画面或安全返回")
    start = _node(
        story,
        "message",
        {"text": "【3.0/5】全部节点均已抵达。请选择死亡卡测试或安全返回 Free。"},
    )
    _set_start(story, start)
    death = story_api.add_death(
        story,
        "全节点样例 3.0：主动选择的死亡画面测试。",
        death_id="930001",
        title="六十三节点·测试谢幕",
        next="Title",
    )["id"]
    safe = _node(story, "goto_scene", {"scene": "Free"})
    story_api.add_choice(
        story,
        [("测试死亡画面（回标题）", death), ("安全返回 Free", safe)],
        after=start,
    )
    return story


def main() -> None:
    _editor_data, fallback = story_api.load_editor_data()
    if fallback:
        raise SystemExit("data/editor_data.json 不可用，拒绝生成样例")

    if not SOURCE_ASSETS.is_dir():
        raise SystemExit("缺少 feature_showcase 用户内容资产")
    assets_root = SHOWCASE_DIR / "assets"
    if assets_root.exists():
        shutil.rmtree(assets_root)
    target_assets = assets_root / "user"
    target_assets.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(SOURCE_ASSETS, target_assets)

    builders = (build_main, build_gameplay, build_combat, build_battle, build_finale)
    STORY_DIR.mkdir(parents=True, exist_ok=True)
    used: set[str] = set()
    stories: dict[str, dict] = {}
    for builder in builders:
        story = builder()
        errors, warnings = story_api.check_story(story)
        for warning in warnings:
            print(f"[警告] {story['id']}: {warning}")
        if errors:
            raise SystemExit(
                "校验失败 %s:\n%s" % (story["id"], "\n".join(errors))
            )
        stories[story["id"]] = story
        used.update(node["type"] for node in story["nodes"])
        story_api.save_story_json(story, STORY_DIR / f"{story['id']}.json")
        print(f"[通过] story/{story['id']}.json ({len(story['nodes'])} 节点)")

    missing = sorted(set(models.NODE_TYPES) - used)
    extra = sorted(used - set(models.NODE_TYPES))
    if missing or extra:
        raise SystemExit(f"节点覆盖错误：missing={missing}, extra={extra}")
    print(f"[通过] 当前契约 {len(models.NODE_TYPES)} 种节点全覆盖")

    (SHOWCASE_DIR / "manifest.json").write_text(
        json.dumps(MANIFEST, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print("[通过] manifest.json")
    print("生成完成；下一步必须用 story_api CLI check / compile / pack。")


if __name__ == "__main__":
    main()
