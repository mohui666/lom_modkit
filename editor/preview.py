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
from PySide6.QtGui import QColor, QFont, QPainter, QPen, QPixmap, QTransform
from PySide6.QtWidgets import QWidget

import models

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
    if t in ("show", "move", "face", "hide", "scene", "say", "choice"):
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
        return f"[人物介绍卡] {cname()}"
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
        return (
            f"[敌方队伍] {models.enum_label('enemy_op', node.get('op', 'team'))}"
            f" {node.get('enemy', '')} {node.get('value', 0)}"
        )
    if t == "battle_skill":
        return (
            f"[战场技能] {models.enum_label('battle_skill_op', node.get('op', 'set'))}"
            f" {node.get('key', '')}"
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
            f"[骰子检定] {node.get('check', '')}"
            f"（{len(node.get('options', []))} 个选项，三向分支）"
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

    if t == "scene":
        state["view"] = node.get("view") or None
    elif t == "show":
        cid = node.get("character") or ""
        if cid:
            actors[cid] = {
                "position": node.get("position") or "M",
                "portrait": node.get("portrait") or "normal",
                "facing": node.get("facing") or "right",
            }
    elif t == "move":
        cid = node.get("character") or ""
        if cid in actors and node.get("to"):
            actors[cid]["position"] = node["to"]
    elif t == "face":
        cid = node.get("character") or ""
        if cid in actors and node.get("facing"):
            actors[cid]["facing"] = node["facing"]
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
    if t in ("end", "goto_scene"):
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
        # 三向分支：推演走第一个选项的"成功"线（无则大成功/失败）
        opts = node.get("options", [])
        if opts:
            for g in ("goto_成功", "goto_大成功", "goto_失败"):
                if opts[0].get(g):
                    return opts[0][g]
    elif node.get("goto"):
        return node["goto"]
    if idx + 1 < len(nodes):
        return nodes[idx + 1].get("id")
    return None


def simulate_stage(
    story: dict, target_id: str | None, editor_data: dict | None = None
) -> dict:
    """从 start 推演到 target_id（含目标节点自身效果），返回舞台状态。

    返回 dict：
      view     当前场景 id（无则 None）
      actors   {characterId: {position, portrait, facing}}（在台上的人物）
      dialog   当前节点为 say 时 {character, mode, text}，否则 None
      choice   当前节点为 choice 时 [{text, goto}]，否则 None
      hint     当前节点无法舞台化时的一行中文说明，否则 None
      reached  是否在步数上限内到达目标节点
      steps    实际推演步数
    """
    state = {
        "view": None,
        "actors": {},
        "dialog": None,
        "choice": None,
        "hint": None,
        "reached": False,
        "steps": 0,
    }
    nodes = story.get("nodes", []) if story else []
    if not nodes or not target_id:
        return state
    by_id = {n.get("id"): i for i, n in enumerate(nodes)}
    if target_id not in by_id:
        return state
    cur = story.get("start") or nodes[0].get("id")
    for step in range(MAX_STEPS):
        idx = by_id.get(cur)
        if idx is None:
            break  # goto 悬空，推演中断
        node = nodes[idx]
        state["steps"] = step + 1
        _apply_node(state, node, editor_data)
        if cur == target_id:
            state["reached"] = True
            break
        nxt = _next_node(node, idx, nodes)
        if nxt is None or nxt not in by_id:
            break
        cur = nxt
    return state


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

    def _load_pixmap(self, rel_path: str | None) -> QPixmap:
        """按相对 data/ 的路径加载图片（LRU 缓存 + 加载即降采样）。

        任何失败（文件缺失/损坏/解码异常）都返回 null QPixmap，调用方走占位图。
        """
        if not rel_path:
            return QPixmap()
        key = rel_path.replace("\\", "/")
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

    def _portrait_path(self, char_id: str, portrait: str) -> str | None:
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
            self._paint_background(p, rect)
            self._paint_actors(p, rect)
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
        pix = QPixmap()
        if image_path and self._story_root is not None:
            source = (self._story_root / image_path).resolve()
            try:
                source.relative_to(self._story_root.resolve())
                pix = self._load_pixmap(str(source))
            except (ValueError, OSError):
                pix = QPixmap()
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

    def _paint_background(self, p: QPainter, rect: QRect) -> None:
        view = self._state.get("view")
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
            facing = info.get("facing", "right")
            portrait = info.get("portrait", "normal")
            pix = self._load_pixmap(self._portrait_path(cid, portrait))
            if not pix.isNull():
                scaled = pix.scaledToHeight(
                    h, Qt.TransformationMode.SmoothTransformation
                )
                if facing == "left":
                    scaled = scaled.transformed(QTransform().scale(-1, 1))
                p.drawPixmap(
                    cx - scaled.width() // 2, baseline - scaled.height(), scaled
                )
            else:
                self._paint_actor_placeholder(p, cx, baseline, h, cid, portrait)
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

        # center 模式：居中旁白，舞台中部横带、无说话者
        if mode == "center":
            band_h = floor(rect.height() * 0.22)
            band = QRect(
                rect.x(), rect.y() + (rect.height() - band_h) // 2, rect.width(), band_h
            )
            p.fillRect(band, QColor(0, 0, 0, 150))
            font = QFont(self.font())
            font.setPointSizeF(max(10.0, band_h * 0.24))
            p.setFont(font)
            p.setPen(QColor(245, 245, 245))
            p.drawText(
                QRectF(band.adjusted(24, 8, -24, -8)),
                Qt.AlignmentFlag.AlignCenter | Qt.TextFlag.TextWordWrap,
                dialog.get("text", ""),
            )
            return

        bar_h = floor(rect.height() * 0.24)
        bar = QRect(rect.x(), rect.bottom() - bar_h, rect.width(), bar_h)
        p.fillRect(bar, DIALOG_BG)
        pad = floor(bar_h * 0.16)
        inner = bar.adjusted(pad, pad, -pad, -pad)

        cid = dialog.get("character") or ""
        if mode == "narrative":
            speaker = "旁白"
        else:
            speaker = models.character_name(self._editor_data, cid) or cid or "？？"
            if mode == "think":
                speaker += "（内心）"

        name_h = floor(inner.height() * 0.34)
        name_font = QFont(self.font())
        name_font.setPointSizeF(max(9.0, bar_h * 0.20))
        name_font.setBold(True)
        p.setFont(name_font)
        p.setPen(QColor(255, 214, 130))
        p.drawText(
            QRect(inner.x(), inner.y(), inner.width(), name_h),
            Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
            speaker,
        )

        text_font = QFont(self.font())
        text_font.setPointSizeF(max(9.0, bar_h * 0.17))
        p.setFont(text_font)
        p.setPen(QColor(245, 245, 245))
        p.drawText(
            QRectF(
                inner.x(), inner.y() + name_h, inner.width(), inner.height() - name_h
            ),
            Qt.AlignmentFlag.AlignLeft
            | Qt.AlignmentFlag.AlignTop
            | Qt.TextFlag.TextWordWrap,
            dialog.get("text", ""),
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
        """右上角小字：当前节点 id 与类型；推演未到达时给提示。"""
        if not self._node_id:
            return
        font = QFont(self.font())
        font.setPointSizeF(max(8.0, rect.height() * 0.022))
        p.setFont(font)
        state = self._state
        if state.get("reached"):
            node = self._current_node()
            ntype = node.get("type", "?")
            text = f"{self._node_id} · {models.NODE_TYPE_CN.get(ntype, ntype)}"
            p.setPen(QColor(180, 180, 180, 200))
        else:
            text = f"{self._node_id}：推演未到达（{state.get('steps', 0)} 步上限或路径中断）"
            p.setPen(QColor(255, 160, 120))
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
