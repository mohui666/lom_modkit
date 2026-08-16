# -*- coding: utf-8 -*-
"""演出预览：舞台状态推演 + 自绘舞台控件。

- simulate_stage()：从 story 的 start 节点按"数组序 + goto"推演到指定节点，
  得出舞台状态（当前场景、台上人物站位/表情/朝向、当前对白/选项）。
  choice 走第一个选项，branch 优先走 value=1 的分支，上限 500 步防死循环。
- StagePreview：QWidget 自绘 16:9 舞台（背景 + 立绘 + 对白栏 + 选项按钮）。
  选项按钮可点击，点击后发 choice_activated 信号通知主窗口跳转到 goto 节点。

立绘/背景图来自 <项目根>/data/preview_map.json（路径相对 data/）；
文件或图片缺失时一律走占位图逻辑。
"""

from __future__ import annotations

import json
import traceback
from collections import OrderedDict
from datetime import datetime
from math import floor
from pathlib import Path

from PySide6.QtCore import QRect, QRectF, Qt, Signal
from PySide6.QtGui import QColor, QFont, QFontMetrics, QPainter, QPen, QPixmap, QTransform
from PySide6.QtWidgets import QWidget

from asset_store import resolve_image_asset
import content_registry
import models
import story_graph

EDITOR_DIR = models.editor_dir()
PROJECT_ROOT = models.project_root()

MAX_STEPS = 500  # 推演步数上限（防 goto 死循环）

# 崩溃取证日志（main.py 的 excepthook 也写这里）；冻结态写 CWD（见 models）
CRASH_LOG = models.crash_log_path()


def log_crash(text: str) -> None:
    """追加写崩溃日志；日志本身绝不能再抛异常。"""
    try:
        with open(CRASH_LOG, "a", encoding="utf-8") as f:
            f.write(
                f"\n===== {datetime.now():%Y-%m-%d %H:%M:%S} =====\n{text.rstrip()}\n"
            )
    except Exception:
        pass


# 站位 → 舞台 x 比例（0=左缘，1=右缘；S 系为屏幕外，画一半）
POSITION_X: dict[str, float] = {
    "SL": -0.06,
    "SR": 1.06,
    "L1": 0.12,
    "L2": 0.22,
    "L3": 0.30,
    "LM1": 0.34,
    "LM2": 0.40,
    "M": 0.50,
    "C": 0.50,
    "RM2": 0.60,
    "RM1": 0.66,
    "R3": 0.70,
    "R2": 0.78,
    "R1": 0.88,
}

# 颜色
BG_FALLBACK = QColor(43, 43, 43)  # 无背景图时的深灰底
DIALOG_BG = QColor(0, 0, 0, 175)  # 对白栏底色
PLACEHOLDER_BG = QColor(90, 90, 100, 220)  # 人物占位框底色
PLACEHOLDER_EDGE = QColor(180, 180, 190)
CHOICE_BG = QColor(24, 24, 28, 225)  # 选项按钮底色
CHOICE_EDGE = QColor(235, 235, 235)

# 图片缓存上限（崩溃修复：原实现缓存无上限，真实素材单张解码约 20MB，
# 浏览稍多即吃光内存被系统强杀 → pythonw 下无声崩溃）
MAX_CACHE_ENTRIES = 60  # LRU 最多缓存张数
MAX_CACHE_BYTES = 256 * 1024 * 1024  # LRU 解码字节预算（约 256MB）
MAX_IMAGE_DIM = 1600  # 加载即降采样到该边长以内


# ---------------------------------------------------------------------------
# preview_map.json 加载
# ---------------------------------------------------------------------------
def load_preview_map(proj_root: Path) -> tuple[dict, Path]:
    """读取 <项目根>/data/preview_map.json。

    返回 (素材映射, data 目录)。文件不存在/损坏时返回 ({}, data 目录)，
    调用方按"无预览素材，使用占位图"处理。
    """
    data_dir = proj_root / "data"
    try:
        m = json.loads((data_dir / "preview_map.json").read_text(encoding="utf-8"))
        if not isinstance(m, dict):
            raise ValueError("preview_map.json 顶层应为对象")
        return m, data_dir
    except Exception:
        return {}, data_dir


# ---------------------------------------------------------------------------
# 舞台状态推演
# ---------------------------------------------------------------------------
def position_x(position: str) -> tuple[float, bool]:
    """站位字符串 → (舞台 x 比例, 是否识别成功)。识别失败兜底中央。"""
    p = (position or "").strip().upper()
    if p in POSITION_X:
        return POSITION_X[p], True
    if p in ("M", "C"):
        return 0.5, True
    # 宽松解析：L/R 开头 + 数字；带 M 的（如 LM2/RM2）中间偏
    if len(p) >= 2 and p[0] in ("L", "R"):
        side = -1.0 if p[0] == "L" else 1.0
        digits = "".join(ch for ch in p if ch.isdigit())
        try:
            n = int(digits) if digits else 2
        except ValueError:
            n = 2
        if "M" in p[1:]:
            offset = 0.10  # 带 M：中间偏
        else:
            offset = {1: 0.38, 2: 0.28, 3: 0.20}.get(n, 0.30)
        x = 0.5 + side * offset
        if p.startswith("S"):  # 不会到这里（S 开头不在 L/R），防御而已
            x = -0.06 if side < 0 else 1.06
        return x, True
    return 0.5, False


