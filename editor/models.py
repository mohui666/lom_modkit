# -*- coding: utf-8 -*-
"""数据模型：story.json 读写、节点工厂、节点摘要、editor_data.json 加载。

格式契约见 docs/zh_CN/mod_format.md §3（节点类型）与 §5（editor_data.json）。
节点在编辑器内部就是普通 dict（与 JSON 结构一致），本模块只管结构约束与默认值。
"""

from __future__ import annotations

import copy
import json
import os
import re
import sys
import tempfile
from pathlib import Path

from i18n import t, term

# story 脚本 id 规则（契约 §1）
ID_PATTERN = re.compile(r"^[a-zA-Z0-9_\-]{1,64}$")
# 节点 id 会拼进 Lua 函数名，不能包含短横线。
NODE_ID_PATTERN = re.compile(r"^[a-zA-Z0-9_]+$")
# manifest mod id 比 story id 更严格：运行时注册名与包格式只接受小写。
MOD_ID_PATTERN = re.compile(r"^[a-z0-9_\-]{1,64}$")
CHARACTER_DISPLAY_PATTERN = re.compile(r"^.*（([a-zA-Z0-9_\-]+)）$")

# ---------------------------------------------------------------------------
# 节点类型中文名（契约 §3.1）
# 源表保持简中，界面显示走 refresh_labels() → i18n。
# ---------------------------------------------------------------------------
NODE_TYPE_CN_SRC: dict[str, str] = {
    "music": "音乐",
    "sound": "音效",
    "scene": "切换官方背景",
    "background": "自定义背景",
    "custom_cg": "自定义全屏 CG",
    "overlay": "前景 / 插图",
    "show": "人物登场",
    "move": "人物移动",
    "face": "人物转向",
    "hide": "人物退场",
    "focus": "镜头聚焦",
    "offset": "人物位移",
    "say": "对白",
    "choice": "选项分支",
    "shock": "人物震动",
    "mask": "独白遮罩",
    "intro": "人物介绍卡",
    "effect": "屏幕特效",
    "transition": "转场",
    "camera": "镜头滤镜",
    "block": "流图块(高级)",
    "cg": "图片/标题显示",
    "dim": "压暗",
    "message": "系统提示",
    "rotate": "旋转",
    "dayenv": "日夜环境",
    "stat": "属性增减",
    "stat_set": "属性设置",
    "affinity": "好感度",
    "talent": "天赋",
    "item": "物品",
    "flag": "剧情旗标",
    "game_flag": "游戏任务旗标",
    "enemy": "敌方队伍",
    "battle_skill": "战场技能",
    "mission": "任务",
    "time": "时间",
    "autosave": "自动存档",
    "branch": "条件分支",
    "dice": "骰子检定",
    "goto_scene": "进入其他场景",
    "panel": "系统面板",
    "wait": "等待",
    "end": "结束剧情",
    "death": "死亡画面",
    "raw": "原生 Lua（高级）",
}
NODE_TYPE_CN: dict[str, str] = dict(NODE_TYPE_CN_SRC)

# 新手菜单最先展示的高频步骤。特殊的“汗青书结局”由主窗口用 goto_scene
# 预设创建，不在这里重复列出。
COMMON_NODE_TYPES: list[str] = [
    "say",
    "show",
    "hide",
    "intro",
    "scene",
    "background",
    "custom_cg",
    "overlay",
    "choice",
    "dice",
    "wait",
    "end",
    "death",
]

# 节点表单顶部的上下文提示。只解释当前动作，不复述控件名称；完整流程见内置帮助。
NODE_HELP_KEYS = {
    "say": "help.say",
    "show": "help.show",
    "hide": "help.hide",
    "scene": "help.scene",
    "background": "help.background",
    "custom_cg": "help.custom_cg",
    "overlay": "help.overlay",
    "choice": "help.choice",
    "dice": "help.dice",
    "goto_scene": "help.goto_scene",
    "end": "help.end",
    "death": "help.death",
    "intro": "help.intro",
    "raw": "help.raw",
    "music": "help.music",
    "sound": "help.sound",
}
NODE_HELP: dict[str, str] = {}

# 契约 §3.1 分组（新增节点菜单按此分组显示）
NODE_GROUPS_SRC: list[tuple[str, list[str]]] = [
    (
        "group.visual",
        [
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
        ],
    ),
    (
        "group.stats",
        [
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
        ],
    ),
    (
        "group.flow",
        ["branch", "dice", "goto_scene", "panel", "wait", "end", "death", "raw"],
    ),
]
NODE_GROUPS: list[tuple[str, list[str]]] = []

# ---------------------------------------------------------------------------
# 枚举字段的中文标注（表单 kind="enum:<名字>" 用）
# ---------------------------------------------------------------------------
ENUM_SETS_SRC: dict[str, list[tuple[str, str]]] = {
    "music_op": [("play", "播放"), ("stop", "停止"), ("fadeout", "淡出")],
    "sound_kind": [("sound", "音效"), ("env", "环境音")],
    "sound_op": [("play", "播放"), ("fadeout", "淡出")],
    "background_action": [
        ("set", "立即设置"),
        ("show", "显示"),
        ("replace", "替换"),
        ("fadein", "淡入"),
        ("fadeout", "淡出并清除"),
        ("clear", "立即清除"),
    ],
    "transition_phase": [("in", "淡入"), ("out", "淡出")],
    "transition_dir": [
        ("lr", "从左到右"),
        ("rl", "从右到左"),
        ("tb", "从上到下"),
        ("bt", "从下到上"),
    ],
    "cg_action": [("show", "显示"), ("hide", "隐藏")],
    "overlay_action": [("show", "显示 / 替换"), ("hide", "隐藏")],
    "overlay_position": [
        ("center", "中央"),
        ("top", "上方"),
        ("bottom", "下方"),
        ("left", "左侧"),
        ("right", "右侧"),
        ("top_left", "左上"),
        ("top_right", "右上"),
        ("bottom_left", "左下"),
        ("bottom_right", "右下"),
    ],
    "overlay_layer": [("back", "人物后方"), ("front", "人物前方")],
    "cg_kind": [
        ("picture", "图片"),
        ("item", "物品图"),
        ("big", "大图"),
        ("map", "地图"),
        ("family", "家谱"),
        ("title", "标题"),
    ],
    "block_flowchart": [("view", "view（场景）"), ("common", "common（通用）")],
    "item_kind": [("book", "秘籍"), ("misc", "杂物"), ("special", "特殊物品")],
    "game_flag_op": [("set", "设置"), ("add", "增减")],
    "enemy_op": [
        ("team", "队伍"),
        ("level", "等级"),
        ("people", "人数"),
        ("id", "替换 id"),
    ],
    "battle_skill_op": [("set", "设置"), ("active", "激活"), ("reset", "重置")],
    "time_op": [
        ("set", "设置时间"),
        ("round", "进入下一旬"),
        ("month", "进入下一月"),
        ("mission", "设置任务时间"),
    ],
    "autosave_kind": [
        ("story", "剧情存档"),
        ("free", "自由存档"),
        ("prologue", "序章存档"),
    ],
    "goto_scene": [
        ("Free", "自由模式"),
        ("Title", "标题画面"),
        ("Combat", "战斗"),
        ("Battle", "战役"),
        ("GameOver", "游戏结束"),
        ("End", "结局"),
        ("Story", "剧情演出"),
        ("DemoEnd", "Demo 结束"),
    ],
    # 原版 GameOverController 的按钮固定为“读档 / 标题画面”，没有自定义去向。
    # 保留该枚举名供旧 JSON 兼容，但编辑器只提供真实有效的 Title。
    "death_next": [("Title", "标题画面")],
    "intro_source": [
        ("official", "使用原版人物资料"),
        ("character", "使用自定义角色介绍卡"),
        ("custom", "本步骤手写资料"),
    ],
    "panel": [
        ("martial", "武学面板"),
        ("weapon", "武器强化"),
        ("poison", "毒药强化"),
        ("cg", "回忆画廊"),
        ("cgvideo", "过场动画"),
        ("shop", "商店"),
        ("newshop", "新商店"),
        ("credit", "制作名单"),
        ("endgame", "结算面板"),
    ],
}
ENUM_SETS: dict[str, list[tuple[str, str]]] = {}
# 切换后需要重建表单的枚举（其它字段或可见字段随它变化）
REBUILD_ENUMS = {
    "item_kind",
    "goto_scene",
    "mode",
    "intro_source",
    "sound_kind",
    "background_action",
    "cg_action",
    "overlay_action",
}

