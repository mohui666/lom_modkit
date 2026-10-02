# -*- coding: utf-8 -*-
"""角色 / 背景素材选择器：左侧列表 + 右侧大图预览。

入口有三处，都复用同一个窗口：

- 「说话」「显示人物」等节点的人物字段（``mode="character"``）；
- 对白节点的表情字段（``mode="portrait"``，人物已定，只挑表情）；
- 「切换官方背景」节点的背景字段（``mode="view"``）。

窗口固定提供「人物」「背景」两个页签，方便挑角色的同时顺手看一眼背景；
但只有与当前字段类型相符的页签能点「使用选中」，另一个页签保持只读浏览，
避免把背景 id 填进人物字段。

预览图来自 :mod:`game_assets`：能从游戏目录取到的当场取，取不到的会明确
写出缺哪个资源 id，而不是只留一个没有解释的灰块。
"""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import (
    QObject,
    QRectF,
    QRunnable,
    QSize,
    Qt,
    QThreadPool,
    QTimer,
    Signal,
)
from PySide6.QtGui import QColor, QFont, QPainter, QPixmap
from PySide6.QtWidgets import (
    QAbstractItemView,
    QComboBox,
    QDialog,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QPushButton,
    QSizePolicy,
    QSplitter,
    QTabWidget,
    QVBoxLayout,
    QWidget,
)

import models
from i18n import t

MODE_CHARACTER = "character"
MODE_PORTRAIT = "portrait"
MODE_VIEW = "view"

THUMB_SIZE = 44
STAGE_ASPECT = 16 / 9
THUMB_BATCH = 6


class _LoadSignals(QObject):
    done = Signal(int, str, str)  # 请求号, 资源 key, 文件路径（空串 = 取不到）


class _LoadTask(QRunnable):
    """把「从游戏目录取图」放进线程池，避免大图解码和解包卡住界面。"""

    def __init__(self, token: int, key: str, fn, signals: _LoadSignals) -> None:
        super().__init__()
        self._token = token
        self._key = key
        self._fn = fn
        self._signals = signals

    def run(self) -> None:  # pragma: no cover - 线程内执行
        try:
            path = self._fn()
            text = str(path) if path else ""
        except Exception:  # noqa: BLE001 - 取图失败一律按「缺图」处理
            text = ""
        try:
            self._signals.done.emit(self._token, self._key, text)
        except RuntimeError:
            # 对话框已关闭、接收方已销毁：这是正常竞态，不必报错
            pass


class StageCanvas(QWidget):
    """16:9 预览画布：背景 + 立绘叠加，底部一条当前选择说明。"""

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self.setMinimumSize(320, 180)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self._background = QPixmap()
        self._portrait = QPixmap()
        self._caption = ""
        self._hint = ""

    def set_content(
        self,
        background: QPixmap | None = None,
        portrait: QPixmap | None = None,
        caption: str = "",
        hint: str = "",
    ) -> None:
        self._background = background or QPixmap()
        self._portrait = portrait or QPixmap()
        self._caption = caption
        self._hint = hint
        self.update()

    def _stage_rect(self) -> QRectF:
        """控件内取一块居中的 16:9 舞台区域。"""
        w, h = float(self.width()), float(self.height())
        if w <= 0 or h <= 0:
            return QRectF(0, 0, 0, 0)
        if w / h > STAGE_ASPECT:
            stage_h, stage_w = h, h * STAGE_ASPECT
        else:
            stage_w, stage_h = w, w / STAGE_ASPECT
        return QRectF((w - stage_w) / 2, (h - stage_h) / 2, stage_w, stage_h)

    def paintEvent(self, event) -> None:  # noqa: N802 - Qt 命名约定
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform)
        stage = self._stage_rect()
        if stage.width() <= 0:
            return

        painter.fillRect(self.rect(), QColor("#f2f2f2"))
        painter.fillRect(stage, QColor("#d8d8d8"))
        if not self._background.isNull():
            painter.drawPixmap(stage, self._background, self._background.rect())

        if not self._portrait.isNull():
            # 立绘按舞台高度 82% 等比缩放、底边贴舞台底，接近原版站位观感
            scaled = self._portrait.scaledToHeight(
                max(1, int(stage.height() * 0.82)),
                Qt.TransformationMode.SmoothTransformation,
            )
            painter.drawPixmap(
                int(stage.left() + (stage.width() - scaled.width()) / 2),
                int(stage.bottom() - scaled.height()),
                scaled,
            )

        if self._hint:
            painter.setPen(QColor("#9b2c2c"))
            font = QFont(self.font())
            font.setPointSize(font.pointSize() + 1)
            painter.setFont(font)
            painter.drawText(
                QRectF(stage.left() + 12, stage.top() + 12, stage.width() - 24, 72),
                int(Qt.AlignmentFlag.AlignHCenter | Qt.AlignmentFlag.AlignTop),
                self._hint,
            )

        if self._caption:
            band = QRectF(stage.left(), stage.bottom() - 26, stage.width(), 26)
            painter.fillRect(band, QColor(0, 0, 0, 140))
            painter.setPen(QColor("#ffffff"))
            painter.drawText(
                band.adjusted(8, 0, -8, 0),
                int(Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft),
                self._caption,
            )