def _hint_text(node: dict, ed: dict) -> str | None:
    """无法舞台化的节点类型：生成一行中文说明（预览在对白栏位置灰字显示）。

    返回 None 表示该类型有真实的舞台呈现（show/move/face/hide/scene/say/choice）。
    """
    t = str(node.get("type") or "")
    if t in (
        "show",
        "move",
        "face",
        "hide",
        "scene",
        "background",
        "custom_cg",
        "overlay",
        "say",
        "choice",
        "offset",
        "shock",
        "dim",
        "rotate",
    ):
        return None

    def cname() -> str:
        return models.character_name(ed, node.get("character", ""))

    if t == "music":
        op = node.get("op", "play")
        if op == "stop":
            return "[音乐] 停止"
        if op == "fadeout":
            return f"[音乐] 淡出（{node.get('seconds', 2)} 秒）"
        return f"[音乐] 播放 {node.get('name', '')}"
    if t == "sound":
        kind = models.enum_label("sound_kind", node.get("kind", "sound"))
        if node.get("op", "play") == "fadeout":
            return f"[{kind}] 淡出（{node.get('seconds', 1)} 秒）"
        return f"[{kind}] 播放 {node.get('name', '')}"
    if t == "focus":
        return f"[镜头聚焦] {cname()}"
    if t == "offset":
        return (
            f"[人物位移] {cname()} 偏移 ({node.get('x', 0)}, "
            f"{node.get('y', 0)})，{node.get('duration', 0)} 秒"
        )
    if t == "shock":
        return f"[人物震动] {cname()}（{node.get('duration', 0.5)} 秒）"
    if t == "mask":
        return f"[独白遮罩] {'显示' if node.get('show') else '隐藏'}"
    if t == "intro":
        if node.get("intro_source", "official") == "custom":
            return f"[人物介绍卡] {node.get('name') or '未填写姓名'}：{node.get('text') or ''}"
        return f"[人物介绍卡] 原版资料 · {cname()}"
    if t == "effect":
        return f"[屏幕特效] {node.get('name', '')}"
    if t == "transition":
        return (
            f"[转场] {models.enum_label('transition_phase', node.get('phase', 'in'))}"
            f"·{models.enum_label('transition_dir', node.get('dir', 'lr'))}"
        )
    if t == "camera":
        return (
            f"[镜头滤镜] {node.get('name', '')} "
            f"{'启用' if node.get('active') else '关闭'}"
        )
    if t == "block":
        return f"[流图块] {node.get('flowchart', '')}.{node.get('name', '')}"
    if t == "cg":
        return (
            f"[图片/标题] {models.enum_label('cg_action', node.get('action', 'show'))}"
            f"{models.enum_label('cg_kind', node.get('kind', 'picture'))}"
            f" {node.get('key') or ''}"
        )
    if t == "dim":
        return f"[人物压暗] {cname()} {'开' if node.get('dimmed') else '关'}"
    if t == "message":
        text = str(node.get("text") or "").replace("\n", " ").strip()
        return f"[系统提示] {text[:20]}{'…' if len(text) > 20 else ''}"
    if t == "rotate":
        return f"[人物旋转] {cname()} {node.get('angle', 0)}°"
    if t == "dayenv":
        return f"[日夜环境] {'白天' if node.get('day_type') == 1 else '晚上'}"
    if t == "stat":
        return (
            f"[属性] {models.display_name(ed, 'stats', node.get('key', ''))} "
            f"{node.get('delta', 0):+d}"
            if isinstance(node.get("delta"), int)
            else f"[属性] {node.get('key', '')} {node.get('delta', 0)}"
        )
    if t == "stat_set":
        return (
            f"[属性设置] {models.display_name(ed, 'stats', node.get('key', ''))} "
            f"= {node.get('value', 0)}"
        )
    if t == "affinity":
        delta = node.get("delta", 0)
        sign = f"{delta:+d}" if isinstance(delta, int) else str(delta)
        return f"[好感度] {cname()} {sign}"
    if t == "talent":
        return (
            f"[天赋] {models.display_name(ed, 'talents', node.get('talent', ''))} "
            f"等级 {node.get('level', 0):+d}"
        )
    if t == "item":
        verb = "移除" if node.get("remove") else "获得"
        iname = models.display_name(
            ed, f"items_{node.get('kind', 'misc')}", node.get("item", "")
        )
        return f"[物品] {verb} {iname} ×{node.get('count', 1)}"
    if t == "flag":
        return f"[剧情旗标] {node.get('flag', '')}"
    if t == "game_flag":
        return (
            f"[任务旗标] {node.get('flag', '')} "
            f"{models.enum_label('game_flag_op', node.get('op', 'set'))} "
            f"{node.get('value', 0)}"
        )
    if t == "enemy":
        op = node.get("op", "team")
        faction = models.display_name(ed, "battle_factions", node.get("enemy", ""))
        value = "" if op == "id" else f" {node.get('value', 0)}"
        return (
            f"[{models.NODE_TYPE_CN.get('enemy', 'enemy')}] "
            f"{models.enum_label('enemy_op', op)} {faction}{value}"
        )
    if t == "battle_skill":
        return (
            f"[战场技能] {models.enum_label('battle_skill_op', node.get('op', 'set'))}"
            f" {node.get('key', '')}"
        )
    if t == "battle_setup":
        return (
            f"[战前配置] 敌方 {node.get('enemy', '') or '不变'}｜"
            f"我方技能 {len(node.get('skills', []))} 项"
        )
    if t == "combat":
        return (
            f"[战斗] 原版 Combat {node.get('key', '')}｜"
            f"胜利→{node.get('win', '')}｜失败→{node.get('lose', '')}"
        )
    if t == "battle":
        return (
            f"[战役] 原版 Battle {node.get('key', '')}｜"
            f"友军胜→{node.get('win', '')}｜敌军胜→{node.get('lose', '')}"
        )
    if t == "battle_result":
        return (
            f"[战斗结果] {node.get('kind', 'any')}｜"
            f"胜利→{node.get('win', '')}｜失败→{node.get('lose', '')}"
        )
    if t == "reward":
        return f"[战斗奖励] {len(node.get('entries', []))} 项"
    if t == "result_screen":
        return (
            f"[自定义结算] {node.get('title', '')}｜"
            f"发放 {len(node.get('entries', []))} 项奖励"
        )
    if t.endswith("_check") or t == "activity":
        return (
            f"[{models.NODE_TYPE_CN.get(t, t)}] "
            f"成功→{node.get('success', '')}｜失败→{node.get('failure', '')}"
        )
    if t == "mission":
        return f"[任务] {node.get('name', '')} {node.get('key', '')}"
    if t == "time":
        op = node.get("op", "round")
        if op in ("set", "mission"):
            return (
                f"[时间] {models.enum_label('time_op', op)}："
                f"{node.get('year', 1)}年{node.get('month', 1)}月"
                f"时段{node.get('stage', 1)}"
            )
        return f"[时间] {models.enum_label('time_op', op)}"
    if t == "autosave":
        return f"[自动存档] {models.enum_label('autosave_kind', node.get('kind', 'story'))}"
    if t == "dice":
        return (
            f"[骰子检定] {node.get('header', '')} 0~{node.get('max', 99)}"
            f"（{len(node.get('bands', []))} 个结果分段）"
        )
    if t == "goto_scene":
        key = node.get("key") or ""
        return (
            f"[场景跳转] {models.enum_label('goto_scene', node.get('scene', 'Free'))}"
            + (f" {key}" if key else "")
        )
    if t == "panel":
        return f"[系统面板] {models.enum_label('panel', node.get('panel', ''))}"
    if t == "wait":
        return f"[等待] {node.get('seconds', 0)} 秒"
    if t == "end":
        nxt = node.get("next_script")
        return f"[结束] 链到脚本 {nxt}" if nxt else "[结束] 返回自由模式"
    if t == "raw":
        return "[原生Lua] 原样插入编译产物（详见 Lua 页签）"
    return f"[{models.NODE_TYPE_CN.get(t, t)}]"


def _apply_node(state: dict, node: dict, ed: dict | None = None) -> None:
    """把单个节点的舞台效果作用到 state 上。"""
    t = node.get("type")
    actors = state["actors"]
    # 只有当前（目标）节点的对白/选项/提示需要展示，每步先清空
    state["dialog"] = None
    state["choice"] = None
    state["hint"] = None
    for actor in actors.values():
        actor["shocked"] = False

    if t == "scene":
        state["view"] = node.get("view") or None
        state["background"] = None
    elif t == "background":
        if node.get("action", "show") in ("fadeout", "clear"):
            state["background"] = None
        else:
            state["background"] = node.get("image") or None
    elif t == "custom_cg":
        if node.get("action", "show") == "hide":
            state["custom_cg"] = None
        else:
            state["custom_cg"] = {
                "image": node.get("image") or None,
                "scale": node.get("scale", 100),
                "x": node.get("x", 0),
                "y": node.get("y", 0),
            }
    elif t == "overlay":
        slot = str(node.get("slot") or "main")
        if node.get("action", "show") == "hide":
            state["overlays"].pop(slot, None)
        else:
            state["overlays"][slot] = {
                "image": node.get("image") or None,
                "position": node.get("position", "center"),
                "scale": node.get("scale", 100),
                "opacity": node.get("opacity", 100),
                "layer": node.get("layer", "front"),
            }
    elif t == "show":
        cid = node.get("character") or ""
        if cid:
            actors[cid] = {
                "position": node.get("position") or "M",
                "portrait": node.get("portrait") or "normal",
                "facing": node.get("facing") or "right",
                "offset_x": 0,
                "offset_y": 0,
                "rotation": 0,
                "dimmed": False,
                "shocked": False,
            }
    elif t == "move":
        cid = node.get("character") or ""
        if cid in actors and node.get("to"):
            actors[cid]["position"] = node["to"]
    elif t == "face":
        cid = node.get("character") or ""
        if cid in actors and node.get("facing"):
            actors[cid]["facing"] = node["facing"]
    elif t == "offset":
        cid = node.get("character") or ""
        if cid in actors:
            actors[cid]["offset_x"] = actors[cid].get("offset_x", 0) + node.get("x", 0)
            actors[cid]["offset_y"] = actors[cid].get("offset_y", 0) + node.get("y", 0)
    elif t == "shock":
        cid = node.get("character") or ""
        if cid in actors:
            actors[cid]["shocked"] = True
    elif t == "dim":
        cid = node.get("character") or ""
        if cid in actors:
            actors[cid]["dimmed"] = bool(node.get("dimmed"))
    elif t == "rotate":
        cid = node.get("character") or ""
        if cid in actors:
            actors[cid]["rotation"] = node.get("angle", 0)
    elif t == "hide":
        actors.pop(node.get("character") or "", None)
    elif t == "say":
        cid = node.get("character") or ""
        # 官方行为：say 前 setcharacter 换表情（仅当该人物在台上）
        if cid in actors:
            actors[cid]["portrait"] = node.get("portrait") or "normal"
        state["dialog"] = {
            "character": cid,
            "mode": node.get("mode") or "character",
            "text": node.get("text") or "",
        }
    elif t == "choice":
        state["choice"] = [
            {"text": o.get("text", ""), "goto": o.get("goto", "")}
            for o in node.get("options", [])
        ]
    # 无法舞台化的类型：给一行中文提示
    state["hint"] = _hint_text(node, ed or models.FALLBACK_EDITOR_DATA)