# say mode 中文标注（清单本身来自 editor_data.modes）
MODE_CN_SRC = {
    "character": "对话",
    "think": "内心独白",
    "narrative": "旁白",
    "center": "居中旁白",
}
MODE_CN = dict(MODE_CN_SRC)
FACING_CN = [("left", "朝左"), ("right", "朝右")]
# 镜头滤镜常见预设（契约 §3.1 示例）
CAMERA_PRESETS = ["stage-memory", "stage-dream", "stage-fire", "stage-blurdim"]

_ENUM_KEY_OVERRIDE = {
    ("time_op", "set"): "enum.set_time",
    ("intro_source", "character"): "enum.intro_character",
    ("intro_source", "custom"): "enum.intro_custom",
}


def refresh_labels() -> None:
    """语言切换后刷新模块级显示名。"""
    global NODE_TYPE_CN, NODE_GROUPS, ENUM_SETS, MODE_CN, FACING_CN, NODE_HELP
    NODE_TYPE_CN = {
        key: t(f"node.{key}", default=label) for key, label in NODE_TYPE_CN_SRC.items()
    }
    NODE_GROUPS = [
        (t(group_key, default=group_key), types)
        for group_key, types in NODE_GROUPS_SRC
    ]
    rebuilt: dict[str, list[tuple[str, str]]] = {}
    for set_name, options in ENUM_SETS_SRC.items():
        rows = []
        for value, fallback in options:
            key = _ENUM_KEY_OVERRIDE.get((set_name, value), f"enum.{value}")
            rows.append((value, t(key, default=fallback)))
        rebuilt[set_name] = rows
    ENUM_SETS = rebuilt
    MODE_CN = {
        key: t(f"mode.{key}", default=label) for key, label in MODE_CN_SRC.items()
    }
    FACING_CN = [
        ("left", t("facing.left", default="朝左")),
        ("right", t("facing.right", default="朝右")),
    ]
    NODE_HELP = {
        key: t(i18n_key, default="") for key, i18n_key in NODE_HELP_KEYS.items()
    }
    global BRANCH_SOURCES, BRANCH_MOD_VALUES, BRANCH_COND_VALUES
    if "BRANCH_SOURCES_SRC" in globals():
        BRANCH_SOURCES = [
            (value, t(key, default=value)) for value, key in BRANCH_SOURCES_SRC
        ]
        BRANCH_MOD_VALUES = [
            (1, t("branch.set", default="1=已设置")),
            (2, t("branch.unset", default="2=未设置")),
        ]
        BRANCH_COND_VALUES = [
            (1, t("branch.true", default="1=真")),
            (2, t("branch.false", default="2=假")),
        ]


refresh_labels()


def enum_label(set_name: str, value: str) -> str:
    """枚举值的中文名，查不到返回原值。"""
    for val, cn in ENUM_SETS.get(set_name, []):
        if val == value:
            return cn
    return str(value)