class _AssetPane(QWidget):
    """一个页签：左侧带搜索的列表 + 右侧预览。``kind`` 为 ``character`` / ``view``。"""

    def __init__(self, dialog: "AssetPickerDialog", kind: str) -> None:
        super().__init__(dialog)
        self.dialog = dialog
        self.kind = kind
        self._items: list[tuple[str, str]] = []
        self._thumb_seen: set[str] = set()
        self._thumb_queue: list[str] = []

        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 8)
        root.setSpacing(6)

        search_row = QHBoxLayout()
        search_row.setSpacing(6)
        self.search = QLineEdit()
        self.search.setPlaceholderText(t("picker.search"))
        self.search.setClearButtonEnabled(True)
        self.search.textChanged.connect(self.refill)
        self.count_label = QLabel("")
        search_row.addWidget(self.search, 1)
        search_row.addWidget(self.count_label)
        root.addLayout(search_row)

        splitter = QSplitter(Qt.Orientation.Horizontal)
        splitter.setChildrenCollapsible(False)

        self.list = QListWidget()
        self.list.setUniformItemSizes(True)
        self.list.setIconSize(QSize(THUMB_SIZE, THUMB_SIZE))
        self.list.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)
        self.list.currentItemChanged.connect(lambda *_: self._on_selected())
        self.list.itemDoubleClicked.connect(lambda *_: self.dialog.try_accept())
        self.list.setMinimumWidth(210)
        splitter.addWidget(self.list)

        right = QWidget()
        right_layout = QVBoxLayout(right)
        right_layout.setContentsMargins(8, 0, 0, 0)
        right_layout.setSpacing(6)

        self.canvas = StageCanvas()
        right_layout.addWidget(self.canvas, 1)

        # 人物页签才需要「表情」和「预览背景」两行
        self.emotion_row = QWidget()
        emotion_layout = QHBoxLayout(self.emotion_row)
        emotion_layout.setContentsMargins(0, 0, 0, 0)
        emotion_layout.setSpacing(6)
        self.emotion_label = QLabel(t("picker.emotion"))
        self.emotion_combo = QComboBox()
        self.emotion_combo.setMinimumWidth(160)
        self.emotion_combo.currentIndexChanged.connect(lambda *_: self.dialog.refresh_preview(self))
        emotion_layout.addWidget(self.emotion_label)
        emotion_layout.addWidget(self.emotion_combo, 1)
        right_layout.addWidget(self.emotion_row)

        self.bg_row = QWidget()
        bg_layout = QHBoxLayout(self.bg_row)
        bg_layout.setContentsMargins(0, 0, 0, 0)
        bg_layout.setSpacing(6)
        self.bg_label = QLabel(t("picker.preview_bg"))
        self.bg_combo = QComboBox()
        self.bg_combo.setMinimumWidth(200)
        self.bg_combo.currentIndexChanged.connect(lambda *_: self.dialog.refresh_preview(self))
        bg_layout.addWidget(self.bg_label)
        bg_layout.addWidget(self.bg_combo, 1)
        right_layout.addWidget(self.bg_row)

        right.setMinimumWidth(280)
        splitter.addWidget(right)
        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)
        splitter.setSizes([240, 460])
        root.addWidget(splitter, 1)

        self._thumb_timer = QTimer(self)
        self._thumb_timer.setInterval(30)
        self._thumb_timer.timeout.connect(self._pump_thumbs)

    # ------------------------------------------------------------ 列表

    def set_items(self, items: list[tuple[str, str]]) -> None:
        self._items = items
        self.refill()

    def refill(self) -> None:
        needle = self.search.text().strip().lower()
        self.list.blockSignals(True)
        self.list.clear()
        for item_id, label in self._items:
            if needle and needle not in f"{item_id} {label}".lower():
                continue
            entry = QListWidgetItem(f"{label}  ({item_id})" if label else item_id)
            entry.setData(Qt.ItemDataRole.UserRole, item_id)
            entry.setToolTip(f"{label}\n{item_id}" if label else item_id)
            self.list.addItem(entry)
        self.list.blockSignals(False)
        self.count_label.setText(t("picker.count", shown=self.list.count(), total=len(self._items)))
        if self.list.count() and self.list.currentRow() < 0:
            self.list.setCurrentRow(0)
        self.start_thumb_pump()

    def start_thumb_pump(self) -> None:
        self._thumb_queue = [
            self.list.item(i).data(Qt.ItemDataRole.UserRole) for i in range(self.list.count())
        ]
        self._thumb_seen.clear()
        if self._thumb_queue:
            self._thumb_timer.start()

    def _pump_thumbs(self) -> None:
        """空闲时分批补缩略图：一次解几百张大图会明显卡住界面。"""
        if not self._thumb_queue:
            self._thumb_timer.stop()
            return
        batch, self._thumb_queue = self._thumb_queue[:THUMB_BATCH], self._thumb_queue[THUMB_BATCH:]
        pending = [i for i in batch if i and i not in self._thumb_seen]
        if not pending:
            return
        self._thumb_seen.update(pending)
        icons = self.dialog.thumbnails(self.kind, pending)
        for row in range(self.list.count()):
            entry = self.list.item(row)
            pix = icons.get(entry.data(Qt.ItemDataRole.UserRole))
            if pix is not None and not pix.isNull():
                entry.setIcon(pix)

    def current_value(self) -> str:
        entry = self.list.currentItem()
        return str(entry.data(Qt.ItemDataRole.UserRole) or "") if entry else ""

    def select_value(self, value: str) -> bool:
        for row in range(self.list.count()):
            entry = self.list.item(row)
            if entry.data(Qt.ItemDataRole.UserRole) == value:
                self.list.setCurrentRow(row)
                return True
        return False

    def emotion(self) -> str:
        data = self.emotion_combo.currentData()
        return str(data) if data else "normal"

    def preview_background(self) -> str:
        data = self.bg_combo.currentData()
        return str(data or "")

    def set_emotions(self, emotions: list[str], current: str) -> None:
        self.emotion_combo.blockSignals(True)
        self.emotion_combo.clear()
        for emo in emotions:
            self.emotion_combo.addItem(emo, emo)
        idx = self.emotion_combo.findData(current)
        if idx >= 0:
            self.emotion_combo.setCurrentIndex(idx)
        self.emotion_combo.blockSignals(False)

    def set_backgrounds(self, views: list[tuple[str, str]], current: str = "") -> None:
        self.bg_combo.blockSignals(True)
        self.bg_combo.clear()
        self.bg_combo.addItem(t("picker.no_bg"), "")
        for vid, label in views:
            self.bg_combo.addItem(f"{label}（{vid}）" if label else vid, vid)
        idx = self.bg_combo.findData(current) if current else -1
        self.bg_combo.setCurrentIndex(idx if idx >= 0 else 0)
        self.bg_combo.blockSignals(False)

    def _on_selected(self) -> None:
        if self.kind == MODE_CHARACTER:
            self.dialog.on_character_selected(self.current_value())
        self.dialog.refresh_preview(self)