def _next_node(node: dict, idx: int, nodes: list) -> str | None:
    """决定下一步节点 id：choice/branch/dice 走分支，显式 goto 优先，否则顺序。"""
    t = node.get("type")
    if t in (
        "end", "goto_scene", "death", "combat", "battle", "battle_result",
        "stat_check", "affinity_check", "item_check", "talent_check", "flag_check", "activity",
        "quest_check", "persistent_check",
    ):
        return None  # 脚本终止/跳离当前场景：推演到此为止
    if t == "choice":
        for opt in node.get("options", []):
            if opt.get("goto"):
                return opt["goto"]
    elif t == "branch":
        cases = [c for c in node.get("cases", []) if c.get("goto")]
        if cases:
            # 优先 value=1 的分支（与自动播放约定一致），否则第一个
            for c in cases:
                if c.get("value") == 1:
                    return c["goto"]
            return cases[0]["goto"]
    elif t == "dice":
        bands = node.get("bands", [])
        if bands and bands[0].get("goto"):
            return bands[0]["goto"]
    elif node.get("goto"):
        return node["goto"]
    if idx + 1 < len(nodes):
        return nodes[idx + 1].get("id")
    return None


def _path_to_target(story: dict, target_id: str) -> list[str] | None:
    """从 start 找任意一条通向 target 的节点 id 链（含两端）。找不到返回 None。"""
    try:
        graph = story_graph.analyze_story(story)
    except Exception:
        return None
    start = str(story.get("start") or "")
    if not start or not target_id:
        return None
    adj: dict[str, list[str]] = {nid: [] for nid in graph.node_order}
    for edge in graph.edges:
        if not edge.missing:
            adj.setdefault(edge.source, []).append(edge.target)
    if start == target_id:
        return [start]
    prev: dict[str, str | None] = {start: None}
    queue = [start]
    while queue:
        cur = queue.pop(0)
        for nxt in adj.get(cur, ()):
            if nxt in prev:
                continue
            prev[nxt] = cur
            if nxt == target_id:
                path = [target_id]
                while path[-1] != start:
                    parent = prev[path[-1]]
                    if parent is None:
                        return None
                    path.append(parent)
                path.reverse()
                return path
            queue.append(nxt)
    return None


def simulate_stage(
    story: dict,
    target_id: str | None,
    editor_data: dict | None = None,
    *,
    include_target: bool = True,
) -> dict:
    """从 start 推演到 target_id（含目标节点自身效果），返回舞台状态。

    默认路线走不到时，改走「任意一条通向此步的分支」，保证选中失败/选项
    支线时仍能看到当时的场景、立绘和对白。实在连不上，也至少应用本步本身。

    include_target=False 时返回目标节点生效【之前】的状态（F5 试玩前导用）。
    """
    state = {
        "view": None,
        "background": None,
        "custom_cg": None,
        "overlays": {},
        "actors": {},
        "dialog": None,
        "choice": None,
        "hint": None,
        "reached": False,
        "via_branch": False,
        "steps": 0,
    }
    nodes = story.get("nodes", []) if story else []
    if not nodes or not target_id:
        return state
    by_id = {n.get("id"): i for i, n in enumerate(nodes)}
    if target_id not in by_id:
        return state

    path = _path_to_target(story, target_id)
    start = story.get("start") or nodes[0].get("id")

    if path:
        walk = path if include_target else path[:-1]
        for step, nid in enumerate(walk, 1):
            idx = by_id.get(nid)
            if idx is None:
                break
            _apply_node(state, nodes[idx], editor_data)
            state["steps"] = step
        state["reached"] = True
        # 默认路线（遇选项走第一项）到不了，才标「沿通向此步的分支」
        default_ids: list[str] = []
        cur = start
        for _ in range(MAX_STEPS):
            default_ids.append(cur)
            if cur == target_id:
                break
            idx = by_id.get(cur)
            if idx is None:
                break
            nxt = _next_node(nodes[idx], idx, nodes)
            if nxt is None or nxt not in by_id:
                break
            cur = nxt
        state["via_branch"] = target_id not in default_ids
        if not include_target:
            state["dialog"] = None
            state["choice"] = None
            state["hint"] = None
        return state

    # 图上连不到：仍把本步画出来，舞台用开头默认路线推到尽头再叠本步
    cur = start
    for step in range(MAX_STEPS):
        idx = by_id.get(cur)
        if idx is None:
            break
        node = nodes[idx]
        state["steps"] = step + 1
        _apply_node(state, node, editor_data)
        nxt = _next_node(node, idx, nodes)
        if nxt is None or nxt not in by_id:
            break
        cur = nxt
    if include_target:
        _apply_node(state, nodes[by_id[target_id]], editor_data)
    state["reached"] = False
    return state


# ---------------------------------------------------------------------------
# F5 试玩舞台前导
# ---------------------------------------------------------------------------
PLAYTEST_PRELUDE_PREFIX = "zz_playtest_"  # 前导节点 id 前缀（排序沉底、一眼可辨）


def build_playtest_prelude(
    story: dict, target_id: str, editor_data: dict | None = None
) -> list[dict]:
    """为 F5 试玩生成舞台状态前导节点链，末尾 goto 到 target_id。

    从 story 的 start 推演到目标节点（不含其自身效果），把当时的场景与台上
    人物（站位/表情/朝向）补成 scene/show 节点；这样从依赖前置舞台状态的
    步骤（如 rotate/hide 一个早前才 show 的人物）进入试玩时，游戏不再因
    “角色不存在”崩掉剧情协程而黑屏。

    返回合成节点列表（调用方把它 extend 进 nodes 并把 start 指向链头）；
    空列表表示没有可重建的舞台状态（保持 start=目标节点即可）。
    只还原舞台视觉状态；音乐、好感、旗标等运行态不在其列。
    """
    state = simulate_stage(story, target_id, editor_data, include_target=False)
    view = state.get("view")
    background = state.get("background")
    custom_cg = state.get("custom_cg")
    overlays = state.get("overlays") or {}
    actors = state.get("actors") or {}

    stage_nodes: list[dict] = []
    # view="out" 只是淡出，无可重建的画面，跳过（否则前导反而把屏幕淡出）
    if view and view != "out":
        stage_nodes.append({"type": "scene", "view": view})
    if background:
        stage_nodes.append(
            {"type": "background", "action": "set", "image": background}
        )
    for slot, overlay in sorted(overlays.items()):
        if overlay.get("image"):
            stage_nodes.append(
                {
                    "type": "overlay",
                    "action": "show",
                    "slot": slot,
                    "image": overlay["image"],
                    "position": overlay.get("position", "center"),
                    "scale": overlay.get("scale", 100),
                    "opacity": overlay.get("opacity", 100),
                    "layer": overlay.get("layer", "front"),
                    "fade": 0,
                }
            )
    # 按站位 x 从左到右依次上场，id 兜底保证确定性
    ordered = sorted(
        actors.items(),
        key=lambda kv: (position_x(kv[1].get("position", ""))[0], kv[0]),
    )
    for cid, info in ordered:
        stage_nodes.append(
            {
                "type": "show",
                "character": cid,
                "position": info.get("position") or "M",
                "portrait": info.get("portrait") or "normal",
                "facing": info.get("facing") or "right",
            }
        )
    if custom_cg and custom_cg.get("image"):
        stage_nodes.append(
            {
                "type": "custom_cg",
                "action": "show",
                "image": custom_cg["image"],
                "fade": 0,
                "scale": custom_cg.get("scale", 100),
                "x": custom_cg.get("x", 0),
                "y": custom_cg.get("y", 0),
            }
        )
    if not stage_nodes:
        return []

    existing = {n.get("id") for n in (story or {}).get("nodes", [])}
    prelude: list[dict] = []
    for seq, stage_node in enumerate(stage_nodes):
        node_id = "%s%d" % (PLAYTEST_PRELUDE_PREFIX, seq)
        while node_id in existing:  # 用户节点撞名时顺延（理论上不会）
            seq += 1
            node_id = "%s%d" % (PLAYTEST_PRELUDE_PREFIX, seq)
        existing.add(node_id)
        stage_node["id"] = node_id
        prelude.append(stage_node)
    for i, stage_node in enumerate(prelude):
        nxt = prelude[i + 1]["id"] if i + 1 < len(prelude) else target_id
        stage_node["goto"] = nxt
    return prelude


