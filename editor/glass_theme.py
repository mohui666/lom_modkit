# -*- coding: utf-8 -*-
"""液态玻璃（Liquid Glass）主题 — 编辑器全局样式。

设计来源：Apple HIG Materials / Liquid Glass 准则（技能 references/hig/liquid-glass.md）：
- 两层结构：功能层（工具栏/菜单/页签/按钮）用半透明玻璃浮于内容层之上；
- 内容层（列表/输入框/表格/预览画布）用更不透明的深色材质保证可读性；
- 玻璃本身无固有色：深色渐变窗体上叠白色低透明度填充 + 1px 高光描边；
- 强调色（ACCENT 蓝）克制使用：仅选中态、焦点描边和每屏唯一的主操作按钮。

Qt Widgets 无真实背景模糊（backdrop-filter），这里用 QSS 渐变底 + rgba 半透明
面板近似玻璃质感；纯样式实现，不触碰任何控件逻辑。主操作按钮通过动态属性
``primary=true`` 标记（``QPushButton[primary="true"]`` 规则命中）。
"""
from __future__ import annotations

import sys
from pathlib import Path

from PySide6.QtGui import QColor, QPalette
from PySide6.QtWidgets import QApplication, QWidget

# ---------------------------------------------------------------- 设计令牌
WINDOW_TOP = "#1c1f2b"  # 窗体渐变顶（深靛蓝）
WINDOW_BOTTOM = "#101218"  # 窗体渐变底（近黑）
GLASS_FILL = "rgba(255, 255, 255, 14)"  # 功能层玻璃（regular 变体）
GLASS_FILL_HOVER = "rgba(255, 255, 255, 26)"
GLASS_FILL_PRESS = "rgba(255, 255, 255, 38)"
GLASS_BORDER = "rgba(255, 255, 255, 34)"
GLASS_BORDER_SOFT = "rgba(255, 255, 255, 20)"
CONTENT_FILL = "rgba(13, 15, 23, 190)"  # 内容层材质（更不透明，保证文字对比度）
CONTENT_BORDER = "rgba(255, 255, 255, 24)"
SURFACE_RAISED = "rgb(30, 32, 44)"  # 浮层（菜单/下拉框），近不透明保可读性
TEXT = "#f2f2f7"  # 主文字（vibrancy primary）
TEXT_SECONDARY = "rgba(242, 242, 247, 160)"  # 次要文字（secondary）
TEXT_DISABLED = "rgba(242, 242, 247, 90)"  # 禁用（tertiary）
ACCENT = "#0a84ff"  # 系统蓝（深色外观）
ACCENT_BORDER = "rgba(96, 168, 255, 200)"
ACCENT_FILL = "rgba(10, 132, 255, 72)"  # 选中/勾选态染色玻璃
DANGER_TEXT = "#ff6b61"  # 编译错误红字（深色背景可读版）

_ASSET_ROOT = (
    Path(getattr(sys, "_MEIPASS"))
    if getattr(sys, "frozen", False)
    else Path(__file__).resolve().parent
)
COMBO_ARROW = (_ASSET_ROOT / "assets" / "combo_arrow.svg").as_posix()