# ---------------------------------------------------------------------------
# 节点类型表（契约 §3.1）
# 每种类型声明字段清单；kind 供 node_form 动态生成控件：
#   character/portrait/position/view/music/stat/mode/facing  → 下拉框
#   line     → 单行文本
#   multiline→ 多行文本
#   int      → 整数 spinbox
#   float    → 小数 spinbox
#   bool     → 勾选框
#   options  → choice 的选项表格（text + goto）
#   cases    → branch 的分支表格（value + goto）
# optional=True 且值为空时，序列化不写出该字段。
# ---------------------------------------------------------------------------
NODE_SCHEMAS: dict[str, dict] = {
    # ---------------------------------------------------------------- 演出类
    "music": {
        "label": "播放音乐",
        "fields": [
            ("name", "音乐", "music", False),
            ("op", "操作", "enum:music_op", True),
            ("seconds", "淡出秒数", "float", True),
        ],
    },
    "sound": {
        "label": "播放音效",
        "fields": [
            ("name", "音效", "sound_name", False),
            ("kind", "声道", "enum:sound_kind", True),
            ("op", "操作", "enum:sound_op", True),
            ("seconds", "淡出秒数", "float", True),
        ],
    },
    "scene": {
        "label": "切换官方背景",
        "fields": [("view", "官方背景", "view", False)],
    },
    "background": {
        "label": "自定义背景",
        "fields": [
            ("action", "动作", "enum:background_action", False),
            ("image", "用户图片", "user_image", True),
            ("fade", "淡入/淡出秒数", "float", True),
        ],
    },
    "custom_cg": {
        "label": "自定义全屏 CG",
        "fields": [
            ("action", "动作", "enum:cg_action", False),
            ("image", "用户图片", "user_image", True),
            ("fade", "淡入/淡出秒数", "float", True),
            ("scale", "缩放百分比", "percent_cg_scale", True),
            ("x", "横向位置百分比", "percent_position", True),
            ("y", "纵向位置百分比", "percent_position", True),
        ],
    },
    "overlay": {
        "label": "前景 / 插图",
        "fields": [
            ("action", "动作", "enum:overlay_action", False),
            ("slot", "槽位 id", "line", False),
            ("image", "用户图片", "user_image", True),
            ("position", "位置", "enum:overlay_position", True),
            ("scale", "缩放百分比", "percent_cg_scale", True),
            ("opacity", "不透明度", "percent_opacity", True),
            ("layer", "层级", "enum:overlay_layer", True),
            ("fade", "淡入/淡出秒数", "float", True),
        ],
    },
    "show": {
        "label": "显示人物",
        "fields": [
            ("character", "人物", "character", False),
            ("position", "站位", "position", False),
            ("portrait", "表情", "portrait", True),
            ("facing", "朝向", "facing", True),
            ("fadeDuration", "淡入时长", "float", True),
            ("moveDuration", "移动时长", "float", True),
        ],
    },
    "move": {
        "label": "移动人物",
        "fields": [
            ("character", "人物", "character", False),
            ("from", "起点站位", "position", False),
            ("to", "终点站位", "position", False),
            ("duration", "时长", "float", True),
        ],
    },
    "face": {
        "label": "人物转向",
        "fields": [
            ("character", "人物", "character", False),
            ("facing", "朝向", "facing", False),
        ],
    },
    "hide": {
        "label": "隐藏人物",
        "fields": [
            ("character", "人物", "character", False),
            ("fadeDuration", "淡出时长", "float", True),
        ],
    },
    "focus": {
        "label": "镜头聚焦",
        "fields": [("character", "人物", "character", False)],
    },
    "offset": {
        "label": "人物位移",
        "fields": [
            ("character", "人物", "character", False),
            ("x", "横向偏移(px)", "int", False),
            ("y", "纵向偏移(px)", "int", False),
            ("duration", "时长", "float", False),
        ],
    },
    "say": {
        "label": "对话/独白/旁白",
        "fields": [
            ("text", "文本", "multiline", False),
            ("character", "人物", "character", True),
            ("portrait", "表情", "portrait", True),
            ("mode", "模式", "mode", True),
            ("voice", "对白语音", "voice", True),
        ],
    },
    "choice": {
        "label": "选项菜单",
        "fields": [
            ("options", "选项(2~4个)", "options", False),
            ("dialog", "菜单皮肤", "menu_dialog", True),
        ],
    },
    "shock": {
        "label": "震动特效",
        "fields": [
            ("character", "人物", "character", False),
            ("duration", "时长", "float", True),
        ],
    },
    "mask": {
        "label": "独白遮罩",
        "fields": [("show", "显示遮罩", "bool", False)],
    },
    "intro": {
        "label": "人物介绍卡",
        "fields": [
            ("intro_source", "资料来源", "enum:intro_source", False),
            ("character", "原版人物", "affinity_character", False),
            ("title", "人物称号", "line", True),
            ("name", "人物姓名", "line", False),
            ("text", "人物介绍", "multiline", False),
            ("image", "人物图片", "intro_image", True),
            ("image_scale", "图片大小", "percent_scale", True),
            ("image_x", "左右微调", "percent_offset", True),
            ("image_y", "上下微调", "percent_offset", True),
        ],
    },
    "effect": {
        "label": "屏幕特效",
        "fields": [
            ("name", "特效名", "effect", False),
            ("x", "x 偏移", "int", True),
            ("y", "y 偏移", "int", True),
            ("a", "参数 a", "float", True),
            ("b", "参数 b", "float", True),
            ("c", "参数 c", "float", True),
            ("play", "停止特效", "bool", True),
        ],
    },
    "transition": {
        "label": "黑场转场",
        "fields": [
            ("phase", "淡入/淡出", "enum:transition_phase", False),
            ("dir", "扫动方向", "enum:transition_dir", True),
        ],
    },
    "camera": {
        "label": "镜头滤镜",
        "fields": [
            ("name", "滤镜名", "camera", False),
            ("active", "启用", "bool", False),
        ],
    },
    "block": {
        "label": "流图块(高级)",
        "fields": [
            ("flowchart", "流图", "enum:block_flowchart", False),
            ("name", "块名", "line", False),
            ("vars", "变量", "vars", True),
        ],
    },
    "cg": {
        "label": "图片/标题显示",
        "fields": [
            ("action", "动作", "enum:cg_action", False),
            ("kind", "类别", "enum:cg_kind", False),
            ("key", "参数 key", "line", True),
            ("key2", "参数 key2", "line", True),
            ("n1", "数值 n1", "int", True),
            ("n2", "数值 n2", "int", True),
        ],
    },
    "dim": {
        "label": "人物压暗",
        "fields": [
            ("character", "人物", "character", False),
            ("dimmed", "压暗", "bool", False),
        ],
    },
    "message": {
        "label": "系统提示",
        "fields": [("text", "提示文本", "multiline", False)],
    },
    "rotate": {
        "label": "人物旋转",
        "fields": [
            ("character", "人物", "character", False),
            ("angle", "角度（整数，正=逆时针）", "int", False),
            ("duration", "时长（秒）", "float", False),
        ],
    },
    "dayenv": {
        "label": "日夜环境",
        "fields": [("day_type", "环境（1=白天 / 2=晚上）", "int", False)],
    },
    # ------------------------------------------------------------ 数值状态类
    "stat": {
        "label": "主角属性增减",
        "fields": [
            ("key", "属性", "stat", False),
            ("delta", "变化量", "int", False),
            ("waitDisplay", "等待显示", "bool", True),
            ("display", "显示样式", "int", True),
            ("mode", "模式参数", "line", True),
        ],
    },
    "stat_set": {
        "label": "主角属性设置",
        "fields": [
            ("key", "属性", "stat", False),
            ("value", "数值", "int", False),
            ("update", "用 Update 接口", "bool", True),
        ],
    },
    "affinity": {
        "label": "好感度增减",
        "fields": [
            ("character", "人物", "character", False),
            ("delta", "变化量", "int", False),
        ],
    },
    "talent": {
        "label": "天赋增减",
        "fields": [
            ("talent", "天赋", "talent", False),
            ("level", "等级变化（±）", "int", False),
        ],
    },
    "item": {
        "label": "物品增减",
        "fields": [
            ("kind", "类别", "enum:item_kind", False),
            ("item", "物品", "item", False),
            ("count", "数量", "int", False),
            ("remove", "移除而非获得", "bool", True),
        ],
    },
    "flag": {
        "label": "记录剧情flag",
        "fields": [("flag", "flag 标识", "line", False)],
    },
    "game_flag": {
        "label": "游戏任务旗标",
        "fields": [
            ("flag", "旗标 id", "game_flag", False),
            ("value", "数值", "int", False),
            ("op", "操作", "enum:game_flag_op", True),
        ],
    },
    "enemy": {
        "label": "敌方队伍修改",
        "fields": [
            ("op", "操作", "enum:enemy_op", False),
            ("enemy", "敌方队伍 id", "line", False),
            ("value", "数值", "int", True),
            ("display", "显示样式", "int", True),
        ],
    },
    "battle_skill": {
        "label": "战场技能",
        "fields": [
            ("op", "操作", "enum:battle_skill_op", False),
            ("key", "技能 id", "line", True),
            ("index", "槽位", "int", True),
            ("active", "激活（1/0）", "int", True),
        ],
    },
    "mission": {
        "label": "任务操作",
        "fields": [
            ("name", "任务名", "line", False),
            ("key", "操作 key", "line", False),
        ],
    },
    "time": {
        "label": "时间操作",
        "fields": [
            ("op", "操作", "enum:time_op", False),
            ("year", "年", "int", True),
            ("month", "月", "int", True),
            ("stage", "时段", "int", True),
            ("name", "任务名", "line", True),
        ],
    },
    "autosave": {
        "label": "自动存档",
        "fields": [
            ("kind", "存档类型", "enum:autosave_kind", True),
            ("save_button", "存档按钮（0/1）", "int", True),
        ],
    },
    # ---------------------------------------------------------------- 流程类
    "branch": {
        "label": "条件分支",
        "fields": [
            ("flag", "flag 标识（mod/game/flag_value/condition）", "line", True),
            ("stat", "属性（stat 来源）", "stat", True),
            ("source", "分支来源", "branch_source", True),
            ("cases", "分支(≥1个)", "cases", False),
        ],
    },
    "dice": {
        "label": "骰子检定",
        "fields": [
            ("check", "骰子检查点", "dice_check", False),
            ("options", "检定选项", "dice_options", False),
        ],
    },
    "goto_scene": {
        "label": "进入其他场景",
        "fields": [
            ("scene", "目标场景", "enum:goto_scene", False),
            ("key", "场景或战斗编号", "goto_scene_key", True),
            ("next", "结束后回到", "line", True),
            ("title", "结局标题", "line", True),
            ("desc", "结局描述", "multiline", True),
            ("image", "汗青书左页插图", "ending_image", True),
        ],
    },
    "panel": {
        "label": "系统面板",
        "fields": [
            ("panel", "面板", "enum:panel", False),
            ("key", "参数 id", "line", True),
            ("discount", "折扣", "int", True),
            ("mode", "模式", "int", True),
        ],
    },
    "wait": {
        "label": "等待",
        "fields": [("seconds", "秒数", "float", False)],
    },
    "end": {
        "label": "结束剧情",
        "fields": [("next_script", "下一章节", "story_ref", True)],
    },
    "death": {
        "label": "死亡画面",
        "fields": [
            ("title", "标题", "line", True),
            ("text", "死亡正文", "multiline", False),
            ("death_id", "专用编号", "death_id", False),
            ("next", "结束后去向", "enum:death_next", True),
        ],
    },
    "raw": {
        "label": "原生 Lua（高级）",
        "fields": [("code", "Lua 代码", "code", False)],
    },
}

