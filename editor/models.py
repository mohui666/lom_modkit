# -*- coding: utf-8 -*-
"""数据模型：story.json 读写、节点工厂、节点摘要、editor_data.json 加载。

格式契约见 docs/mod_format.md §3（节点类型）与 §5（editor_data.json）。
节点在编辑器内部就是普通 dict（与 JSON 结构一致），本模块只管结构约束与默认值。
"""

from __future__ import annotations

import copy
import json
import re
import sys
from pathlib import Path

# story 脚本 id / 节点 id 规则（契约 §1）
ID_PATTERN = re.compile(r"^[a-zA-Z0-9_\-]+$")
# manifest mod id 比 story id 更严格：运行时注册名与包格式只接受小写。
MOD_ID_PATTERN = re.compile(r"^[a-z0-9_\-]+$")
CHARACTER_DISPLAY_PATTERN = re.compile(r"^.*（([a-zA-Z0-9_\-]+)）$")

# ---------------------------------------------------------------------------
# 节点类型中文名（契约 §3.1 全量 43 种）
# ---------------------------------------------------------------------------
NODE_TYPE_CN: dict[str, str] = {
    "music": "音乐",
    "sound": "音效",
    "scene": "切换背景",
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

# 新手菜单最先展示的高频步骤。特殊的“汗青书结局”由主窗口用 goto_scene
# 预设创建，不在这里重复列出。
COMMON_NODE_TYPES: list[str] = [
    "say",
    "show",
    "hide",
    "intro",
    "scene",
    "choice",
    "dice",
    "wait",
    "end",
    "death",
]

# 节点表单顶部的上下文提示。只解释当前动作，不复述控件名称；完整流程见内置帮助。
NODE_HELP: dict[str, str] = {
    "say": "写人物对白、内心独白或旁白。旁白模式不需要选择人物。",
    "show": "让人物立绘出现在画面中。先登场再对白，游戏效果更稳定。",
    "hide": "让指定人物退出画面。",
    "scene": "切换剧情背景。编辑器预览只用于确认构图，不包含原版素材。",
    "choice": "给玩家 2～4 个选项；每个选项都要选择后续步骤。",
    "dice": "使用官方骰子检查点，并分别指定成功、失败等结果的后续步骤。",
    "goto_scene": "离开当前剧情并进入战斗、标题、死亡或汗青书结局等场景。",
    "end": "结束当前章节。留空“下一章节”时回到自由模式。",
    "death": "显示官方样式的死亡画面。正文必填，专用编号使用 900000 以上。",
    "intro": "官方人物会直接使用游戏内介绍和头像；自定义人物可填写称号、姓名与正文。",
    "raw": "高级功能：代码会原样进入 Lua。普通剧情不需要使用。",
}

# 契约 §3.1 分组（新增节点菜单按此分组显示）
NODE_GROUPS: list[tuple[str, list[str]]] = [
    (
        "画面与声音",
        [
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
            "dim",
            "message",
            "rotate",
            "dayenv",
        ],
    ),
    (
        "数值、物品与任务",
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
        "流程与高级功能",
        ["branch", "dice", "goto_scene", "panel", "wait", "end", "death", "raw"],
    ),
]

# ---------------------------------------------------------------------------
# 枚举字段的中文标注（表单 kind="enum:<名字>" 用）
# ---------------------------------------------------------------------------
ENUM_SETS: dict[str, list[tuple[str, str]]] = {
    "music_op": [("play", "播放"), ("stop", "停止"), ("fadeout", "淡出")],
    "sound_kind": [("sound", "音效"), ("env", "环境音")],
    "sound_op": [("play", "播放"), ("fadeout", "淡出")],
    "transition_phase": [("in", "淡入"), ("out", "淡出")],
    "transition_dir": [
        ("lr", "从左到右"),
        ("rl", "从右到左"),
        ("tb", "从上到下"),
        ("bt", "从下到上"),
    ],
    "cg_action": [("show", "显示"), ("hide", "隐藏")],
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
        ("custom", "自定义人物资料"),
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
# 切换后需要重建表单的枚举（其它字段或可见字段随它变化）
REBUILD_ENUMS = {"item_kind", "goto_scene", "mode", "intro_source"}

# say mode 中文标注（清单本身来自 editor_data.modes）
MODE_CN = {
    "character": "对话",
    "think": "内心独白",
    "narrative": "旁白",
    "center": "居中旁白",
}
FACING_CN = [("left", "朝左"), ("right", "朝右")]
# 镜头滤镜常见预设（契约 §3.1 示例）
CAMERA_PRESETS = ["stage-memory", "stage-dream", "stage-fire", "stage-blurdim"]


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
            ("name", "音效名", "line", False),
            ("kind", "声道", "enum:sound_kind", True),
            ("op", "操作", "enum:sound_op", True),
            ("seconds", "淡出秒数", "float", True),
        ],
    },
    "scene": {
        "label": "切换场景",
        "fields": [("view", "场景", "view", False)],
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
    return [(entry_id(e), entry_display(e)) for e in items]


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
        items.append((cid, "%s（骰子%s·%d带）" % (disp, dice_max, len(bands))))
    return items


def display_name(editor_data: dict, key: str, item_id: str) -> str:
    """按清单查某 id 的显示名，查不到返回 id 本身。"""
    for e in editor_data.get(key) or []:
        if entry_id(e) == item_id:
            return entry_name(e) or item_id
    return item_id


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


def character_portraits(editor_data: dict, char_id: str) -> list[str]:
    """按人物 id 查表情清单，查不到返回通用默认表情。"""
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
    """新建空剧情：含一个 say 起始节点；mood 默认 false（隐藏官方心情气泡）。"""
    first = new_node("say", "n1", editor_data)
    return {
        "id": story_id,
        "title": "新剧情",
        "mood": False,
        "start": first["id"],
        "nodes": [first],
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
    """写出 story.json（UTF-8、缩进 2、保留中文）。"""
    Path(path).write_text(
        json.dumps(story, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )


def make_node_id(story: dict, prefix: str = "n") -> str:
    """生成 story 内唯一的节点 id：n1、n2……"""
    used = {n.get("id") for n in story.get("nodes", [])}
    i = 1
    while f"{prefix}{i}" in used:
        i += 1
    return f"{prefix}{i}"


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


def node_summary(node: dict, editor_data: dict | None = None) -> str:
    """节点列表里的一行中文摘要，如 "对白·唐惟元: 文本前20字…"。"""
    ed = editor_data or FALLBACK_EDITOR_DATA
    t = node.get("type", "?")
    tcn = NODE_TYPE_CN.get(t, t)

    def cname() -> str:
        return character_name(ed, node.get("character", "")) or "（未选）"

    def stat_name(key) -> str:
        return display_name(ed, "stats", str(key or ""))

    if t == "say":
        mode = node.get("mode", "character")
        who = {"narrative": "旁白", "think": "内心", "center": "居中旁白"}.get(
            mode, cname()
        )
        return f"{tcn}·{who}: {_short(node.get('text', ''))}"
    if t == "music":
        op = node.get("op", "play")
        extra = "" if op == "play" else f"（{enum_label('music_op', op)}）"
        return f"{tcn}·{node.get('name', '')}{extra}"
    if t == "sound":
        return f"{tcn}·{node.get('name', '')}"
    if t == "scene":
        return f"{tcn}·{display_name(ed, 'views', node.get('view', ''))}"
    if t == "show":
        return f"{tcn}·{cname()}@{node.get('position', '')}"
    if t == "move":
        return f"{tcn}·{cname()} {node.get('from', '')}→{node.get('to', '')}"
    if t == "face":
        return f"{tcn}·{cname()}→{node.get('facing', '')}"
    if t == "hide":
        return f"{tcn}·{cname()}"
    if t == "focus":
        return f"{tcn}·{cname()}"
    if t == "offset":
        return f"{tcn}·{cname()} ({node.get('x', 0)},{node.get('y', 0)})"
    if t == "choice":
        return f"{tcn}·{len(node.get('options', []))}个选项"
    if t == "shock":
        return f"{tcn}·{cname()}"
    if t == "mask":
        return f"{tcn}·{'开' if node.get('show') else '关'}"
    if t == "intro":
        if node.get("intro_source", "official") == "custom":
            return f"{tcn}·自定义·{node.get('name') or '未填写姓名'}"
        return f"{tcn}·原版·{cname()}"
    if t == "effect":
        name = node.get("name", "")
        return f"{tcn}·{name}" + ("（停止）" if node.get("play") is False else "")
    if t == "transition":
        return (
            f"{tcn}·{enum_label('transition_phase', node.get('phase', 'in'))}"
            f"（{enum_label('transition_dir', node.get('dir', 'lr'))}）"
        )
    if t == "camera":
        return f"{tcn}·{node.get('name', '')}{'开' if node.get('active') else '关'}"
    if t == "block":
        return f"{tcn}·{node.get('flowchart', '')}.{node.get('name', '')}"
    if t == "cg":
        return (
            f"{tcn}·{enum_label('cg_action', node.get('action', 'show'))}"
            f"{enum_label('cg_kind', node.get('kind', 'picture'))}"
            f" {_short(str(node.get('key') or ''))}"
        )
    if t == "dim":
        return f"{tcn}·{cname()}{'开' if node.get('dimmed') else '关'}"
    if t == "message":
        return f"{tcn}·{_short(node.get('text', ''))}"
    if t == "rotate":
        return f"{tcn}·{cname()} {node.get('angle', 0)}°"
    if t == "dayenv":
        return f"{tcn}·{'白天' if node.get('day_type') == 1 else '晚上'}"
    if t == "stat":
        return f"{tcn}·{stat_name(node.get('key'))}{_signed(node.get('delta', 0))}"
    if t == "stat_set":
        return f"{tcn}·{stat_name(node.get('key'))}={node.get('value', 0)}"
    if t == "affinity":
        return f"{tcn}·{cname()}{_signed(node.get('delta', 0))}"
    if t == "talent":
        return (
            f"{tcn}·{display_name(ed, 'talents', node.get('talent', ''))}"
            f"{_signed(node.get('level', 0))}"
        )
    if t == "item":
        verb = "移除" if node.get("remove") else "获得"
        iname = display_name(
            ed, f"items_{node.get('kind', 'misc')}", node.get("item", "")
        )
        return f"{tcn}·{verb} {iname}×{node.get('count', 1)}"
    if t == "flag":
        return f"{tcn}·{node.get('flag', '')}"
    if t == "game_flag":
        return f"{tcn}·{node.get('flag', '')}={node.get('value', 0)}"
    if t == "enemy":
        return (
            f"{tcn}·{enum_label('enemy_op', node.get('op', 'team'))}"
            f" {node.get('enemy', '')} {_signed(node.get('value', 0))}"
        )
    if t == "battle_skill":
        return (
            f"{tcn}·{enum_label('battle_skill_op', node.get('op', 'set'))}"
            f" {node.get('key', '')}"
        )
    if t == "mission":
        return f"{tcn}·{node.get('name', '')} {node.get('key', '')}"
    if t == "time":
        return f"{tcn}·{enum_label('time_op', node.get('op', 'round'))}"
    if t == "autosave":
        return f"{tcn}·{enum_label('autosave_kind', node.get('kind', 'story'))}"
    if t == "branch":
        src = {
            "mod": "本mod",
            "game": "官方",
            "stat": "属性",
            "flag_value": "官方旗标",
            "condition": "条件",
        }.get(node.get("source", "mod"), "?")
        key = node.get("stat") or node.get("flag", "")
        return f"{tcn}·{src}:{key}({len(node.get('cases', []))}支)"
    if t == "dice":
        return f"{tcn}·{node.get('check', '')}({len(node.get('options', []))}项)"
    if t == "goto_scene":
        key = node.get("key") or ""
        title = node.get("title") or ""
        return f"{tcn}·{enum_label('goto_scene', node.get('scene', 'Free'))}" + (
            (f" {key}" if key else "") + (f"「{_short(title)}」" if title else "")
        )
    if t == "panel":
        return f"{tcn}·{enum_label('panel', node.get('panel', ''))}"
    if t == "wait":
        return f"{tcn}·{node.get('seconds', 0)}秒"
    if t == "end":
        nxt = node.get("next_script")
        return f"{tcn}→{nxt}" if nxt else f"{tcn}·结束"
    if t == "death":
        return (
            f"{tcn}·{_short(node.get('text', ''))} → GameOver("
            f"{node.get('death_id', '')})/{node.get('next', 'Title')}"
        )
    if t == "raw":
        first = (node.get("code") or "").strip().splitlines()
        return f"{tcn}·{_short(first[0] if first else '')}"
    return f"{tcn}·{node.get('id', '')}"
