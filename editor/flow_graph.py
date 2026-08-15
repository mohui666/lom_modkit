# -*- coding: utf-8 -*-
"""可交互剧情流程图。"""

from __future__ import annotations

import math

from PySide6.QtCore import QPointF, QRectF, Qt, Signal
from PySide6.QtGui import QBrush, QColor, QFont, QPainter, QPainterPath, QPen, QPolygonF
from PySide6.QtWidgets import (
    QGraphicsItem,
    QGraphicsPathItem,
    QGraphicsPolygonItem,
    QGraphicsRectItem,
    QGraphicsScene,
    QGraphicsTextItem,
    QGraphicsView,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

from i18n import t
import models
from story_graph import GraphEdge, StoryGraphAnalysis, analyze_story


CARD_W = 300.0
CARD_H = 76.0
ROW_GAP = 48.0
LEFT = 36.0
LANE_GAP = 26.0  # 侧边车道间距（每条边独占一条车道，互不重叠）

# 同一节点发出多条出边时的区分色（与深色背景 #12161d 协调；红色留给缺失目标）。
MULTI_EDGE_COLORS = (
    "#6fb3e0",  # 蓝
    "#70d7a6",  # 绿
    "#e0b56f",  # 橙
    "#b48ede",  # 紫
    "#5fcfcf",  # 青
    "#e08bb0",  # 粉
)


class _NodeCard(QGraphicsRectItem):
    def __init__(self, node_id: str, callback, rect: QRectF):
        super().__init__(rect)
        self.node_id = node_id
        self._callback = callback
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setToolTip(t("flow.card_tip"))
        self.setFlag(QGraphicsItem.GraphicsItemFlag.ItemIsFocusable, True)

    def mousePressEvent(self, event) -> None:  # noqa: N802 - Qt API
        self._callback(self.node_id)
        super().mousePressEvent(event)


class FlowGraphView(QGraphicsView):
    node_activated = Signal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        scene = QGraphicsScene(self)
        scene.setBackgroundBrush(QBrush(QColor("#12161d")))
        self.setScene(scene)
        self.setRenderHint(QPainter.RenderHint.Antialiasing, True)
        self.setDragMode(QGraphicsView.DragMode.ScrollHandDrag)
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        self.setAccessibleName(t("flow.name"))
        self.setToolTip(t("flow.tooltip"))
        self.setStyleSheet("QGraphicsView { background: #12161d; border: 1px solid #3d4658; }")

    def wheelEvent(self, event) -> None:  # noqa: N802 - Qt API
        if event.modifiers() & Qt.KeyboardModifier.ControlModifier:
            self.scale_by(1.12 if event.angleDelta().y() > 0 else 1 / 1.12)
            event.accept()
            return
        super().wheelEvent(event)

    def scale_by(self, factor: float) -> None:
        current = self.transform().m11()
        target = max(0.35, min(2.5, current * factor))
        self.scale(target / current, target / current)

    def fit_graph(self) -> None:
        bounds = self.scene().itemsBoundingRect().adjusted(-24, -24, 24, 24)
        if not bounds.isEmpty():
            self.fitInView(bounds, Qt.AspectRatioMode.KeepAspectRatio)

    def set_story(self, story: dict, editor_data: dict) -> StoryGraphAnalysis:
        scene = self.scene()
        scene.clear()
        analysis = analyze_story(story)
        node_by_id = {
            str(node.get("id")): node
            for node in story.get("nodes") or []
            if isinstance(node, dict) and node.get("id")
        }
        positions = {
            node_id: QPointF(LEFT, 30.0 + index * (CARD_H + ROW_GAP))
            for index, node_id in enumerate(analysis.node_order)
        }

        # 按源节点分组出边（用于颜色与出口错层）；指向下一行的边画直线，
        # 其余一律走右侧“肘形通道”：每条边独占一条全局车道（互不重叠），
        # 标签贴在源卡片右缘按行堆叠（不同源的标签天然不会撞车）。
        out_edges: dict[str, list[GraphEdge]] = {}
        drawable: list[GraphEdge] = []
        for edge in analysis.edges:
            if edge.source not in positions:
                continue
            if not edge.missing and edge.target not in positions:
                continue
            out_edges.setdefault(edge.source, []).append(edge)
            drawable.append(edge)

        def is_straight(edge: GraphEdge) -> bool:
            if edge.missing:
                return False
            source = positions[edge.source]
            target = positions[edge.target]
            return target.y() > source.y() and abs(
                target.y() - source.y() - CARD_H - ROW_GAP
            ) < 3

        side_edges = [edge for edge in drawable if not is_straight(edge)]
        side_edges.sort(key=lambda e: (positions[e.source].y(), self._sort_y(positions, e)))
        side_lane = {
            id(edge): LEFT + CARD_W + 42.0 + lane_index * LANE_GAP
            for lane_index, edge in enumerate(side_edges)
        }

        missing_ids = sorted({edge.target for edge in analysis.edges if edge.missing})
        placeholder_x = LEFT + CARD_W + 42.0 + len(side_edges) * LANE_GAP + 130.0
        for offset, target in enumerate(missing_ids):
            positions[f"?{target}"] = QPointF(
                placeholder_x, 30.0 + offset * (CARD_H + ROW_GAP)
            )

        # 先画边，让卡片始终盖在线条上方。出口/入口高度分别按源、按目标错层；
        # 同一对相邻节点之间的多条直线边（如 goto 与隐式下一步撞车）水平排开。
        exit_counts: dict[str, int] = {}
        entry_counts: dict[str, int] = {}
        straight_counts: dict[tuple[str, str], int] = {}
        for edge in side_edges:
            exit_counts[edge.source] = exit_counts.get(edge.source, 0) + 1
            key = f"?{edge.target}" if edge.missing else edge.target
            entry_counts[key] = entry_counts.get(key, 0) + 1
        for edge in drawable:
            if is_straight(edge):
                pair = (edge.source, edge.target)
                straight_counts[pair] = straight_counts.get(pair, 0) + 1
        exit_seen: dict[str, int] = {}
        entry_seen: dict[str, int] = {}
        straight_seen: dict[tuple[str, str], int] = {}

        for source, source_edges in out_edges.items():
            for out_index, edge in enumerate(source_edges):
                target_key = f"?{edge.target}" if edge.missing else edge.target
                if is_straight(edge):
                    pair = (source, edge.target)
                    straight_seen[pair] = straight_seen.get(pair, 0) + 1
                    self._draw_straight_edge(
                        edge, positions[source], positions[target_key],
                        out_index, len(source_edges),
                        straight_seen[pair] - 1, straight_counts[pair],
                    )
                    continue
                exit_seen[source] = exit_seen.get(source, 0) + 1
                entry_seen[target_key] = entry_seen.get(target_key, 0) + 1
                self._draw_side_edge(
                    edge,
                    positions[source],
                    positions[target_key],
                    side_lane[id(edge)],
                    out_index,
                    len(source_edges),
                    exit_seen[source] - 1,
                    exit_counts[source],
                    entry_seen[target_key] - 1,
                    entry_counts[target_key],
                )

        for node_id in analysis.node_order:
            self._draw_node(
                node_id,
                node_by_id.get(node_id, {}),
                positions[node_id],
                analysis,
                editor_data,
                node_id == story.get("start"),
            )
        for target in missing_ids:
            self._draw_missing(target, positions[f"?{target}"])

        scene.setSceneRect(scene.itemsBoundingRect().adjusted(-30, -30, 80, 30))
        return analysis

    @staticmethod
    def _sort_y(positions: dict, edge: GraphEdge) -> float:
        """侧边排序用的目标纵坐标；缺失目标排最后。"""
        if edge.missing:
            return float("inf")
        return positions[edge.target].y()

    @staticmethod
    def _edge_color(edge: GraphEdge, out_index: int, out_count: int) -> QColor:
        if edge.missing:
            return QColor("#ff6b6b")
        if out_count > 1:
            return QColor(MULTI_EDGE_COLORS[out_index % len(MULTI_EDGE_COLORS)])
        return QColor("#7f8aa3")

    def _pen_for(self, edge: GraphEdge, color: QColor) -> QPen:
        pen = QPen(color, 2.0)
        if edge.kind != "fallthrough":
            pen.setStyle(Qt.PenStyle.DashLine)
        return pen

    def _add_arrow(self, tip: QPointF, angle: float, color: QColor) -> None:
        arrow = QPolygonF(
            [
                tip,
                tip + QPointF(math.cos(angle + 0.55) * 10, math.sin(angle + 0.55) * 10),
                tip + QPointF(math.cos(angle - 0.55) * 10, math.sin(angle - 0.55) * 10),
            ]
        )
        item = QGraphicsPolygonItem(arrow)
        item.setPen(QPen(color))
        item.setBrush(QBrush(color))
        item.setZValue(-1)
        self.scene().addItem(item)

    def _add_label(self, text: str, pos: QPointF, color: QColor) -> None:
        """带深色底衬的标签，压在线条之上仍可读。"""
        label = QGraphicsTextItem(text)
        label.setDefaultTextColor(color)
        label.setFont(QFont("Microsoft YaHei UI", 8))
        label.setPos(pos)
        label.setZValue(1)
        self.scene().addItem(label)
        rect = label.boundingRect().translated(pos).adjusted(-2, -1, 2, 1)
        backdrop = QGraphicsRectItem(rect)
        backdrop.setPen(QPen(Qt.PenStyle.NoPen))
        backdrop.setBrush(QBrush(QColor("#12161d")))
        backdrop.setZValue(0.9)
        self.scene().addItem(backdrop)

    def _draw_straight_edge(
        self,
        edge: GraphEdge,
        source: QPointF,
        target: QPointF,
        out_index: int,
        out_count: int,
        parallel_index: int,
        parallel_count: int,
    ) -> None:
        """指向下一行的竖直直线（最常见的“下一步”）；同一对节点间多条时水平排开。"""
        color = self._edge_color(edge, out_index, out_count)
        offset = (parallel_index - (parallel_count - 1) / 2) * 14.0
        start = QPointF(source.x() + CARD_W / 2 + offset, source.y() + CARD_H)
        end = QPointF(target.x() + CARD_W / 2 + offset, target.y())
        path = QPainterPath(start)
        path.lineTo(end)
        item = QGraphicsPathItem(path)
        item.setPen(self._pen_for(edge, color))
        item.setZValue(-2)
        self.scene().addItem(item)
        self._add_arrow(end, math.pi / 2, color)
        label_color = color if (edge.missing or out_count > 1) else QColor("#c5cad6")
        label_pos = QPointF(end.x() + 4, start.y() + 6 + parallel_index * 13)
        self._add_label(edge.label, label_pos, label_color)

    def _draw_side_edge(
        self,
        edge: GraphEdge,
        source: QPointF,
        target: QPointF,
        lane: float,
        out_index: int,
        out_count: int,
        exit_index: int,
        exit_count: int,
        entry_index: int,
        entry_count: int,
    ) -> None:
        """右侧肘形通道：出卡 → 独占车道 → 进卡，车道全局唯一所以互不重叠。"""
        color = self._edge_color(edge, out_index, out_count)
        exit_y = source.y() + CARD_H * (exit_index + 1) / (exit_count + 1)
        entry_y = target.y() + CARD_H * (entry_index + 1) / (entry_count + 1)
        start = QPointF(source.x() + CARD_W, exit_y)
        # 真实目标从卡片右缘进（箭头朝左）；缺失占位卡在最右侧，从左缘进（箭头朝右）。
        if edge.missing:
            end = QPointF(target.x(), entry_y)
            angle = 0.0
        else:
            end = QPointF(target.x() + CARD_W, entry_y)
            angle = math.pi
        path = QPainterPath(start)
        path.lineTo(QPointF(lane, exit_y))
        path.lineTo(QPointF(lane, entry_y))
        path.lineTo(end)
        item = QGraphicsPathItem(path)
        item.setPen(self._pen_for(edge, color))
        item.setZValue(-2)
        self.scene().addItem(item)
        self._add_arrow(end, angle, color)

        # 标签按源卡片右缘纵向堆叠：同色同序对应各出口，不同源之间不会重叠。
        text = edge.label if len(edge.label) <= 14 else edge.label[:14] + "…"
        label_color = color if (edge.missing or out_count > 1) else QColor("#c5cad6")
        label_pos = QPointF(
            source.x() + CARD_W + 8, source.y() + 2 + exit_index * 14
        )
        self._add_label(text, label_pos, label_color)

    def _draw_node(
        self,
        node_id: str,
        node: dict,
        pos: QPointF,
        analysis: StoryGraphAnalysis,
        editor_data: dict,
        is_start: bool,
    ) -> None:
        problems: list[str] = []
        if node_id in analysis.missing_targets:
            problems.append("断路：目标不存在")
        if node_id in analysis.dead_ends:
            problems.append("断路：没有下一步")
        if node_id in analysis.infinite_loops:
            problems.append("死循环")
        if node_id in analysis.unreachable:
            problems.append("无法到达")
        error = bool(problems)
        card = _NodeCard(node_id, self.node_activated.emit, QRectF(0, 0, CARD_W, CARD_H))
        card.setPos(pos)
        card.setPen(QPen(QColor("#ff6b6b") if error else QColor("#657089"), 2.2 if error else 1.2))
        card.setBrush(QBrush(QColor("#2b1d23") if error else QColor("#202632")))
        card.setZValue(2)
        self.scene().addItem(card)

        title = QGraphicsTextItem(f"{node_id}  ·  {models.NODE_TYPE_CN.get(node.get('type'), node.get('type', '?'))}")
        title.setDefaultTextColor(QColor("#ffffff"))
        font = QFont("Microsoft YaHei UI", 10)
        font.setBold(True)
        title.setFont(font)
        title.setPos(pos + QPointF(12, 6))
        title.setZValue(3)
        self.scene().addItem(title)

        summary = models.node_summary(node, editor_data)
        detail = QGraphicsTextItem(summary[:38] + ("…" if len(summary) > 38 else ""))
        detail.setDefaultTextColor(QColor("#c9ced8"))
        detail.setFont(QFont("Microsoft YaHei UI", 8))
        detail.setPos(pos + QPointF(12, 31))
        detail.setZValue(3)
        self.scene().addItem(detail)

        badges = (["开始"] if is_start else []) + problems
        if badges:
            badge = QGraphicsTextItem("  |  ".join(badges))
            badge.setDefaultTextColor(QColor("#ff8b8b") if problems else QColor("#70d7a6"))
            badge.setFont(QFont("Microsoft YaHei UI", 8, QFont.Weight.DemiBold))
            badge.setPos(pos + QPointF(12, 52))
            badge.setZValue(3)
            self.scene().addItem(badge)

    def _draw_missing(self, target: str, pos: QPointF) -> None:
        rect = QGraphicsRectItem(QRectF(0, 0, CARD_W, CARD_H))
        rect.setPos(pos)
        pen = QPen(QColor("#ff6b6b"), 2)
        pen.setStyle(Qt.PenStyle.DashLine)
        rect.setPen(pen)
        rect.setBrush(QBrush(QColor("#301b22")))
        rect.setZValue(2)
        self.scene().addItem(rect)
        text = QGraphicsTextItem(f"缺失目标：{target}")
        text.setDefaultTextColor(QColor("#ff8b8b"))
        text.setFont(QFont("Microsoft YaHei UI", 10, QFont.Weight.DemiBold))
        text.setPos(pos + QPointF(12, 24))
        text.setZValue(3)
        self.scene().addItem(text)


class FlowGraphPanel(QWidget):
    node_activated = Signal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(4, 4, 4, 4)
        top = QHBoxLayout()
        self.summary = QLabel("流程正常")
        self.summary.setWordWrap(True)
        top.addWidget(self.summary, 1)
        fit_btn = QPushButton("适应窗口")
        minus_btn = QPushButton("缩小")
        plus_btn = QPushButton("放大")
        top.addWidget(fit_btn)
        top.addWidget(minus_btn)
        top.addWidget(plus_btn)
        layout.addLayout(top)
        self.view = FlowGraphView()
        self.view.node_activated.connect(self.node_activated)
        layout.addWidget(self.view, 1)
        fit_btn.clicked.connect(self.view.fit_graph)
        minus_btn.clicked.connect(lambda: self.view.scale_by(1 / 1.2))
        plus_btn.clicked.connect(lambda: self.view.scale_by(1.2))

    def set_story(self, story: dict, editor_data: dict) -> StoryGraphAnalysis:
        analysis = self.view.set_story(story, editor_data)
        counts = [
            (len(analysis.missing_targets) + len(analysis.dead_ends), "处断路"),
            (len(analysis.infinite_loops), "处死循环"),
            (len(analysis.unreachable), "个无法到达"),
        ]
        problems = [f"{count}{label}" for count, label in counts if count]
        if problems:
            self.summary.setText(t("flow.problems", text="，".join(problems)))
            self.summary.setStyleSheet("color: #ff8b8b; font-weight: 600;")
        else:
            self.summary.setText(t("flow.ok"))
            self.summary.setStyleSheet("color: #70d7a6; font-weight: 600;")
        return analysis