NODE_TYPES: list[str] = list(NODE_SCHEMAS.keys())

# 各类型新节点的默认值（契约 §3.1 标注的默认值）
_NODE_DEFAULTS: dict[str, dict] = {
    "music": {"name": "", "op": "play"},
    "sound": {"name": "", "kind": "sound", "op": "play"},
    "scene": {"view": ""},
    "background": {"action": "show", "image": "", "fade": 0.5},
    "custom_cg": {
        "action": "show",
        "image": "",
        "fade": 0.5,
        "scale": 100,
        "x": 0,
        "y": 0,
    },
    "overlay": {
        "action": "show",
        "slot": "main",
        "image": "",
        "position": "center",
        "scale": 100,
        "opacity": 100,
        "layer": "front",
        "fade": 0.25,
    },
    "show": {
        "character": "",
        "position": "M",
        "portrait": "normal",
        "facing": "right",
        "fadeDuration": 0,
        "moveDuration": 0,
    },
    "move": {"character": "", "from": "L1", "to": "M", "duration": 1},
    "face": {"character": "", "facing": "right"},
    "hide": {"character": "", "fadeDuration": 0},
    "focus": {"character": ""},
    "offset": {"character": "", "x": 0, "y": 0, "duration": 0.5},
    "say": {"text": "", "character": "", "portrait": "normal", "mode": "character"},
    "choice": {
        "options": [{"text": "选项一", "goto": ""}, {"text": "选项二", "goto": ""}],
        "dialog": "Options",
    },
    "shock": {"character": "", "duration": 0.5},
    "mask": {"show": True},
    "intro": {
        "intro_source": "official",
        "character": "",
        "title": "",
        "name": "",
        "text": "",
        "image": "",
        "image_scale": 100,
        "image_x": 0,
        "image_y": 0,
    },
    "effect": {"name": "", "x": 0, "y": 0, "a": 1, "b": 1, "c": 1, "play": True},
    "transition": {"phase": "in", "dir": "lr"},
    "camera": {"name": "stage-memory", "active": True},
    "block": {"flowchart": "common", "name": "", "vars": []},
    "cg": {"action": "show", "kind": "picture"},
    "dim": {"character": "", "dimmed": True},
    "message": {"text": ""},
    "rotate": {"character": "", "angle": 180, "duration": 1},
    "dayenv": {"day_type": 1},
    "stat": {"key": "", "delta": 0, "waitDisplay": True, "display": 1},
    "stat_set": {"key": "", "value": 0, "update": False},
    "affinity": {"character": "", "delta": 1},
    "talent": {"talent": "", "level": 1},
    "item": {"kind": "misc", "item": "", "count": 1, "remove": False},
    "flag": {"flag": ""},
    "game_flag": {"flag": "", "value": 1, "op": "set"},
    "enemy": {"op": "team", "enemy": "", "value": 0, "display": 1},
    "battle_skill": {"op": "set", "key": "", "index": 2, "active": 1},
    "mission": {"name": "Main", "key": ""},
    "time": {"op": "round"},
    "autosave": {"kind": "story"},
    "branch": {"flag": "", "source": "mod", "cases": [{"value": 1, "goto": ""}]},
    "dice": {
        "check": "",
        "options": [
            {
                "goto_大成功": "",
                "goto_成功": "",
                "goto_失败": "",
            }
        ],
    },
    "goto_scene": {"scene": "Free"},
    "panel": {"panel": "martial"},
    "wait": {"seconds": 1},
    "end": {},
    "death": {"title": "", "text": "", "death_id": "900001", "next": "Title"},
    "raw": {"code": "-- 原生 Lua 代码，原样插入编译产物\n"},
}

# 新节点可用的默认人物（editor_data 的第一个人物），由 new_node 调用方注入
DEFAULT_PORTRAITS = [
    "normal",
    "nervous1",
    "nervous2",
    "nervous3",
    "angry1",
    "angry2",
    "laugh1",
    "gloomy2",
]