# ---------------------------------------------------------------------------
# 舞台预览控件
# ---------------------------------------------------------------------------
class StagePreview(QWidget):
    """16:9 舞台自绘控件：背景 + 人物立绘 + 对白栏 + 可点击选项按钮。"""

    # 点击选项按钮时发出（goto 节点 id）
    choice_activated = Signal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMinimumSize(320, 180)
        self._editor_data: dict = models.FALLBACK_EDITOR_DATA
        self._pmap: dict = {}
        self._data_dir: Path = PROJECT_ROOT / "data"
        self._story_root: Path | None = None
        self._story: dict | None = None
        self._node_id: str | None = None
        self._state: dict = simulate_stage({}, None)
        self._pix_cache: OrderedDict[str, QPixmap] = OrderedDict()
        self._cache_bytes = 0  # 缓存 pixmap 的解码字节数（用于按字节驱逐）
        self._choice_rects: list[tuple[QRect, str]] = []

    # ------------------------------------------------------------ 数据注入
    def set_context(self, editor_data: dict) -> None:
        """注入 editor_data（查人物显示名用）。"""
        self._editor_data = editor_data or models.FALLBACK_EDITOR_DATA
        self.update()

    def set_assets(self, preview_map: dict, data_dir: Path) -> None:
        """注入 preview_map 素材映射与 data 目录。"""
        self._pmap = preview_map or {}
        self._data_dir = data_dir
        self._pix_cache.clear()
        self._cache_bytes = 0
        self.update()

    def set_story_root(self, root: Path | None) -> None:
        """设置 story/ 所属 mod 根目录，供 End 节点预览 assets/ 自定义插图。"""
        self._story_root = Path(root) if root is not None else None

    def has_assets(self) -> bool:
        """是否有任何预览素材（否则全走占位图）。"""
        return bool(self._pmap.get("characters") or self._pmap.get("views"))

    def show_node(self, story: dict, node_id: str | None) -> None:
        """设置要预览的 story 与节点 id，重新推演并重绘。"""
        self._story = story
        self._node_id = node_id
        try:
            self._state = simulate_stage(story, node_id, self._editor_data)
        except Exception:
            # 畸形 story（节点不是 dict 等）：记日志，退回空舞台而不是崩掉
            log_crash(f"舞台推演异常（节点 {node_id!r}）：\n" + traceback.format_exc())
            self._state = simulate_stage({}, None)
        self.update()

    # ------------------------------------------------------------ 图片加载
    @staticmethod
    def _pix_bytes(pix: QPixmap) -> int:
        return 0 if pix.isNull() else pix.width() * pix.height() * 4

    def _evict_cache(self) -> None:
        """LRU 驱逐：超出条数/字节上限时逐出最久未用的项（至少保留 1 项）。"""
        while len(self._pix_cache) > 1 and (
            len(self._pix_cache) > MAX_CACHE_ENTRIES
            or self._cache_bytes > MAX_CACHE_BYTES
        ):
            _key, pix = self._pix_cache.popitem(last=False)
            self._cache_bytes -= self._pix_bytes(pix)

    def _load_pixmap(self, rel_path: str | Path | None) -> QPixmap:
        """按相对 data/ 的路径加载图片（LRU 缓存 + 加载即降采样）。

        任何失败（文件缺失/损坏/解码异常）都返回 null QPixmap，调用方走占位图。
        """
        if not rel_path:
            return QPixmap()
        # content_registry.resolve() 返回 pathlib.Path；预览映射则通常返回 str。
        # 必须先统一成文本，否则 Path.replace 是“移动文件”API，并不是字符串替换。
        key = str(rel_path).replace("\\", "/")
        hit = self._pix_cache.get(key)
        if hit is not None:
            self._pix_cache.move_to_end(key)
            return hit
        try:
            source = Path(key)
            if not source.is_absolute():
                source = self._data_dir / source
            pix = QPixmap(str(source))
            # 加载即降采样：原图最大约 3000px/20MB 解码，控件绘制最多几百 px，
            # 降到 MAX_IMAGE_DIM 内可把单张缓存压到约 1/4，且后续每次重绘的
            # 缩放开销也大幅降低
            if not pix.isNull() and max(pix.width(), pix.height()) > MAX_IMAGE_DIM:
                pix = pix.scaled(
                    MAX_IMAGE_DIM,
                    MAX_IMAGE_DIM,
                    Qt.AspectRatioMode.KeepAspectRatio,
                    Qt.TransformationMode.SmoothTransformation,
                )
        except Exception:
            log_crash(f"图片加载异常（{key}）：\n{traceback.format_exc()}")
            pix = QPixmap()
        self._pix_cache[key] = pix  # 失败也缓存（isNull），避免重复读盘
        self._cache_bytes += self._pix_bytes(pix)
        self._evict_cache()
        return pix

    def _load_story_image(self, image_path: str) -> QPixmap:
        """先找当前项目 assets/，再找编辑器托管的用户图片仓库。"""
        if image_path and self._story_root is not None:
            source = (self._story_root / image_path).resolve()
            try:
                source.relative_to(self._story_root.resolve())
                if source.is_file():
                    return self._load_pixmap(str(source))
            except (ValueError, OSError):
                pass
        managed = resolve_image_asset(image_path)
        return self._load_pixmap(str(managed)) if managed is not None else QPixmap()

    def _character_title(self, char_id: str) -> str:
        if not isinstance(char_id, str) or not char_id.startswith("user:"):
            return ""
        try:
            rec, _main = content_registry.resolve(char_id, expected_type="character")
        except Exception:
            return ""
        if rec.title:
            return rec.title
        if rec.intro and rec.intro.get("title"):
            return str(rec.intro["title"])
        return ""

    def _bound_intro_texts(self, char_id: str) -> tuple[str, str, str]:
        try:
            rec, _main = content_registry.resolve(char_id, expected_type="character")
        except Exception:
            return "", char_id or "自定义人物", "还没有介绍卡。"
        intro = rec.intro or {}
        return (
            str(intro.get("title") or rec.title or ""),
            str(intro.get("name") or rec.name or char_id),
            str(intro.get("text") or "还没有介绍卡。请先在角色「介绍卡」页填写。"),
        )

    def _load_bound_intro_image(self, char_id: str) -> QPixmap:
        try:
            rec, _main = content_registry.resolve(char_id, expected_type="character")
        except Exception:
            return QPixmap()
        fname = (rec.intro or {}).get("image")
        if not fname:
            return QPixmap()
        return self._load_pixmap(str(rec.folder / fname))

    def _character_look(self, char_id: str) -> tuple[int, str]:
        """自定义角色的体型百分比与原图朝向。官方角色按原版立绘朝左、100%。"""
        if isinstance(char_id, str) and char_id.startswith("user:"):
            try:
                rec, _main = content_registry.resolve(
                    char_id, expected_type="character"
                )
            except Exception:
                return 100, "left"
            scale = int(rec.scale or 100)
            if scale < 50:
                scale = 50
            if scale > 130:
                scale = 130
            facing = rec.art_facing if rec.art_facing in ("left", "right") else "left"
            return scale, facing
        return 100, "left"

    def _portrait_path(self, char_id: str, portrait: str) -> str | None:
        if isinstance(char_id, str) and char_id.startswith("user:"):
            try:
                rec, _main = content_registry.resolve(
                    char_id, expected_type="character"
                )
            except Exception:
                rec = None
            if rec is not None:
                portraits = rec.portraits or {}
                fname = portraits.get(portrait) or portraits.get("normal") or rec.main_file
                if fname:
                    path = rec.folder / fname
                    if path.is_file():
                        return str(path)
        c = self._pmap.get("characters", {}).get(char_id, {})
        return (c.get("portraits") or {}).get(portrait)

    def _view_path(self, view: str | None) -> str | None:
        if not view:
            return None
        return self._pmap.get("views", {}).get(view)

    # ------------------------------------------------------------ 绘制
    def _stage_rect(self) -> QRect:
        """控件内居中的 16:9 舞台区。"""
        w, h = self.width(), self.height()
        tw, th = w, floor(w * 9 / 16)
        if th > h:
            th, tw = h, floor(h * 16 / 9)
        return QRect((w - tw) // 2, (h - th) // 2, max(0, tw), max(0, th))

    def paintEvent(self, _event) -> None:  # noqa: N802（Qt 命名）
        # 防御：绘制中的任何异常都不能让进程崩掉（pythonw 下无声），
        # 记日志并退化为兜底画面。QPainter 全程只此一个，finally 里收尾。
        p = QPainter(self)
        try:
            p.setRenderHint(QPainter.RenderHint.Antialiasing)
            p.fillRect(self.rect(), QColor(18, 18, 18))
            rect = self._stage_rect()
            self._choice_rects.clear()
            if rect.width() <= 0 or rect.height() <= 0:
                return
            if self._paint_ending_card(p, rect):
                self._paint_caption(p, rect)
                return
            if self._paint_intro_card(p, rect):
                self._paint_caption(p, rect)
                return
            self._paint_background(p, rect)
            self._paint_overlays(p, rect, "back")
            self._paint_actors(p, rect)
            self._paint_custom_cg(p, rect)
            self._paint_overlays(p, rect, "front")
            self._paint_dialog(p, rect)
            self._paint_hint(p, rect)
            self._paint_choices(p, rect)
            self._paint_caption(p, rect)
        except Exception:
            log_crash("演出预览 paintEvent 异常：\n" + traceback.format_exc())
            try:
                p.resetTransform()
                p.fillRect(self.rect(), QColor(24, 24, 24))
                p.setPen(QColor(255, 160, 120))
                p.drawText(
                    self.rect(),
                    Qt.AlignmentFlag.AlignCenter,
                    "演出预览绘制异常（详见 crash.log）",
                )
            except Exception:
                pass
        finally:
            p.end()

    def _current_node(self) -> dict:
        return next(
            (
                n
                for n in (self._story or {}).get("nodes", [])
                if n.get("id") == self._node_id
            ),
            {},
        )

    def _paint_ending_card(self, p: QPainter, rect: QRect) -> bool:
        """预览官方 EndGamePanel 的汗青书构图，而不是只显示一行场景跳转提示。"""
        node = self._current_node()
        if node.get("type") != "goto_scene" or node.get("scene") != "End":
            return False

        p.fillRect(rect, QColor(9, 12, 12))
        band = QRect(
            rect.x(),
            rect.y() + floor(rect.height() * 0.22),
            rect.width(),
            floor(rect.height() * 0.57),
        )
        p.fillRect(band, QColor(229, 213, 177))
        p.setPen(QPen(QColor(102, 72, 44), max(1, floor(rect.height() * 0.004))))
        p.drawLine(band.topLeft(), band.topRight())
        p.drawLine(band.bottomLeft(), band.bottomRight())

        # 左侧书封与书脊；自定义 image 对应官方 _picImage 槽。没有文件时画清楚的
        # 开发占位，游戏内则由运行时直接借用原版 20047.Picture。
        book = QRect(
            rect.x() + floor(rect.width() * 0.15),
            rect.y() + floor(rect.height() * 0.10),
            floor(rect.width() * 0.38),
            floor(rect.height() * 0.78),
        )
        p.fillRect(book, QColor(200, 166, 107))
        cover = book.adjusted(
            floor(book.width() * 0.06),
            floor(book.height() * 0.04),
            -floor(book.width() * 0.18),
            -floor(book.height() * 0.04),
        )
        p.fillRect(cover, QColor(224, 204, 163))
        p.setPen(QPen(QColor(111, 77, 42), max(1, floor(rect.height() * 0.004))))
        p.drawRect(book)
        p.drawRect(cover)

        image_path = str(node.get("image") or "").strip()
        pix = self._load_story_image(image_path) if image_path else QPixmap()
        art = cover.adjusted(
            floor(cover.width() * 0.09),
            floor(cover.height() * 0.12),
            -floor(cover.width() * 0.09),
            -floor(cover.height() * 0.12),
        )
        if not pix.isNull():
            scaled = pix.scaled(
                art.size(),
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )
            p.drawPixmap(
                art.x() + (art.width() - scaled.width()) // 2,
                art.y() + (art.height() - scaled.height()) // 2,
                scaled,
            )
        else:
            p.fillRect(art, QColor(238, 225, 196))
            p.setPen(QColor(80, 64, 45))
            placeholder_font = QFont(self.font())
            placeholder_font.setPointSizeF(max(8.0, rect.height() * 0.026))
            p.setFont(placeholder_font)
            p.drawText(
                QRectF(art),
                Qt.AlignmentFlag.AlignCenter | Qt.TextFlag.TextWordWrap,
                "原版结局插图占位\n（游戏内借用 20047）",
            )

        text_area = QRect(
            rect.x() + floor(rect.width() * 0.56),
            band.y() + floor(band.height() * 0.12),
            floor(rect.width() * 0.37),
            floor(band.height() * 0.72),
        )
        title_font = QFont(self.font())
        title_font.setPointSizeF(max(14.0, rect.height() * 0.075))
        title_font.setBold(True)
        p.setFont(title_font)
        p.setPen(QColor(151, 48, 38))
        title_h = floor(text_area.height() * 0.34)
        p.drawText(
            QRectF(text_area.x(), text_area.y(), text_area.width(), title_h),
            Qt.AlignmentFlag.AlignHCenter | Qt.AlignmentFlag.AlignVCenter,
            str(node.get("title") or "自定义结局标题"),
        )
        body_font = QFont(self.font())
        body_font.setPointSizeF(max(9.0, rect.height() * 0.032))
        body_font.setBold(True)
        p.setFont(body_font)
        p.setPen(QColor(37, 31, 24))
        p.drawText(
            QRectF(
                text_area.x(),
                text_area.y() + title_h,
                text_area.width(),
                text_area.height() - title_h,
            ),
            Qt.AlignmentFlag.AlignHCenter
            | Qt.AlignmentFlag.AlignTop
            | Qt.TextFlag.TextWordWrap,
            str(node.get("desc") or "在这里填写自定义结局正文。"),
        )
        return True

    def _paint_intro_card(self, p: QPainter, rect: QRect) -> bool:
        """按游戏 CharacterIntroPanel 的全屏构图预览人物介绍卡。

        预览不打包游戏原始 UI 素材，但尺寸、层级和安全区域与运行时保持一致：
        背景压暗、左侧人物、中央水墨装饰、右侧文字、右上关闭、底部继续。
        """
        node = self._current_node()
        if node.get("type") != "intro":
            return False
        custom = node.get("intro_source", "official") == "custom"

        # 原版面板覆盖在当前场景上，不是独立的米色卡片。
        self._paint_background(p, rect)
        p.fillRect(rect, QColor(4, 6, 9, 112))

        # 下半部半透明宣纸框，对应官方 _frameImage / _brushImage 所在区域。
        panel = QRect(
            rect.x() + floor(rect.width() * 0.35),
            rect.y() + floor(rect.height() * 0.50),
            floor(rect.width() * 0.49),
            floor(rect.height() * 0.29),
        )
        p.fillRect(panel, QColor(220, 207, 178, 86))
        edge = max(1, floor(rect.height() * 0.003))
        p.setPen(QPen(QColor(221, 209, 183, 148), edge))
        p.drawRect(panel)
        inner = panel.adjusted(
            floor(panel.width() * 0.015),
            floor(panel.height() * 0.05),
            -floor(panel.width() * 0.015),
            -floor(panel.height() * 0.05),
        )
        p.setPen(QPen(QColor(184, 170, 143, 116), edge))
        p.drawRect(inner)

        # 左侧水墨圆与横向笔刷。这里只复刻轮廓，不携带游戏原始纹理。
        ink_center_x = rect.x() + floor(rect.width() * 0.36)
        ink_center_y = rect.y() + floor(rect.height() * 0.49)
        ink_w = floor(rect.width() * 0.25)
        ink_h = floor(rect.height() * 0.50)
        p.setPen(QPen(QColor(28, 23, 20, 112), max(2, floor(rect.height() * 0.010))))
        p.drawEllipse(
            QRect(
                ink_center_x - ink_w // 2,
                ink_center_y - ink_h // 2,
                ink_w,
                ink_h,
            )
        )
        brush = QRect(
            rect.x() + floor(rect.width() * 0.49),
            rect.y() + floor(rect.height() * 0.46),
            floor(rect.width() * 0.18),
            floor(rect.height() * 0.075),
        )
        p.fillRect(brush, QColor(20, 19, 18, 190))
        for offset, alpha in ((-5, 90), (5, 115), (10, 62)):
            p.setPen(QPen(QColor(29, 25, 22, alpha), max(1, edge)))
            p.drawLine(
                brush.x() - floor(brush.width() * 0.06),
                brush.center().y() + offset,
                brush.right() + floor(brush.width() * 0.08),
                brush.center().y() + offset,
            )

        def number(key: str, default: float, low: float, high: float) -> float:
            try:
                value = float(node.get(key, default))
            except (TypeError, ValueError):
                value = default
            return max(low, min(high, value))

        image_scale = number("image_scale", 100, 40, 160) if custom else 100
        image_x = number("image_x", 0, -30, 30) if custom else 0
        image_y = number("image_y", 0, -30, 30) if custom else 0
        box_width = floor(rect.width() * 0.30 * image_scale / 100.0)
        box_height = floor(rect.height() * 0.62 * image_scale / 100.0)
        center_x = rect.x() + floor(rect.width() * (0.31 + image_x / 100.0))
        # Qt 向下为正；游戏字段的正数约定为向上。
        center_y = rect.y() + floor(rect.height() * (0.50 - image_y / 100.0))
        avatar = QRect(
            center_x - box_width // 2,
            center_y - box_height // 2,
            box_width,
            box_height,
        )
        image_path = str(node.get("image") or "").strip() if custom else ""
        if custom:
            avatar_pix = self._load_story_image(image_path) if image_path else QPixmap()
        elif node.get("intro_source") == "character":
            avatar_pix = self._load_bound_intro_image(str(node.get("character") or ""))
        else:
            # 官方介绍卡预览不使用游戏原图，避免编辑器展示/误打官方头像。
            avatar_pix = QPixmap()
        if not avatar_pix.isNull():
            scaled = avatar_pix.scaled(
                avatar.size(),
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )
            x = avatar.x() + (avatar.width() - scaled.width()) // 2
            y = avatar.y() + (avatar.height() - scaled.height()) // 2
            p.drawPixmap(x, y, scaled)
        else:
            p.fillRect(avatar, QColor(24, 27, 30, 112))
            p.setPen(QColor(220, 210, 187))
            avatar_font = QFont(self.font())
            avatar_font.setPointSizeF(max(10.0, rect.height() * 0.027))
            p.setFont(avatar_font)
            if custom:
                avatar_text = "自定义人物\n未选择人物图片"
            elif node.get("intro_source") == "character":
                avatar_text = "自定义角色\n未设置介绍图"
            else:
                avatar_text = "游戏内显示\n原版关系头像"
            p.drawText(
                QRectF(avatar),
                Qt.AlignmentFlag.AlignCenter | Qt.TextFlag.TextWordWrap,
                avatar_text,
            )

        text_rect = QRect(
            rect.x() + floor(rect.width() * 0.52),
            rect.y() + floor(rect.height() * 0.42),
            floor(rect.width() * 0.29),
            floor(rect.height() * 0.31),
        )
        if custom:
            title = str(node.get("title") or "")
            name = str(node.get("name") or "自定义人物姓名")
            intro = str(node.get("text") or "在这里填写人物介绍。")
        elif node.get("intro_source") == "character":
            title, name, intro = self._bound_intro_texts(str(node.get("character") or ""))
        else:
            title, name, intro = models.official_character_intro(
                self._editor_data, str(node.get("character") or "")
            )
        # 官方层级：金色小称号、红色大姓名、浅色正文。
        p.setPen(QColor(213, 184, 122))
        title_font = QFont(self.font())
        title_font.setPointSizeF(max(10.0, rect.height() * 0.033))
        title_font.setBold(True)
        p.setFont(title_font)
        title_rect = QRect(
            text_rect.x(), text_rect.y(), text_rect.width(), floor(text_rect.height() * 0.22)
        )
        p.drawText(title_rect, Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter, title)
        name_font = QFont(self.font())
        name_font.setPointSizeF(max(16.0, rect.height() * 0.070))
        name_font.setBold(True)
        p.setFont(name_font)
        p.setPen(QColor(235, 78, 56))
        name_rect = QRect(
            text_rect.x(),
            text_rect.y() + floor(text_rect.height() * 0.18),
            text_rect.width(),
            floor(text_rect.height() * 0.31),
        )
        p.drawText(name_rect, Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter, name)
        body_font = QFont(self.font())
        body_font.setPointSizeF(max(10.0, rect.height() * 0.030))
        p.setFont(body_font)
        p.setPen(QColor(235, 229, 216))
        body_rect = QRect(
            text_rect.x(),
            text_rect.y() + floor(text_rect.height() * 0.49),
            text_rect.width(),
            floor(text_rect.height() * 0.49),
        )
        p.drawText(
            QRectF(body_rect),
            Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignLeft | Qt.TextFlag.TextWordWrap,
            intro,
        )

        # 右上关闭按钮：官方菱形轮廓；预览不做点击，仅表达实际构图。
        close_size = max(18, floor(rect.height() * 0.075))
        close_center_x = rect.x() + floor(rect.width() * 0.84)
        close_center_y = rect.y() + floor(rect.height() * 0.42)
        p.save()
        p.translate(close_center_x, close_center_y)
        p.rotate(45)
        p.fillRect(
            QRect(-close_size // 2, -close_size // 2, close_size, close_size),
            QColor(43, 39, 43, 218),
        )
        p.setPen(QPen(QColor(224, 202, 153), max(1, edge)))
        p.drawRect(QRect(-close_size // 2, -close_size // 2, close_size, close_size))
        p.rotate(-45)
        p.setPen(QPen(QColor(241, 235, 224), max(1, edge)))
        cross = floor(close_size * 0.20)
        p.drawLine(-cross, -cross, cross, cross)
        p.drawLine(-cross, cross, cross, -cross)
        p.restore()

        continue_font = QFont(self.font())
        continue_font.setPointSizeF(max(9.0, rect.height() * 0.025))
        continue_font.setBold(True)
        p.setFont(continue_font)
        p.setPen(QColor(214, 185, 124))
        continue_rect = QRect(
            rect.x() + floor(rect.width() * 0.69),
            rect.y() + floor(rect.height() * 0.78),
            floor(rect.width() * 0.15),
            floor(rect.height() * 0.07),
        )
        p.drawText(
            continue_rect,
            Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter,
            "点击继续……  ▼",
        )
        return True

    def _paint_background(self, p: QPainter, rect: QRect) -> None:
        view = self._state.get("view")
        background = self._state.get("background")
        missing_user = False
        if background:
            try:
                _record, source = content_registry.resolve(
                    str(background), expected_type="image"
                )
                pix = self._load_pixmap(source)
            except content_registry.ContentRegistryError:
                pix = QPixmap()
                missing_user = True
        else:
            pix = self._load_pixmap(self._view_path(view))
        if not pix.isNull():
            # 缩放铺满、保比例、居中（超出部分裁掉）
            scaled = pix.scaled(
                rect.size(),
                Qt.AspectRatioMode.KeepAspectRatioByExpanding,
                Qt.TransformationMode.SmoothTransformation,
            )
            x = rect.x() + (rect.width() - scaled.width()) // 2
            y = rect.y() + (rect.height() - scaled.height()) // 2
            p.save()
            try:
                p.setClipRect(rect)
                p.drawPixmap(x, y, scaled)
            finally:
                p.restore()  # 裁剪配平：异常也不把 clip 泄漏给后续绘制
        else:
            # 契约 §3.1：view="black"/"white" 为纯色场景
            if view == "black":
                p.fillRect(rect, QColor(0, 0, 0))
            elif view == "white":
                p.fillRect(rect, QColor(245, 245, 245))
            else:
                p.fillRect(rect, BG_FALLBACK)
            p.setPen(
                QColor(200, 200, 200) if view != "white" else QColor(120, 120, 120)
            )
            font = QFont(self.font())
            font.setPointSizeF(max(10.0, rect.height() * 0.06))
            font.setBold(True)
            p.setFont(font)
            if missing_user:
                label = f"缺失用户图片：{background}"
            else:
                label = f"场景：{view}" if view else "（尚未切场景）"
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter, label)

    def _paint_actors(self, p: QPainter, rect: QRect) -> None:
        actors = self._state.get("actors", {})
        if not actors:
            return
        baseline = rect.bottom() - floor(rect.height() * 0.02)  # 底部基准线
        h = floor(rect.height() * 0.78)  # 立绘高度
        # 按 x 排序，左边先画，避免右侧人物被左压
        ordered = sorted(
            actors.items(), key=lambda kv: position_x(kv[1].get("position", ""))[0]
        )
        unknown: list[str] = []
        for cid, info in ordered:
            pos = info.get("position", "")
            frac, known = position_x(pos)
            if not known:
                unknown.append(pos or "（空）")
            cx = rect.x() + floor(rect.width() * frac)
            cx += floor(float(info.get("offset_x", 0)))
            actor_baseline = baseline - floor(float(info.get("offset_y", 0)))
            facing = info.get("facing", "right")
            portrait = info.get("portrait", "normal")
            body_scale, art_facing = self._character_look(cid)
            draw_h = max(8, floor(h * body_scale / 100.0))
            pix = self._load_pixmap(self._portrait_path(cid, portrait))
            p.save()
            p.translate(cx, actor_baseline)
            p.rotate(float(info.get("rotation", 0)))
            if info.get("shocked"):
                p.translate(5, -3)
            if info.get("dimmed"):
                p.setOpacity(0.52)
            if not pix.isNull():
                scaled = pix.scaledToHeight(
                    draw_h, Qt.TransformationMode.SmoothTransformation
                )
                want_left = facing == "left"
                art_left = art_facing != "right"
                if want_left != art_left:
                    scaled = scaled.transformed(QTransform().scale(-1, 1))
                p.drawPixmap(
                    -scaled.width() // 2, -scaled.height(), scaled
                )
            else:
                self._paint_actor_placeholder(p, 0, 0, draw_h, cid, portrait)
            p.restore()
        if unknown:
            # 未识别站位：角落小字标注原值
            p.setPen(QColor(255, 200, 120))
            font = QFont(self.font())
            font.setPointSizeF(max(8.0, rect.height() * 0.022))
            p.setFont(font)
            note = "未识别站位（按中央处理）：" + "、".join(unknown)
            p.drawText(
                rect.adjusted(8, 6, -8, 0),
                Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignLeft,
                note,
            )

    def _paint_custom_cg(self, p: QPainter, rect: QRect) -> None:
        data = self._state.get("custom_cg")
        if not isinstance(data, dict) or not data.get("image"):
            return
        raw = str(data["image"])
        try:
            _record, source = content_registry.resolve(raw, expected_type="image")
            pix = self._load_pixmap(source)
        except content_registry.ContentRegistryError:
            pix = QPixmap()
        if pix.isNull():
            p.save()
            p.fillRect(rect, QColor(15, 15, 18, 210))
            p.setPen(QColor(255, 150, 120))
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter, f"缺失用户 CG：{raw}")
            p.restore()
            return
        scale_factor = max(0.1, min(3.0, float(data.get("scale", 100)) / 100.0))
        base = pix.scaled(
            rect.size(),
            Qt.AspectRatioMode.KeepAspectRatio,
            Qt.TransformationMode.SmoothTransformation,
        )
        target_w = max(1, floor(base.width() * scale_factor))
        target_h = max(1, floor(base.height() * scale_factor))
        shown = base.scaled(
            target_w,
            target_h,
            Qt.AspectRatioMode.IgnoreAspectRatio,
            Qt.TransformationMode.SmoothTransformation,
        )
        x = rect.center().x() - shown.width() // 2
        y = rect.center().y() - shown.height() // 2
        x += floor(rect.width() * float(data.get("x", 0)) / 100.0)
        y -= floor(rect.height() * float(data.get("y", 0)) / 100.0)
        p.save()
        try:
            p.setClipRect(rect)
            p.drawPixmap(x, y, shown)
        finally:
            p.restore()

    def _paint_overlays(self, p: QPainter, rect: QRect, layer: str) -> None:
        overlays = self._state.get("overlays") or {}
        positions = {
            "center": (0.50, 0.50), "top": (0.50, 0.18),
            "bottom": (0.50, 0.82), "left": (0.18, 0.50),
            "right": (0.82, 0.50), "top_left": (0.18, 0.18),
            "top_right": (0.82, 0.18), "bottom_left": (0.18, 0.82),
            "bottom_right": (0.82, 0.82),
        }
        for slot, data in sorted(overlays.items()):
            if data.get("layer", "front") != layer or not data.get("image"):
                continue
            raw = str(data["image"])
            try:
                _record, source = content_registry.resolve(raw, expected_type="image")
                pix = self._load_pixmap(source)
            except content_registry.ContentRegistryError:
                pix = QPixmap()
            if pix.isNull():
                p.save()
                p.setPen(QColor(255, 150, 120))
                p.drawText(rect, Qt.AlignmentFlag.AlignCenter, f"缺失插图 {slot}：{raw}")
                p.restore()
                continue
            scale = max(0.1, min(3.0, float(data.get("scale", 100)) / 100.0))
            base = pix.scaled(
                rect.size(), Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )
            shown = base.scaled(
                max(1, floor(base.width() * scale)),
                max(1, floor(base.height() * scale)),
                Qt.AspectRatioMode.IgnoreAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )
            px, py = positions.get(str(data.get("position")), positions["center"])
            x = rect.x() + floor(rect.width() * px) - shown.width() // 2
            y = rect.y() + floor(rect.height() * py) - shown.height() // 2
            p.save()
            try:
                p.setClipRect(rect)
                p.setOpacity(max(0.0, min(1.0, float(data.get("opacity", 100)) / 100.0)))
                p.drawPixmap(x, y, shown)
            finally:
                p.restore()

    def _paint_actor_placeholder(
        self, p: QPainter, cx: int, baseline: int, h: int, cid: str, portrait: str
    ) -> None:
        w = floor(h * 0.45)
        box = QRect(cx - w // 2, baseline - h, w, h)
        p.fillRect(box, PLACEHOLDER_BG)
        p.setPen(QPen(PLACEHOLDER_EDGE, 2))
        p.drawRect(box)
        name = models.character_name(self._editor_data, cid)
        p.setPen(QColor(240, 240, 240))
        font = QFont(self.font())
        font.setPointSizeF(max(9.0, h * 0.045))
        font.setBold(True)
        p.setFont(font)
        p.drawText(
            QRectF(box.adjusted(4, 4, -4, -4)),
            Qt.AlignmentFlag.AlignCenter | Qt.TextFlag.TextWordWrap,
            f"{name}\n（{portrait}）",
        )

    def _paint_dialog(self, p: QPainter, rect: QRect) -> None:
        dialog = self._state.get("dialog")
        if not dialog:
            return
        mode = dialog.get("mode", "character")
        text = dialog.get("text", "")
        wrap = (
            Qt.AlignmentFlag.AlignLeft
            | Qt.AlignmentFlag.AlignTop
            | Qt.TextFlag.TextWordWrap
            | Qt.TextFlag.TextWrapAnywhere
        )

        # 字号跟舞台高度走，不跟字幕栏走，避免栏太矮时字被裁成乱码。
        name_font = QFont(self.font())
        name_font.setPointSizeF(max(11.0, rect.height() * 0.036))
        name_font.setBold(True)
        text_font = QFont(self.font())
        text_font.setPointSizeF(max(10.0, rect.height() * 0.030))

        # center 模式：居中旁白，舞台中部横带、无说话者
        if mode == "center":
            pad = max(16, floor(rect.height() * 0.03))
            max_w = rect.width() - pad * 2
            fm = QFontMetrics(text_font)
            body = fm.boundingRect(
                QRect(0, 0, max(1, max_w), 10000), int(wrap), text
            )
            band_h = min(
                max(body.height() + pad * 2, floor(rect.height() * 0.18)),
                floor(rect.height() * 0.48),
            )
            band = QRect(
                rect.x(), rect.y() + (rect.height() - band_h) // 2, rect.width(), band_h
            )
            p.fillRect(band, QColor(0, 0, 0, 150))
            p.setFont(text_font)
            p.setPen(QColor(245, 245, 245))
            p.drawText(QRectF(band.adjusted(pad, pad // 2, -pad, -pad // 2)), wrap, text)
            return

        cid = dialog.get("character") or ""
        speaker_title = ""
        if mode == "narrative":
            speaker = "旁白"
        else:
            speaker = models.character_name(self._editor_data, cid) or cid or "？？"
            speaker_title = self._character_title(cid)
            if mode == "think":
                speaker += "（内心）"

        pad = max(14, floor(rect.height() * 0.028))
        inner_w = max(1, rect.width() - pad * 2)
        title_h = (
            QFontMetrics(text_font).height() + 2 if speaker_title else 0
        )
        name_h = QFontMetrics(name_font).height() + 4
        body = QFontMetrics(text_font).boundingRect(
            QRect(0, 0, inner_w, 10000), int(wrap), text
        )
        needed = title_h + name_h + body.height() + pad * 2 + 6
        bar_h = min(
            max(needed, floor(rect.height() * 0.22)),
            floor(rect.height() * 0.48),
        )
        bar = QRect(rect.x(), rect.bottom() - bar_h, rect.width(), bar_h)
        p.fillRect(bar, DIALOG_BG)
        inner = bar.adjusted(pad, pad, -pad, -pad)

        y = inner.y()
        if speaker_title:
            p.setFont(text_font)
            p.setPen(QColor(213, 184, 122))
            p.drawText(
                QRect(inner.x(), y, inner.width(), title_h),
                Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
                speaker_title,
            )
            y += title_h
        p.setFont(name_font)
        p.setPen(QColor(255, 214, 130))
        p.drawText(
            QRect(inner.x(), y, inner.width(), name_h),
            Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
            speaker,
        )
        p.setFont(text_font)
        p.setPen(QColor(245, 245, 245))
        p.drawText(
            QRectF(
                inner.x(),
                y + name_h,
                inner.width(),
                inner.height() - name_h,
            ),
            wrap,
            text,
        )

    def _paint_hint(self, p: QPainter, rect: QRect) -> None:
        """无法舞台化的节点（stat/item/mission/dice/panel 等）：底部灰字一行说明。"""
        hint = self._state.get("hint")
        if not hint or self._state.get("dialog"):
            return
        bar_h = max(24, floor(rect.height() * 0.10))
        bar = QRect(rect.x(), rect.bottom() - bar_h, rect.width(), bar_h)
        p.fillRect(bar, QColor(0, 0, 0, 140))
        font = QFont(self.font())
        font.setPointSizeF(max(9.0, bar_h * 0.34))
        p.setFont(font)
        p.setPen(QColor(170, 170, 170))
        p.drawText(
            bar.adjusted(12, 0, -12, 0),
            Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
            hint,
        )

    def _paint_choices(self, p: QPainter, rect: QRect) -> None:
        choice = self._state.get("choice")
        if not choice:
            return
        btn_w = floor(rect.width() * 0.56)
        btn_h = max(26, floor(rect.height() * 0.085))
        gap = floor(btn_h * 0.3)
        total = len(choice) * btn_h + (len(choice) - 1) * gap
        y = rect.bottom() - floor(rect.height() * 0.06) - total
        font = QFont(self.font())
        font.setPointSizeF(max(9.0, btn_h * 0.32))
        p.setFont(font)
        for opt in choice:
            box = QRect(rect.x() + (rect.width() - btn_w) // 2, y, btn_w, btn_h)
            p.fillRect(box, CHOICE_BG)
            p.setPen(QPen(CHOICE_EDGE, 1.5))
            p.drawRect(box)
            p.setPen(QColor(245, 245, 245))
            p.drawText(box, Qt.AlignmentFlag.AlignCenter, opt.get("text", ""))
            self._choice_rects.append((box, opt.get("goto", "")))
            y += btn_h + gap

    def _paint_caption(self, p: QPainter, rect: QRect) -> None:
        """右上角小字：当前节点 id 与类型；默认路线未经过时给普通提示。"""
        if not self._node_id:
            return
        font = QFont(self.font())
        font.setPointSizeF(max(8.0, rect.height() * 0.022))
        p.setFont(font)
        state = self._state
        node = self._current_node()
        ntype = node.get("type", "?")
        if state.get("reached"):
            text = f"{self._node_id} · {models.NODE_TYPE_CN.get(ntype, ntype)}"
            if state.get("via_branch"):
                text += "（沿通向此步的分支）"
            p.setPen(QColor(180, 180, 180, 200))
        else:
            text = (
                f"{self._node_id} · {models.NODE_TYPE_CN.get(ntype, ntype)}"
                " · 开头连不到此步，只显示本步"
            )
            p.setPen(QColor(210, 185, 125))
        p.drawText(
            rect.adjusted(0, 6, -8, 0),
            Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignRight,
            text,
        )

    # ------------------------------------------------------------ 交互
    def mousePressEvent(self, event) -> None:  # noqa: N802（Qt 命名）
        # 点击选项按钮 → 发信号通知主窗口跳转。信号槽里发生什么都不往回抛。
        try:
            pos = event.position().toPoint()
            for box, goto in self._choice_rects:
                if box.contains(pos):
                    if goto:
                        self.choice_activated.emit(goto)
                    event.accept()
                    return
        except Exception:
            log_crash("演出预览点击处理异常：\n" + traceback.format_exc())
        super().mousePressEvent(event)
