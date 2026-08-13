# -*- coding: utf-8 -*-
"""骰子检查点元数据：从 data/editor_data.json 的 dice_meta 读取。

由 tools/extract_editor_data.py 从官方脚本提取（schema 3）。每个检查点有：
  max    —— 骰子范围（官方调用点 math.random(max) / SetRandom(max, ...)）
  bands  —— 结果带（差→好）[{text, cond}]，条数 = 骰子菜单选项按钮数

游戏内 DiceMenuDialog 按结果带索引选项按钮（UpdateSelection 用
_resultSelection - 1 做下标），选项条数少于结果带数时索引越界 NRE、
菜单卡死（继续按钮永不出现），因此编译期必须校验元数据存在。
"""

import json
import os
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
# 源码态 compiler/lomc -> 项目根；PyInstaller 冻结态的数据文件位于
# sys._MEIPASS/data，不能再从 PYZ 内的 lomc.__file__ 反推目录。
_PROJECT_ROOT = (
    getattr(sys, "_MEIPASS")
    if getattr(sys, "frozen", False) and hasattr(sys, "_MEIPASS")
    else os.path.dirname(os.path.dirname(_HERE))
)
EDITOR_DATA_PATH = os.path.join(_PROJECT_ROOT, "data", "editor_data.json")

_META = None  # 进程级缓存；测试可赋值覆盖
_ED_IDS = None  # 官方死亡/结局画面 id 缓存（goto_scene 警告用）
_PORTRAITS = None  # 角色表情表缓存：{character_id: [portrait, ...]}；缺文件时 None


def load_editor_ids(path=None):
    """读 data/editor_data.json 的官方 death_ids/ending_ids（schema 3）。

    goto_scene 校验警告用：scene=GameOver/End 且 key 命中官方 id 时提示改用
    ≥900000 的 mod 专属 id（官方 id 会触发结局解锁与记录，污染存档）。
    """
    global _ED_IDS
    if _ED_IDS is not None:
        return _ED_IDS
    ids = {"death_ids": [], "ending_ids": []}
    p = path or EDITOR_DATA_PATH
    try:
        with open(p, "r", encoding="utf-8") as f:
            data = json.load(f)
        ids["death_ids"] = list(data.get("death_ids") or [])
        ids["ending_ids"] = list(data.get("ending_ids") or [])
    except (OSError, ValueError):
        pass
    _ED_IDS = ids
    return ids


def get_official_scene_ids():
    """官方死亡/结局画面 id 集合（death_ids ∪ ending_ids），字符串集合。

    schema 3 起 death_ids/ending_ids 为 [{id, name}] 对象数组（兼容旧裸字符串）。
    """
    ids = load_editor_ids()

    def flat(lst):
        out = set()
        for e in lst:
            if isinstance(e, dict):
                out.add(str(e.get("id", "")))
            else:
                out.add(str(e))
        return out

    return flat(ids["death_ids"]) | flat(ids["ending_ids"])


def load_dice_meta(path=None):
    """读 data/editor_data.json 的 dice_meta；缺文件/缺字段时返回空表。"""
    global _META
    if _META is not None:
        return _META
    meta = {}
    p = path or EDITOR_DATA_PATH
    try:
        with open(p, "r", encoding="utf-8") as f:
            meta = json.load(f).get("dice_meta") or {}
    except (OSError, ValueError):
        meta = {}
    _META = meta
    return meta


def get_dice_meta(check):
    """单个检查点元数据；查不到返回 None。"""
    return load_dice_meta().get(check)


def load_portrait_table(path=None):
    """读 data/editor_data.json 的 characters[].portraits 角色表情表。

    返回 {character_id: [portrait, ...]}；文件缺失/损坏（OSError/JSON 解析失败）
    时返回 None（表不可用）。文件存在但无 characters 字段时返回空表 {}。

    用途（练武场卡死修复）：show/say 节点的 (character, portrait) 组合必须落在
    表内——游戏 CharacterPlaceholder.LoadCharacterPortrait 对无效表情 key 抛
    KeyNotFoundException → Lua 协程死 → 对话冻结（Player.log 实证）。
    """
    global _PORTRAITS
    if _PORTRAITS is not None:
        return _PORTRAITS
    table = None
    p = path or EDITOR_DATA_PATH
    try:
        with open(p, "r", encoding="utf-8") as f:
            data = json.load(f)
        table = {}
        for c in data.get("characters") or []:
            if not isinstance(c, dict):
                continue
            cid = c.get("id")
            if cid is None:
                continue
            table[str(cid)] = [
                str(x) for x in (c.get("portraits") or []) if isinstance(x, str)
            ]
    except (OSError, ValueError):
        table = None
    _PORTRAITS = table
    return table


def check_portrait(table, character, portrait):
    """表情合法性检查（表情表不可用/角色不在表 → 放行）。

    返回 None 表示放行；返回字符串时该串为报错文案（角色存在但表情不在其列表）。
    """
    if table is None or character not in table:
        return None
    if portrait in table[character]:
        return None
    return (
        '角色 "%s" 没有表情 "%s"（该角色表情：%s）。游戏 LoadCharacterPortrait '
        % (
            character,
            portrait,
            "、".join(table[character]) or "无",
        )
        + "对无效表情 key 抛 KeyNotFoundException → Lua 协程死 → 对话冻结，"
        + "请改用清单内表情。"
    )