# editor_data.json 不存在时的兜底数据（契约 §5 schema 2 同构，{id,name} 对象数组）
FALLBACK_EDITOR_DATA: dict = {
    "schema": 3,
    "characters": [
        {"id": "player", "name": "主角", "portraits": list(DEFAULT_PORTRAITS)},
        {"id": "brother4", "name": "四师兄", "portraits": list(DEFAULT_PORTRAITS)},
        {"id": "trainee1", "name": "师弟", "portraits": list(DEFAULT_PORTRAITS)},
    ],
    "views": [{"id": v, "name": v} for v in ("out", "center", "paddy")],
    "music": [{"id": "普通_001", "name": "普通_001"}],
    "positions": [
        {"id": p, "name": p} for p in ("SL", "L1", "L2", "M", "R1", "R2", "RM2", "SR")
    ],
    "stats": [
        {"id": "mental", "name": "心相"},
        {"id": "money", "name": "银两"},
        {"id": "disposition", "name": "处世"},
        {"id": "behaviour", "name": "行为"},
        {"id": "karma", "name": "业力"},
        {"id": "fame", "name": "名声"},
        {"id": "talking", "name": "口才"},
        {"id": "team", "name": "队伍"},
    ],
    "free_positions": [{"id": "Center", "name": "练功场"}],
    "modes": ["character", "think", "narrative", "center"],
    "menu_dialogs": ["Options", "Talk", "Meet", "Center"],
    "effects": [{"id": "Hit_001", "name": "Hit_001"}],
    "dice_checks": [],
    "dice_meta": {},
    "combat_ids": [],
    "battle_ids": [],
    "death_ids": [],
    "ending_ids": [],
    "game_flags": [],
    "talents": [],
    "items_book": [],
    "items_misc": [],
    "items_special": [],
    "messages": [],
    "affinity_characters": ["sister1", "brother1", "brother4"],
}

FACING_OPTIONS = ["left", "right"]
# branch.source 取值（契约 §3.1）：mod=本 mod 的 flag 状态，game=官方检查点，
# stat=主角属性数值，flag_value=官方旗标数值，condition=官方条件检查点（bool）
BRANCH_SOURCES_SRC = [
    ("mod", "branch.mod"),
    ("game", "branch.game"),
    ("stat", "branch.stat"),
    ("flag_value", "branch.flag_value"),
    ("condition", "branch.condition"),
]
BRANCH_SOURCES = [
    ("mod", "本 mod 旗标（mod）"),
    ("game", "官方检查点（game）"),
    ("stat", "主角属性数值（stat）"),
    ("flag_value", "官方旗标数值（flag_value）"),
    ("condition", "官方条件检查点（condition）"),
]
# source=mod 时 cases 的 value 只允许 1/2，且最多两行
BRANCH_MOD_VALUES = [(1, "1=已设置"), (2, "2=未设置")]
# source=condition 时 cases 的 value 只允许 1/2（真/假）
BRANCH_COND_VALUES = [(1, "1=真"), (2, "2=假")]
refresh_labels()
# source=stat/flag_value 时 cases 的比较运算符（缺省 >=）
BRANCH_OPS = [(">=", ">="), (">", ">"), ("<=", "<="), ("<", "<"), ("==", "==")]
TEXT_PREVIEW_LEN = 20  # 节点摘要中文本截断长度


# ---------------------------------------------------------------------------
# schema 2 清单访问助手：兼容 {id,name} 对象数组与旧的纯字符串数组；缺键→空
# ---------------------------------------------------------------------------
def entry_id(entry) -> str:
    """清单项的 id：对象取 id，字符串即本身。"""
    if isinstance(entry, dict):
        return str(entry.get("id", ""))
    return str(entry)


def entry_name(entry) -> str:
    """清单项的显示名：对象取 name（缺省回退 id），字符串即本身。"""
    if isinstance(entry, dict):
        return str(entry.get("name") or entry.get("id", ""))
    return str(entry)


def entry_display(entry) -> str:
    """清单项下拉显示：name==id 时只显示 id，否则 "名字（id）"。"""
    i, n = entry_id(entry), entry_name(entry)
    return i if not n or n == i else f"{n}（{i}）"


def list_items(editor_data: dict, key: str) -> list[tuple[str, str]]:
    """取某清单的 [(id, 显示文本)]，兼容两种 schema；清单缺失返回空列表。"""
    items = editor_data.get(key) or []
    if not isinstance(items, list):
        return []
    result = []
    for entry in items:
        item_id = entry_id(entry)
        name = term(key, item_id, default=entry_name(entry))
        display = item_id if not name or name == item_id else f"{name}（{item_id}）"
        result.append((item_id, display))
    return result


def affinity_character_items(editor_data: dict) -> list[tuple[str, str]]:
    """原版人物介绍卡支持的 RelationshipStatType 人物清单。"""
    result = []
    for entry in editor_data.get("affinity_characters") or []:
        char_id = entry_id(entry)
        name = character_name(editor_data, char_id)
        display = char_id if name == char_id else f"{name}（{char_id}）"
        result.append((char_id, display))
    return result


def normalize_character_ids(stories, editor_data: dict) -> int:
    """修复旧编辑器误存的“名称（内部ID）”人物值，返回修复数量。"""
    valid_ids = {
        entry_id(entry) for entry in editor_data.get("characters") or [] if entry_id(entry)
    }
    changed = 0
    iterable = stories.values() if isinstance(stories, dict) else stories
    for story in iterable:
        for node in story.get("nodes") or []:
            value = node.get("character")
            if not isinstance(value, str):
                continue
            match = CHARACTER_DISPLAY_PATTERN.fullmatch(value.strip())
            if match and match.group(1) in valid_ids:
                node["character"] = match.group(1)
                changed += 1
    return changed


def dice_check_items(editor_data: dict) -> list[tuple[str, str]]:
    """骰子检查点下拉清单：仅含带官方元数据的检查点（schema 3 dice_meta）。

    无元数据的检查点会在游戏内导致骰子菜单崩溃（选项条数不足时
    UpdateSelection 索引越界 NRE），因此不在清单中提供。
    """
    meta = editor_data.get("dice_meta") or {}
    items = []
    for cid, disp in list_items(editor_data, "dice_checks"):
        if cid not in meta:
            continue
        bands = meta[cid].get("bands") or []
        dice_max = meta[cid].get("max", "?")
        items.append(
            (
                cid,
                t(
                    "summary.dice_meta",
                    default="%s（骰子%s·%d带）" % (disp, dice_max, len(bands)),
                    name=disp,
                    max=dice_max,
                    bands=len(bands),
                ),
            )
        )
    return items


def display_name(editor_data: dict, key: str, item_id: str) -> str:
    """按清单查某 id 的显示名，查不到返回 id 本身。优先用当前语言的游戏名词。"""
    base = item_id
    for e in editor_data.get(key) or []:
        if entry_id(e) == item_id:
            base = entry_name(e) or item_id
            break
    return term(key, item_id, default=base)


# PyInstaller 冻结态路径推导（源码态行为与打包前完全一致）
FROZEN = bool(getattr(sys, "frozen", False))


def _meipass() -> Path:
    """冻结态打包数据解包目录（onedir 下 dist/<bundle>/_internal）。"""
    return Path(getattr(sys, "_MEIPASS", None) or Path(sys.executable).parent).resolve()