QSS = f"""
/* ========== 基底：窗体深色渐变，子控件默认透明让玻璃叠在渐变上 ========== */
QWidget {{
    background: transparent;
    color: {TEXT};
}}
QMainWindow, QDialog {{
    background: qlineargradient(x1: 0, y1: 0, x2: 0.35, y2: 1,
        stop: 0 {WINDOW_TOP}, stop: 1 {WINDOW_BOTTOM});
}}

/* ========== 功能层：菜单栏 / 工具栏 / 状态栏 / 页签 ========== */
QMenuBar {{
    background: transparent;
    padding: 2px 4px;
}}
QMenuBar::item {{
    background: transparent;
    padding: 5px 12px;
    border-radius: 7px;
}}
QMenuBar::item:selected {{
    background: {GLASS_FILL_HOVER};
}}
QMenu {{
    background: {SURFACE_RAISED};
    border: 1px solid {GLASS_BORDER};
    border-radius: 10px;
    padding: 6px;
}}
QMenu::item {{
    padding: 6px 26px 6px 20px;
    border-radius: 6px;
}}
QMenu::item:selected {{
    background: {ACCENT_FILL};
}}
QMenu::separator {{
    height: 1px;
    background: {GLASS_BORDER_SOFT};
    margin: 5px 10px;
}}
QToolBar {{
    background: {GLASS_FILL};
    border: 1px solid {GLASS_BORDER_SOFT};
    border-radius: 10px;
    padding: 4px;
    margin: 4px 6px 0px 6px;
}}
QToolBar::separator {{
    background: {GLASS_BORDER_SOFT};
    width: 1px;
    margin: 6px 8px;
}}
QToolButton {{
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 6px 12px;
}}
QToolButton:hover {{
    background: {GLASS_FILL_HOVER};
    border-color: {GLASS_BORDER};
}}
QToolButton:pressed, QToolButton:checked {{
    background: {ACCENT_FILL};
    border-color: {ACCENT_BORDER};
}}
QStatusBar {{
    background: transparent;
    border-top: 1px solid {GLASS_BORDER_SOFT};
    color: {TEXT_SECONDARY};
}}
QTabWidget::pane {{
    background: rgba(255, 255, 255, 8);
    border: 1px solid {GLASS_BORDER_SOFT};
    border-radius: 12px;
    top: -1px;
}}
QTabBar::tab {{
    background: rgba(255, 255, 255, 10);
    border: 1px solid {GLASS_BORDER_SOFT};
    border-bottom: none;
    border-top-left-radius: 10px;
    border-top-right-radius: 10px;
    padding: 7px 18px;
    margin-right: 4px;
    color: {TEXT_SECONDARY};
}}
QTabBar::tab:selected {{
    background: {GLASS_FILL_HOVER};
    border-color: {GLASS_BORDER};
    color: {TEXT};
}}
QTabBar::tab:hover:!selected {{
    background: rgba(255, 255, 255, 18);
}}

/* ========== 功能层：按钮（含主操作 accent 染色玻璃） ========== */
QPushButton {{
    background: {GLASS_FILL};
    border: 1px solid {GLASS_BORDER};
    border-radius: 9px;
    padding: 6px 14px;
}}
QPushButton:hover {{
    background: {GLASS_FILL_HOVER};
    border-color: rgba(255, 255, 255, 52);
}}
QPushButton:pressed {{
    background: {GLASS_FILL_PRESS};
}}
QPushButton:checked {{
    background: {ACCENT_FILL};
    border-color: {ACCENT_BORDER};
}}
QPushButton:disabled {{
    background: rgba(255, 255, 255, 7);
    border-color: rgba(255, 255, 255, 16);
    color: {TEXT_DISABLED};
}}
QPushButton[primary="true"], QToolButton[primary="true"] {{
    background: rgba(10, 132, 255, 108);
    border-color: {ACCENT_BORDER};
    color: #ffffff;
}}
QPushButton[primary="true"]:hover, QToolButton[primary="true"]:hover {{
    background: rgba(10, 132, 255, 142);
}}
QPushButton[primary="true"]:pressed, QToolButton[primary="true"]:pressed {{
    background: rgba(10, 132, 255, 90);
}}

/* ========== 内容层：输入与展示控件（更不透明的深色材质） ========== */
QLineEdit, QPlainTextEdit, QTextEdit, QTextBrowser, QSpinBox, QDoubleSpinBox,
QComboBox {{
    background: {CONTENT_FILL};
    border: 1px solid {CONTENT_BORDER};
    border-radius: 8px;
    padding: 5px 9px;
    selection-background-color: rgba(10, 132, 255, 120);
}}
QComboBox {{
    padding-right: 34px;
}}
QLineEdit:focus, QPlainTextEdit:focus, QTextEdit:focus, QTextBrowser:focus,
QSpinBox:focus, QDoubleSpinBox:focus, QComboBox:focus {{
    border-color: {ACCENT_BORDER};
}}
QComboBox::drop-down {{
    border: none;
    border-left: 1px solid {CONTENT_BORDER};
    width: 30px;
    background: rgba(255, 255, 255, 8);
    border-top-right-radius: 8px;
    border-bottom-right-radius: 8px;
}}
QComboBox::drop-down:hover {{
    background: {GLASS_FILL_HOVER};
}}
QComboBox::down-arrow {{
    image: url({COMBO_ARROW});
    width: 12px;
    height: 8px;
}}
QComboBox QAbstractItemView {{
    background: {SURFACE_RAISED};
    border: 1px solid {GLASS_BORDER};
    outline: none;
    selection-background-color: {ACCENT_FILL};
}}
QComboBox QLineEdit {{
    background: transparent;
    border: none;
}}
QListWidget, QTableWidget {{
    background: {CONTENT_FILL};
    border: 1px solid {CONTENT_BORDER};
    border-radius: 10px;
    padding: 4px;
    outline: none;
}}
QListWidget::item {{
    border-radius: 6px;
    padding: 5px 8px;
}}
QListWidget::item:hover {{
    background: rgba(255, 255, 255, 14);
}}
QListWidget::item:selected {{
    background: {ACCENT_FILL};
    border: 1px solid rgba(96, 168, 255, 120);
}}
QTableWidget {{
    gridline-color: rgba(255, 255, 255, 18);
    selection-background-color: {ACCENT_FILL};
}}
QTableWidget::item:selected {{
    background: {ACCENT_FILL};
}}
QHeaderView::section {{
    background: rgba(255, 255, 255, 10);
    border: none;
    border-bottom: 1px solid {CONTENT_BORDER};
    padding: 5px 8px;
    color: {TEXT_SECONDARY};
}}
QTableCornerButton::section {{
    background: transparent;
    border: none;
}}
QGroupBox {{
    background: rgba(255, 255, 255, 10);
    border: 1px solid {GLASS_BORDER_SOFT};
    border-radius: 12px;
    margin-top: 14px;
    padding-top: 8px;
}}
QGroupBox::title {{
    subcontrol-origin: margin;
    left: 12px;
    padding: 0 4px;
    color: {TEXT_SECONDARY};
}}

/* ========== 复选框指示器交给 Fusion 按暗色调色板绘制（自带对勾） ========== */
QCheckBox {{
    spacing: 8px;
}}

/* ========== 滚动条：细、半透明、悬浮加深 ========== */
QScrollBar:vertical {{
    background: transparent;
    width: 10px;
    margin: 2px;
}}
QScrollBar:horizontal {{
    background: transparent;
    height: 10px;
    margin: 2px;
}}
QScrollBar::handle {{
    background: rgba(255, 255, 255, 46);
    border-radius: 5px;
    min-height: 28px;
    min-width: 28px;
}}
QScrollBar::handle:hover {{
    background: rgba(255, 255, 255, 72);
}}
QScrollBar::add-line, QScrollBar::sub-line,
QScrollBar::add-page, QScrollBar::sub-page {{
    background: transparent;
    border: none;
    height: 0;
    width: 0;
}}

/* ========== 分隔条 / 浮动提示 ========== */
QSplitter::handle {{
    background: rgba(255, 255, 255, 16);
}}
QSplitter::handle:horizontal {{
    width: 2px;
    margin: 4px 2px;
}}
QSplitter::handle:vertical {{
    height: 2px;
    margin: 2px 4px;
}}
QToolTip {{
    background: {SURFACE_RAISED};
    border: 1px solid {GLASS_BORDER};
    border-radius: 6px;
    padding: 5px 8px;
    color: {TEXT};
}}
"""


