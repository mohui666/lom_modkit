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

import models
from story_graph import GraphEdge, StoryGraphAnalysis, analyze_story


CARD_W = 300.0
CARD_H = 76.0
ROW_GAP = 48.0
LEFT = 36.0

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
        self.setToolTip("点击后在左侧步骤列表中定位")
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
        self.setAccessibleName("剧情流程图")
        self.setToolTip("拖动空白处平移；Ctrl+滚轮缩放；点击节点可定位")
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
        missing_ids = sorted({edge.target for edge in analysis.edges if edge.missing})
        for offset, target in enumerate(missing_ids):
            positions[f"?{target}"] = QPointF(
                LEFT + CARD_W + 245.0, 30.0 + offset * (CARD_H + ROW_GAP)
            )

        # 先画边，让卡片始终盖在线条上方。按源节点分组，同一节点的多条出边
        # 用组内序号错开车道、颜色和标签位置，避免曲线与标签互相叠合。
        out_edges: dict[str, list[GraphEdge]] = {}
        for edge in analysis.edges:
            target_key = f"?{edge.target}" if edge.missing else edge.target
            if edge.source not in positions or target_key not in positions:
                continue
            out_edges.setdefault(edge.source, []).append(edge)
        for source_edges in out_edges.values():
            for out_index, edge in enumerate(source_edges):
                target_key = f"?{edge.target}" if edge.missing else edge.target
                self._draw_edge(
                    edge,
                    positions[edge.source],
                    positions[target_key],
                    out_index,
                    len(source_edges),
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

    def _draw_edge(
        self,
        edge: GraphEdge,
        source: QPointF,
        target: QPointF,
        out_index: int,
        out_count: int,
    ) -> None:
        start = QPointF(source.x() + CARD_W / 2, source.y() + CARD_H)
        end = QPointF(target.x() + CARD_W / 2, target.y())
        if target.y() > source.y() and abs(target.y() - source.y() - CARD_H - ROW_GAP) < 3:
            path = QPainterPath(start)
            path.lineTo(end)
        else:
            side = source.x() + CARD_W + 42 + out_index * 20
            # 多条曲线从卡片右缘的不同高度发出/汇入，避免从同一点叠合。
            start_y = source.y() + CARD_H * (out_index + 1) / (out_count + 1)
            end_y = target.y() + CARD_H * (out_index + 1) / (out_count + 1)
            path = QPainterPath(QPointF(source.x() + CARD_W, start_y))
            path.cubicTo(
                QPointF(side, start_y),
                QPointF(side, end_y),
                QPointF(target.x() + CARD_W, end_y),
            )
            end = QPointF(target.x() + CARD_W, end_y)
        if edge.missing:
            color = QColor("#ff6b6b")
        elif out_count > 1:
            color = QColor(MULTI_EDGE_COLORS[out_index % len(MULTI_EDGE_COLORS)])
        else:
            color = QColor("#7f8aa3")
        pen = QPen(color, 2.0)
        if edge.kind != "fallthrough":
            pen.setStyle(Qt.PenStyle.DashLine)
        item = QGraphicsPathItem(path)
        item.setPen(pen)
        item.setZValue(-2)
        self.scene().addItem(item)

        angle = math.atan2(path.pointAtPercent(0.98).y() - end.y(), path.pointAtPercent(0.98).x() - end.x())
        arrow = QPolygonF(
            [
                end,
                end + QPointF(math.cos(angle + 0.55) * 10, math.sin(angle + 0.55) * 10),
                end + QPointF(math.cos(angle - 0.55) * 10, math.sin(angle - 0.55) * 10),
            ]
        )
        arrow_item = QGraphicsPolygonItem(arrow)
        arrow_item.setPen(QPen(color))
        arrow_item.setBrush(QBrush(color))
        arrow_item.setZValue(-1)
        self.scene().addItem(arrow_item)

        label = QGraphicsTextItem(edge.label)
        label.setDefaultTextColor(QColor("#c5cad6") if not edge.missing else color)
        label.setFont(QFont("Microsoft YaHei UI", 8))
        # 多条出边时标签沿路径错开，避免彼此以及直线“下一步”标签叠在一起。
        label_t = 0.48 if out_count <= 1 else 0.30 + 0.36 * out_index / (out_count - 1)
        label.setPos(path.pointAtPercent(label_t) + QPointF(5, -10))
        label.setZValue(1)
        self.scene().addItem(label)

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
            self.summary.setText("⚠ " + "，".join(problems) + "。点击红色节点即可定位。")
            self.summary.setStyleSheet("color: #ff8b8b; font-weight: 600;")
        else:
            self.summary.setText("✓ 流程正常：所有步骤可到达，并且能走到结局。")
            self.summary.setStyleSheet("color: #70d7a6; font-weight: 600;")
        return analysis