def project_root(editor_dir: Path | None = None) -> Path:
    """项目根：源码态由 editor 目录推导为上一级；冻结态返回打包解包根 _MEIPASS。

    editor_dir 参数保留给旧调用方（源码态 = 其上一级）；冻结态忽略参数。
    """
    if FROZEN:
        return _meipass()
    if editor_dir is not None:
        return Path(editor_dir).resolve().parent
    return Path(__file__).resolve().parent.parent


def editor_dir() -> Path:
    """editor/ 目录：源码态 <仓库根>/editor；冻结态 _MEIPASS（模块平铺在解包根）。"""
    if FROZEN:
        return _meipass()
    return Path(__file__).resolve().parent


def crash_log_path() -> Path:
    """崩溃日志位置：冻结态写当前工作目录（解包目录不应被写入）。"""
    if FROZEN:
        return Path.cwd() / "crash.log"
    return Path(__file__).resolve().parent / "crash.log"


def load_editor_data(proj_root: Path) -> tuple[dict, bool]:
    """读取 <项目根>/data/editor_data.json（契约 §5，schema 1/2 均兼容）。

    返回 (数据, 是否兜底)。文件不存在或损坏时使用内置兜底数据；
    清单字段缺失时保留兜底里的对应清单（优雅降级，不硬崩）。
    """
    path = proj_root / "data" / "editor_data.json"
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        # 关键键缺失时也视为不可用，回退兜底
        if not isinstance(data, dict) or "characters" not in data:
            raise ValueError("editor_data.json 缺少 characters")
        merged = copy.deepcopy(FALLBACK_EDITOR_DATA)
        merged.update(data)
    except Exception:
        return copy.deepcopy(FALLBACK_EDITOR_DATA), True
    # say mode 规范化：契约四种模式必须齐全（旧文件可能缺 center）
    modes = [entry_id(m) for m in merged.get("modes") or []]
    for m in ("character", "think", "narrative", "center"):
        if m not in modes:
            modes.append(m)
    merged["modes"] = modes
    return merged, False


def character_name(editor_data: dict, char_id: str) -> str:
    """按人物 id 查显示名，查不到返回 id 本身。"""
    return display_name(editor_data, "characters", char_id)


def character_combo_items(editor_data: dict) -> tuple[list[tuple[str, str]], list[tuple[str, str]]]:
    """人物下拉：自定义角色 + 官方角色，各自带分组前缀。"""
    custom: list[tuple[str, str]] = []
    try:
        import content_registry

        for rec in content_registry.list_contents(content_type="character"):
            custom.append((rec.ref, "自定义 · %s（%s）" % (rec.name, rec.ref)))
    except Exception:
        custom = []
    official = [
        (item_id, "官方 · %s" % display)
        for item_id, display in list_items(editor_data, "characters")
    ]
    return custom, official


def character_portraits(editor_data: dict, char_id: str) -> list[str]:
    """按人物 id 查表情清单。自定义角色读仓库 content.json；官方读 editor_data。"""
    if isinstance(char_id, str) and char_id.startswith("user:"):
        try:
            import content_registry

            rec, _main = content_registry.resolve(char_id, expected_type="character")
            ids = rec.portrait_ids()
            return ids or ["normal"]
        except Exception:
            return ["normal"]
    for c in editor_data.get("characters", []):
        if entry_id(c) == char_id:
            if isinstance(c, dict):
                return list(c.get("portraits") or DEFAULT_PORTRAITS)
            return list(DEFAULT_PORTRAITS)
    return list(DEFAULT_PORTRAITS)


def new_node(node_type: str, node_id: str, editor_data: dict | None = None) -> dict:
    """新节点工厂：按契约默认值生成节点 dict。"""
    if node_type not in NODE_SCHEMAS:
        raise ValueError(f"未知节点类型: {node_type}")
    node = {"id": node_id, "type": node_type}
    node.update(copy.deepcopy(_NODE_DEFAULTS[node_type]))
    # 人物字段默认取 editor_data 第一个人物，避免空引用
    chars = (editor_data or {}).get("characters") or []
    default_char = entry_id(chars[0]) if chars else ""
    for key, _label, kind, _opt in NODE_SCHEMAS[node_type]["fields"]:
        if kind == "character" and not node.get(key):
            node[key] = default_char
        elif kind == "affinity_character" and not node.get(key):
            affinity = affinity_character_items(editor_data or {})
            node[key] = affinity[0][0] if affinity else ""
    return node


def new_story(story_id: str = "main", editor_data: dict | None = None) -> dict:
    """新建空剧情：登场 + 对白开场；mood 默认 false（隐藏官方心情气泡）。

    动作人物必须先登场再做动作，否则游戏会因“角色不存在”崩掉剧情协程
    （黑屏），所以起始结构固定为 show → say。
    """
    draft: dict = {"nodes": []}
    entrance = new_node("show", make_node_id(draft, "show"), editor_data)
    entrance["position"] = "M"
    draft["nodes"].append(entrance)
    first = new_node("say", make_node_id(draft, "say"), editor_data)
    return {
        "id": story_id,
        "title": t("new_story_title", default="新剧情"),
        "mood": False,
        "start": entrance["id"],
        "nodes": [entrance, first],
    }


def load_story(path: Path) -> dict:
    """读取 story.json 并做最基本结构校验。"""
    try:
        story = json.loads(Path(path).read_text(encoding="utf-8"))
    except OSError as exc:
        raise ValueError("story.json 读取失败: %s" % exc) from exc
    except json.JSONDecodeError as exc:
        raise ValueError("story.json 不是合法 JSON: %s" % exc) from exc
    if not isinstance(story, dict) or not isinstance(story.get("nodes"), list):
        raise ValueError("story.json 结构非法：缺少 nodes 数组")
    story.setdefault("id", "main")
    story.setdefault("title", story["id"])
    story.setdefault("mood", False)
    story.setdefault("start", story["nodes"][0]["id"] if story["nodes"] else "")
    for node in story["nodes"]:
        if "id" not in node or "type" not in node:
            raise ValueError("story.json 结构非法：节点缺少 id/type")
    return story


def save_story(story: dict, path: Path) -> None:
    """原子写出 story.json；失败时保留原文件并清理临时文件。"""
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(story, ensure_ascii=False, indent=2) + "\n"
    temp_path = None
    try:
        fd, raw_temp = tempfile.mkstemp(
            prefix=target.name + ".", suffix=".tmp", dir=str(target.parent)
        )
        temp_path = Path(raw_temp)
        with os.fdopen(fd, "w", encoding="utf-8", newline="") as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp_path, target)
        temp_path = None
    finally:
        if temp_path is not None:
            try:
                temp_path.unlink()
            except OSError:
                pass


_GOTO_KEYS = ("goto", "goto_大成功", "goto_成功", "goto_失败")


