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

_HERE = os.path.dirname(os.path.abspath(__file__))
# compiler/lomc -> 项目根
_PROJECT_ROOT = os.path.dirname(os.path.dirname(_HERE))
EDITOR_DATA_PATH = os.path.join(_PROJECT_ROOT, "data", "editor_data.json")

_META = None  # 进程级缓存；测试可赋值覆盖


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