class AssetPickerDialog(QDialog):
    """角色 / 背景素材选择器。

    ``mode`` 决定「使用选中」把哪个页签的结果交回调用方（``character`` /
    ``portrait`` / ``view``）；另一个页签仍可浏览，但不能提交。
    """

    def __init__(
        self,
        parent=None,
        *,
        editor_data: dict | None = None,
        library=None,
        mode: str = MODE_CHARACTER,
        initial: str = "",
        portrait_char: str = "",
        initial_emotion: str = "normal",
        initial_background: str = "",
        browse_only: bool = False,
    ) -> None:
        super().__init__(parent)
        self._editor_data = editor_data or models.FALLBACK_EDITOR_DATA
        self.library = library
        self.mode = mode if mode in (MODE_CHARACTER, MODE_PORTRAIT, MODE_VIEW) else MODE_CHARACTER
        self._browse_only = browse_only
        self._portrait_char = portrait_char or (initial if mode == MODE_PORTRAIT else "")
        self._initial = initial
        self._initial_emotion = initial_emotion or "normal"
        self._initial_background = initial_background

        self._signals = _LoadSignals()
        self._signals.done.connect(self._on_loaded)
        self._pool = QThreadPool.globalInstance()
        self._token = 0
        self._pending: dict[int, str] = {}  # 请求号 -> 资源 key
        self._pix_cache: dict[str, QPixmap] = {}
        self._thumb_cache: dict[str, QPixmap] = {}
        self._result: tuple[str, str] | None = None

        self.setWindowTitle(t("picker.title"))
        self.resize(1000, 660)

        root = QVBoxLayout(self)
        root.setContentsMargins(10, 10, 10, 10)
        root.setSpacing(8)

        self._header = QLabel(self._header_text())
        self._header.setWordWrap(True)
        root.addWidget(self._header)

        self.tabs = QTabWidget()
        self.char_pane = _AssetPane(self, MODE_CHARACTER)
        self.view_pane = _AssetPane(self, MODE_VIEW)
        self.tabs.addTab(self.char_pane, t("picker.tab_character"))
        self.tabs.addTab(self.view_pane, t("picker.tab_view"))
        self.tabs.currentChanged.connect(lambda *_: self._sync_accept_state())
        root.addWidget(self.tabs, 1)

        self.status = QLabel("")
        self.status.setWordWrap(True)
        root.addWidget(self.status)

        buttons = QHBoxLayout()
        buttons.addStretch(1)
        self.accept_btn = QPushButton(t("picker.use"))
        self.accept_btn.setMinimumHeight(30)
        self.accept_btn.setMinimumWidth(120)
        self.accept_btn.clicked.connect(self.try_accept)
        self.cancel_btn = QPushButton(t("common.cancel"))
        self.cancel_btn.setMinimumHeight(30)
        self.cancel_btn.setMinimumWidth(90)
        self.cancel_btn.clicked.connect(self.reject)
        buttons.addWidget(self.accept_btn)
        buttons.addWidget(self.cancel_btn)
        root.addLayout(buttons)

        self._populate()

    # ------------------------------------------------------------ 初始化

    def _header_text(self) -> str:
        if self.mode == MODE_PORTRAIT:
            return t("picker.header_portrait", char=self._portrait_char)
        if self.mode == MODE_VIEW:
            return t("picker.header_view")
        return t("picker.header_character")

    def _accept_kinds(self) -> set[str]:
        if self._browse_only:
            return {MODE_CHARACTER, MODE_VIEW}
        return {self.mode}

    def _populate(self) -> None:
        custom, official = models.character_combo_items(self._editor_data)
        char_items = [(cid, label) for cid, label in (list(custom) + list(official)) if cid]
        char_items.sort(key=lambda item: (item[1] or item[0], item[0]))
        self.char_pane.set_items(char_items)

        view_items = [(vid, label) for vid, label in models.list_items(self._editor_data, "views") if vid]
        view_items.sort(key=lambda item: (item[1] or item[0], item[0]))
        self.view_pane.set_items(view_items)

        # 人物页签底部的「预览背景」，只认本地已有的图，避免开窗时同步解包
        self.char_pane.set_backgrounds(
            view_items, current=self._initial_background if self.mode == MODE_VIEW else ""
        )
        self.char_pane.bg_combo.setCurrentIndex(
            max(0, self.char_pane.bg_combo.findData(self._default_preview_view()))
        )
        self.view_pane.bg_row.setVisible(False)

        if self.mode == MODE_PORTRAIT:
            self.char_pane.search.setVisible(False)
            self.char_pane.list.setEnabled(False)
            if not self.char_pane.select_value(self._portrait_char):
                # 自定义角色（user:xxx）不在官方清单里，补一行进去，
                # 否则列表会停在第一行，表情就被套到别的角色上了。
                self._pin_character_row(self._portrait_char)
            self.tabs.setCurrentIndex(0)
        elif self.mode == MODE_VIEW:
            self.view_pane.select_value(self._initial)
            self.tabs.setCurrentIndex(1)
        else:
            self.char_pane.select_value(self._initial)
            self.tabs.setCurrentIndex(0)

        self._sync_accept_state()
        self._refresh_emotions()
        self.refresh_preview(self.char_pane)
        self.refresh_preview(self.view_pane)

    def _pin_character_row(self, char_id: str) -> None:
        """把官方清单里没有的角色（自定义角色）插到人物列表首行并选中。"""
        if not char_id:
            return
        pane = self.char_pane
        label = models.character_name(self._editor_data, char_id) or char_id
        pane._items.insert(0, (char_id, label))
        pane.refill()
        pane.select_value(char_id)

    def _default_preview_view(self) -> str:
        """默认给人物预览配一张本地已缓存的背景，省得每次手动挑。"""
        if self.library is None:
            return ""
        for candidate in ("center", "free", "title"):
            path = self.library.asset_root / "views" / f"{candidate}.png"
            if path.is_file():
                return candidate
        return ""

    # ------------------------------------------------------------ 缩略图

    def thumbnails(self, kind: str, item_ids: list[str]) -> dict[str, QPixmap]:
        """只对本地已有缓存的条目生成缩略图，绝不在列表滚动时去解包。"""
        out: dict[str, QPixmap] = {}
        if self.library is None:
            return out
        for item_id in item_ids:
            key = f"{kind}:{item_id}"
            hit = self._thumb_cache.get(key)
            if hit is not None:
                out[item_id] = hit
                continue
            target = self._cached_thumb_source(kind, item_id)
            if target is None:
                continue
            pix = QPixmap(str(target))
            if pix.isNull():
                continue
            thumb = pix.scaled(
                THUMB_SIZE * 2,
                THUMB_SIZE * 2,
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )
            self._thumb_cache[key] = thumb
            out[item_id] = thumb
        return out

    def _cached_thumb_source(self, kind: str, item_id: str) -> Path | None:
        if self.library is None:
            return None
        if kind == MODE_VIEW:
            path = self.library.asset_root / "views" / f"{item_id}.png"
            return path if path.is_file() else None
        folder = self.library.asset_root / "portraits" / item_id
        if not folder.is_dir():
            return None
        try:
            emotions = self.library.emotions(item_id)
        except Exception:  # noqa: BLE001
            emotions = []
        for emo in emotions or ["normal"]:
            path = folder / f"{emo}.png"
            if path.is_file():
                return path
        return None

    # ------------------------------------------------------------ 预览

    def on_character_selected(self, char_id: str) -> None:
        self._refresh_emotions()

    def _refresh_emotions(self) -> None:
        char_id = self.char_pane.current_value() or self._portrait_char
        emotions: list[str] = []
        if self.library is not None and char_id:
            try:
                emotions = self.library.emotions(char_id)
            except Exception:  # noqa: BLE001
                emotions = []
        if not emotions:
            emotions = models.character_portraits(self._editor_data, char_id) or ["normal"]
        current = self.char_pane.emotion() or self._initial_emotion
        if char_id == self._portrait_char:
            current = self._initial_emotion
        self.char_pane.set_emotions(emotions, current)

    @staticmethod
    def _pix_key(kind: str, item_id: str, emotion: str = "") -> str:
        return f"view:{item_id}" if kind == MODE_VIEW else f"character:{item_id}:{emotion}"

    def refresh_preview(self, pane: _AssetPane) -> None:
        """按当前选择刷新预览；缺图时同步发起一次「从游戏取图」。"""
        if pane.kind == MODE_VIEW:
            view_id = pane.current_value()
            if not view_id:
                pane.canvas.set_content(hint=t("picker.nothing_selected"))
                return
            key = self._pix_key(MODE_VIEW, view_id)
            cached = self._pix_cache.get(key)
            if cached is not None:
                self._show_view(view_id, cached)
                return
            pane.canvas.set_content(caption=view_id, hint=t("picker.loading", name=view_id))
            self._start_load(key, lambda: self._view_path(view_id))
            return

        char_id = pane.current_value()
        if not char_id:
            pane.canvas.set_content(hint=t("picker.nothing_selected"))
            return
        emotion = pane.emotion()
        bg_view = pane.preview_background()
        bg_pix = self._pix_cache.get(f"view:{bg_view}") if bg_view else None
        caption = f"{char_id}   {emotion}" + (f"   @ {bg_view}" if bg_view else "")

        key = self._pix_key(MODE_CHARACTER, char_id, emotion)
        cached = self._pix_cache.get(key)
        if cached is not None:
            self._apply_portrait(char_id, emotion, cached, bg_pix)
        else:
            pane.canvas.set_content(
                background=bg_pix, caption=caption, hint=t("picker.loading", name=char_id)
            )
            self._start_load(key, lambda: self._portrait_path(char_id, emotion))
        if bg_view and f"view:{bg_view}" not in self._pix_cache:
            self._start_load(f"view:{bg_view}", lambda: self._view_path(bg_view))

    def _portrait_path(self, char_id: str, emotion: str):
        return None if self.library is None else self.library.portrait_file(char_id, emotion)

    def _view_path(self, view_id: str):
        return None if self.library is None else self.library.view_file(view_id)

    def _start_load(self, key: str, fn) -> None:
        if self.library is None:
            self._pix_cache.setdefault(key, QPixmap())
            self._handle_result(key)
            return
        self._token += 1
        self._pending[self._token] = key
        self._pool.start(_LoadTask(self._token, key, fn, self._signals))

    def _on_loaded(self, token: int, key: str, path: str) -> None:
        self._pending.pop(token, None)
        self._pix_cache[key] = QPixmap(path) if path else QPixmap()
        self._handle_result(key)

    def _handle_result(self, key: str) -> None:
        pix = self._pix_cache.get(key, QPixmap())
        if key.startswith("view:"):
            view_id = key.split(":", 1)[1]
            self._show_view(view_id, pix)
            # 人物页签正用这张图当底图时，一并重绘
            if self.char_pane.preview_background() == view_id:
                self.refresh_preview(self.char_pane)
            return
        _prefix, char_id, emotion = key.split(":", 2)
        self._apply_portrait(char_id, emotion, pix, None)

    def _show_view(self, view_id: str, pix: QPixmap) -> None:
        """背景页签的展示与说明。

        命中缓存和刚解出来两条路径都走这里——之前两条各写一遍，结果「命中缓存」
        那条漏了状态栏说明，用户只看到灰块却不知道为什么。
        """
        missing = t("picker.missing", name=view_id) if pix.isNull() else ""
        self.view_pane.canvas.set_content(background=pix, caption=view_id, hint=missing)
        if self.view_pane.current_value() == view_id:
            self.status.setText(self._missing_detail(view_id) if pix.isNull() else "")

    def _apply_portrait(
        self, char_id: str, emotion: str, pix: QPixmap, bg_pix: QPixmap | None
    ) -> None:
        pane = self.char_pane
        if pane.current_value() != char_id or pane.emotion() != emotion:
            return  # 结果已过期：用户又选了别的
        bg_view = pane.preview_background()
        if bg_pix is None and bg_view:
            bg_pix = self._pix_cache.get(f"view:{bg_view}")
        caption = f"{char_id}   {emotion}" + (f"   @ {bg_view}" if bg_view else "")
        if pix.isNull():
            pane.canvas.set_content(
                background=bg_pix,
                caption=caption,
                hint=t("picker.missing", name=f"{char_id}/{emotion}"),
            )
            self.status.setText(self._missing_detail(f"{char_id}/{emotion}"))
        else:
            pane.canvas.set_content(background=bg_pix, portrait=pix, caption=caption)
            self.status.setText("")

    def _missing_detail(self, name: str) -> str:
        """说清楚「为什么没有预览图」——不能只留一个没有解释的灰块。"""
        if self.library is None:
            return t("picker.no_library")
        ok, reason = self.library.probe()
        if not ok:
            return t("picker.not_ready", reason=reason)
        return t("picker.missing_detail", name=name)

    # ------------------------------------------------------------ 提交

    def _active_pane(self) -> _AssetPane:
        return self.view_pane if self.tabs.currentIndex() == 1 else self.char_pane

    def _sync_accept_state(self) -> None:
        allowed = self._active_pane().kind in self._accept_kinds()
        self.accept_btn.setEnabled(allowed)
        self.accept_btn.setToolTip("" if allowed else t("picker.wrong_tab"))

    def try_accept(self) -> None:
        if self._browse_only:
            # 独立浏览器：只是看一眼，不产出选择结果
            self.accept()
            return
        pane = self._active_pane()
        if pane.kind not in self._accept_kinds():
            self.status.setText(t("picker.wrong_tab"))
            return
        value = pane.current_value()
        if not value:
            self.status.setText(t("picker.nothing_selected"))
            return
        if pane.kind == MODE_VIEW:
            self._result = (MODE_VIEW, value)
        elif self.mode == MODE_PORTRAIT:
            self._result = (MODE_PORTRAIT, pane.emotion())
        else:
            self._result = (MODE_CHARACTER, value)
        self.accept()

    def result_value(self) -> tuple[str, str] | None:
        """返回 (结果类型, 值)；未确认时为 None。"""
        return self._result


def pick_asset(
    parent,
    *,
    editor_data: dict | None,
    library,
    mode: str,
    initial: str = "",
    portrait_char: str = "",
    initial_emotion: str = "normal",
    initial_background: str = "",
) -> tuple[str, str] | None:
    """便捷入口：打开选择器并返回 ``(类型, 值)``。"""
    dialog = AssetPickerDialog(
        parent,
        editor_data=editor_data,
        library=library,
        mode=mode,
        initial=initial,
        portrait_char=portrait_char,
        initial_emotion=initial_emotion,
        initial_background=initial_background,
    )
    if dialog.exec() != QDialog.DialogCode.Accepted:
        return None
    return dialog.result_value()