def retarget_node_ids(story: dict, mapping: dict[str, str]) -> int:
    """把 story 内节点 id 与全部跳转引用按 mapping 改写，返回改写次数。"""
    if not mapping:
        return 0
    changed = 0
    start = story.get("start")
    if isinstance(start, str) and start in mapping:
        story["start"] = mapping[start]
        changed += 1
    for node in story.get("nodes") or []:
        if not isinstance(node, dict):
            continue
        nid = node.get("id")
        if isinstance(nid, str) and nid in mapping:
            node["id"] = mapping[nid]
            changed += 1
        value = node.get("goto")
        if isinstance(value, str) and value in mapping:
            node["goto"] = mapping[value]
            changed += 1
        for option in node.get("options") or []:
            if not isinstance(option, dict):
                continue
            for key in _GOTO_KEYS:
                target = option.get(key)
                if isinstance(target, str) and target in mapping:
                    option[key] = mapping[target]
                    changed += 1
        for case in node.get("cases") or []:
            if not isinstance(case, dict):
                continue
            target = case.get("goto")
            if isinstance(target, str) and target in mapping:
                case["goto"] = mapping[target]
                changed += 1
    return changed


def rename_node(story: dict, old_id: str, new_id: str) -> int:
    """重命名节点 id，并同步 start / goto / 选项 / 分支 / 骰子去向。"""
    old_id = str(old_id or "").strip()
    new_id = str(new_id or "").strip()
    if not old_id:
        raise ValueError("原节点编号为空")
    if old_id == new_id:
        return 0
    if not NODE_ID_PATTERN.fullmatch(new_id):
        raise ValueError("节点编号只使用英文字母、数字或下划线")
    ids = [n.get("id") for n in story.get("nodes") or [] if isinstance(n, dict)]
    if old_id not in ids:
        raise ValueError("节点不存在: %s" % old_id)
    if new_id in ids:
        raise ValueError("节点编号已被占用: %s" % new_id)
    return retarget_node_ids(story, {old_id: new_id})


def reorder_node(story: dict, from_index: int, to_index: int) -> int:
    """把 nodes[from_index] 挪到插入点 to_index（移动前下标），返回最终下标。"""
    nodes = story.setdefault("nodes", [])
    if not (0 <= from_index < len(nodes)):
        raise ValueError("源步骤下标越界")
    dest = max(0, min(int(to_index), len(nodes)))
    if dest == from_index or dest == from_index + 1:
        return from_index
    node = nodes.pop(from_index)
    if dest > from_index:
        dest -= 1
    nodes.insert(dest, node)
    return dest


def make_node_id(story: dict, node_type: str | None = None, prefix: str | None = None) -> str:
    """生成 story 内唯一节点 id：类型 + 该类型次序，例如 say1、show2、choice1。

    旧故事里的 n1/n2 保持不动。prefix 仍可用，便于试玩前导等特殊编号。
    """
    stem = (prefix or node_type or "n").strip()
    if not NODE_ID_PATTERN.fullmatch(stem):
        stem = "n"
    used = {n.get("id") for n in story.get("nodes", [])}
    i = 1
    while f"{stem}{i}" in used:
        i += 1
    return f"{stem}{i}"


def make_story_id(stories: dict) -> str:
    """生成项目内唯一的剧情脚本 id：main、story2、story3……"""
    i = 1
    while True:
        candidate = "main" if i == 1 else f"story{i}"
        if candidate not in stories:
            return candidate
        i += 1


def _short(text: str, limit: int = TEXT_PREVIEW_LEN) -> str:
    text = (text or "").replace("\n", " ").strip()
    return text[:limit] + ("…" if len(text) > limit else "")


def _signed(value) -> str:
    try:
        v = int(value)
    except (TypeError, ValueError):
        return str(value)
    return f"+{v}" if v > 0 else str(v)


def node_bullet(node_type: str) -> str:
    """步骤列表前缀符号：分支/结局与普通步骤区分开。"""
    if node_type in ("choice", "branch", "dice"):
        return "◆"
    if node_type in ("end", "death", "goto_scene"):
        return "■"
    return "●"


def node_list_caption(node: dict, editor_data: dict | None = None) -> tuple[str, str]:
    """步骤列表两行文案：(标题行, 详情行)。

    标题：类型中文名；详情：人物/站位/摘要，不再用 n1·武师@M 这种程序员串。
    """
    ed = editor_data or FALLBACK_EDITOR_DATA
    nt = node.get("type", "?")
    tcn = NODE_TYPE_CN.get(nt, nt)
    summary = node_summary(node, ed)
    # node_summary 形如 "对白·武师: 文本…"——拆成类型 + 其余
    if summary.startswith(tcn + "·"):
        detail = summary[len(tcn) + 1 :].strip()
    elif summary.startswith(tcn):
        detail = summary[len(tcn) :].lstrip("· ").strip()
    else:
        detail = summary
    # 站位代号换成中文名（show 的 "武师@M" → "武师 · 中"）
    if nt == "show":
        pos = display_name(ed, "positions", node.get("position", "")) or node.get(
            "position", ""
        )
        who = character_name(ed, node.get("character", "")) or t(
            "form.unselected", default="（未选）"
        )
        detail = f"{who} · {pos}" if pos else who
    elif nt == "say":
        mode = node.get("mode", "character")
        who = {
            "narrative": MODE_CN.get("narrative", "旁白"),
            "think": t("summary.think_short", default="内心"),
            "center": MODE_CN.get("center", "居中旁白"),
        }.get(
            mode,
            character_name(ed, node.get("character", ""))
            or t("form.unselected", default="（未选）"),
        )
        text = _short(node.get("text", ""), 28)
        detail = f"{who} · {text}" if text else who
    return tcn, detail or t("form.empty", default="（未填写）")