def _build_palette() -> QPalette:
    """暗色调色板：QSS 覆盖不到的部分（占位文本、复选框、禁用态）由它兜底。"""
    pal = QPalette()
    pal.setColor(QPalette.ColorRole.Window, QColor(WINDOW_BOTTOM))
    pal.setColor(QPalette.ColorRole.WindowText, QColor(TEXT))
    pal.setColor(QPalette.ColorRole.Base, QColor(13, 15, 23))
    pal.setColor(QPalette.ColorRole.AlternateBase, QColor(20, 22, 32))
    pal.setColor(QPalette.ColorRole.Text, QColor(TEXT))
    pal.setColor(QPalette.ColorRole.Button, QColor(32, 35, 48))
    pal.setColor(QPalette.ColorRole.ButtonText, QColor(TEXT))
    pal.setColor(QPalette.ColorRole.Highlight, QColor(10, 132, 255))
    pal.setColor(QPalette.ColorRole.HighlightedText, QColor("#ffffff"))
    pal.setColor(QPalette.ColorRole.PlaceholderText, QColor(242, 242, 247, 100))
    pal.setColor(QPalette.ColorRole.ToolTipBase, QColor(SURFACE_RAISED))
    pal.setColor(QPalette.ColorRole.ToolTipText, QColor(TEXT))
    pal.setColor(QPalette.ColorRole.BrightText, QColor(DANGER_TEXT))
    disabled = QColor(242, 242, 247, 90)
    pal.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.Text, disabled)
    pal.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.ButtonText, disabled)
    pal.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.WindowText, disabled)
    return pal


def apply_glass_theme(app: QApplication) -> None:
    """应用液态玻璃主题：Fusion 基样式 + 暗色调色板 + 全局 QSS。"""
    app.setStyle("Fusion")  # Fusion 对 QSS（圆角/rgba）的渲染在各平台最一致
    app.setPalette(_build_palette())
    app.setStyleSheet(QSS)


def mark_primary(widget: QWidget) -> None:
    """把控件标记为当前上下文唯一主操作（accent 染色玻璃，规范建议每屏最多一个）。"""
    widget.setProperty("primary", True)
    # 主题应用后再标记时需要重抛光才能命中新样式
    widget.style().unpolish(widget)
    widget.style().polish(widget)