def node_summary(node: dict, editor_data: dict | None = None) -> str:
    """节点列表里的一行中文摘要，如 "对白·唐惟元: 文本前20字…"。"""
    ed = editor_data or FALLBACK_EDITOR_DATA
    nt = node.get("type", "?")
    tcn = NODE_TYPE_CN.get(nt, nt)

    def cname() -> str:
        return character_name(ed, node.get("character", "")) or t(
            "form.unselected", default="（未选）"
        )

    def stat_name(key) -> str:
        return display_name(ed, "stats", str(key or ""))

    if nt == "say":
        mode = node.get("mode", "character")
        who = {
            "narrative": MODE_CN.get("narrative", "旁白"),
            "think": t("summary.think_short", default="内心"),
            "center": MODE_CN.get("center", "居中旁白"),
        }.get(mode, cname())
        extra = " 🔊" if node.get("voice") else ""
        return f"{tcn}·{who}{extra}: {_short(node.get('text', ''))}"
    if nt == "music":
        op = node.get("op", "play")
        extra = "" if op == "play" else f"（{enum_label('music_op', op)}）"
        return f"{tcn}·{node.get('name', '')}{extra}"
    if nt == "sound":
        return f"{tcn}·{node.get('name', '')}"
    if nt == "scene":
        return f"{tcn}·{display_name(ed, 'views', node.get('view', ''))}"
    if nt == "background":
        action = enum_label("background_action", node.get("action", "show"))
        image = node.get("image") or ""
        return f"{tcn}·{action}" + (f" {image}" if image else "")
    if nt == "custom_cg":
        action = enum_label("cg_action", node.get("action", "show"))
        image = node.get("image") or ""
        return f"{tcn}·{action}" + (f" {image}" if image else "")
    if nt == "overlay":
        action = enum_label("overlay_action", node.get("action", "show"))
        return f"{tcn}·{node.get('slot', 'main')} {action}"
    if nt == "show":
        return f"{tcn}·{cname()}@{node.get('position', '')}"
    if nt == "move":
        return f"{tcn}·{cname()} {node.get('from', '')}→{node.get('to', '')}"
    if nt == "face":
        return f"{tcn}·{cname()}→{node.get('facing', '')}"
    if nt == "hide":
        return f"{tcn}·{cname()}"
    if nt == "focus":
        return f"{tcn}·{cname()}"
    if nt == "offset":
        return f"{tcn}·{cname()} ({node.get('x', 0)},{node.get('y', 0)})"
    if nt == "choice":
        return f"{tcn}·" + t(
            "summary.options",
            default="{n}个选项",
            n=len(node.get("options", [])),
        )
    if nt == "shock":
        return f"{tcn}·{cname()}"
    if nt == "mask":
        return f"{tcn}·" + (
            t("summary.on", default="开")
            if node.get("show")
            else t("summary.off", default="关")
        )
    if nt == "intro":
        if node.get("intro_source", "official") == "custom":
            return (
                f"{tcn}·{t('summary.custom', default='自定义')}·"
                f"{node.get('name') or t('summary.no_name', default='未填写姓名')}"
            )
        return f"{tcn}·{t('summary.official', default='原版')}·{cname()}"
    if nt == "effect":
        name = node.get("name", "")
        return f"{tcn}·{name}" + (
            t("summary.stop", default="（停止）") if node.get("play") is False else ""
        )
    if nt == "transition":
        return (
            f"{tcn}·{enum_label('transition_phase', node.get('phase', 'in'))}"
            f"（{enum_label('transition_dir', node.get('dir', 'lr'))}）"
        )
    if nt == "camera":
        return f"{tcn}·{node.get('name', '')}" + (
            t("summary.on", default="开")
            if node.get("active")
            else t("summary.off", default="关")
        )
    if nt == "block":
        return f"{tcn}·{node.get('flowchart', '')}.{node.get('name', '')}"
    if nt == "cg":
        return (
            f"{tcn}·{enum_label('cg_action', node.get('action', 'show'))}"
            f"{enum_label('cg_kind', node.get('kind', 'picture'))}"
            f" {_short(str(node.get('key') or ''))}"
        )
    if nt == "dim":
        return f"{tcn}·{cname()}" + (
            t("summary.on", default="开")
            if node.get("dimmed")
            else t("summary.off", default="关")
        )
    if nt == "message":
        return f"{tcn}·{_short(node.get('text', ''))}"
    if nt == "rotate":
        return f"{tcn}·{cname()} {node.get('angle', 0)}°"
    if nt == "dayenv":
        return f"{tcn}·" + (
            t("summary.day", default="白天")
            if node.get("day_type") == 1
            else t("summary.night", default="晚上")
        )
    if nt == "stat":
        return f"{tcn}·{stat_name(node.get('key'))}{_signed(node.get('delta', 0))}"
    if nt == "stat_set":
        return f"{tcn}·{stat_name(node.get('key'))}={node.get('value', 0)}"
    if nt == "affinity":
        return f"{tcn}·{cname()}{_signed(node.get('delta', 0))}"
    if nt == "talent":
        return (
            f"{tcn}·{display_name(ed, 'talents', node.get('talent', ''))}"
            f"{_signed(node.get('level', 0))}"
        )
    if nt == "item":
        verb = (
            t("summary.remove", default="移除")
            if node.get("remove")
            else t("summary.gain", default="获得")
        )
        iname = display_name(
            ed, f"items_{node.get('kind', 'misc')}", node.get("item", "")
        )
        return f"{tcn}·{verb} {iname}×{node.get('count', 1)}"
    if nt == "flag":
        return f"{tcn}·{node.get('flag', '')}"
    if nt == "game_flag":
        return f"{tcn}·{node.get('flag', '')}={node.get('value', 0)}"
    if nt == "enemy":
        return (
            f"{tcn}·{enum_label('enemy_op', node.get('op', 'team'))}"
            f" {node.get('enemy', '')} {_signed(node.get('value', 0))}"
        )
    if nt == "battle_skill":
        return (
            f"{tcn}·{enum_label('battle_skill_op', node.get('op', 'set'))}"
            f" {node.get('key', '')}"
        )
    if nt == "mission":
        return f"{tcn}·{node.get('name', '')} {node.get('key', '')}"
    if nt == "time":
        return f"{tcn}·{enum_label('time_op', node.get('op', 'round'))}"
    if nt == "autosave":
        return f"{tcn}·{enum_label('autosave_kind', node.get('kind', 'story'))}"
    if nt == "branch":
        src = {
            "mod": t("branch.src.mod", default="本mod"),
            "game": t("branch.src.game", default="官方"),
            "stat": t("branch.src.stat", default="属性"),
            "flag_value": t("branch.src.flag_value", default="官方旗标"),
            "condition": t("branch.src.condition", default="条件"),
        }.get(node.get("source", "mod"), "?")
        key = node.get("stat") or node.get("flag", "")
        return f"{tcn}·{src}:{key}(" + t(
            "summary.branches",
            default="{n}支",
            n=len(node.get("cases", [])),
        ) + ")"
    if nt == "dice":
        return f"{tcn}·{node.get('check', '')}(" + t(
            "summary.items",
            default="{n}项",
            n=len(node.get("options", [])),
        ) + ")"
    if nt == "goto_scene":
        key = node.get("key") or ""
        title = node.get("title") or ""
        return f"{tcn}·{enum_label('goto_scene', node.get('scene', 'Free'))}" + (
            (f" {key}" if key else "") + (f"「{_short(title)}」" if title else "")
        )
    if nt == "panel":
        return f"{tcn}·{enum_label('panel', node.get('panel', ''))}"
    if nt == "wait":
        return f"{tcn}·" + t(
            "summary.seconds", default="{n}秒", n=node.get("seconds", 0)
        )
    if nt == "end":
        nxt = node.get("next_script")
        return f"{tcn}→{nxt}" if nxt else f"{tcn}·{t('summary.end', default='结束')}"
    if nt == "death":
        return (
            f"{tcn}·{_short(node.get('text', ''))} → GameOver("
            f"{node.get('death_id', '')})/{node.get('next', 'Title')}"
        )
    if nt == "raw":
        first = (node.get("code") or "").strip().splitlines()
        return f"{tcn}·{_short(first[0] if first else '')}"
    return f"{tcn}·{node.get('id', '')}"
