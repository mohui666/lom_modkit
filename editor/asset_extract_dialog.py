# -*- coding: utf-8 -*-
"""一次性从游戏目录提取全部立绘 / 背景，并显示进度。

按需提取已经够日常用（点哪张取哪张），但首次打开一个陌生剧情时仍会一张张
现取。这个窗口提供「一次跑完」，把 400 多个角色的立绘和 150 多张背景全部
落到本地缓存，之后预览就是普通读图。

提取放在独立线程里跑，窗口可以随时「停止」；中断不会留下半张破损文件——
写入前先落到临时文件再原子替换（见 game_assets 的写入路径）。
"""

from __future__ import annotations

from PySide6.QtCore import QThread, Signal
from PySide6.QtWidgets import (
    QDialog,
    QHBoxLayout,
    QLabel,
    QProgressBar,
    QPushButton,
    QTextEdit,
    QVBoxLayout,
)

from i18n import t


class _ExtractWorker(QThread):
    """在后台线程里跑 library.extract_all()。"""

    progressed = Signal(int, int, str)  # 已完成, 总数, 当前文件名
    finished_with = Signal(dict)

    def __init__(self, library, parent=None) -> None:
        super().__init__(parent)
        self._library = library
        self._stop = False

    def request_stop(self) -> None:
        self._stop = True

    def run(self) -> None:  # pragma: no cover - 线程内执行
        try:
            report = self._library.extract_all(
                progress=lambda done, total, text: self.progressed.emit(done, total, text),
                should_stop=lambda: self._stop,
            )
        except Exception as exc:  # noqa: BLE001 - 线程内异常必须带回主线程
            report = {"ok": False, "reason": str(exc), "done": 0, "total": 0, "failed": []}
        self.finished_with.emit(report)


class ExtractAssetsDialog(QDialog):
    """「从游戏提取预览素材」进度窗口。"""

    def __init__(self, library, parent=None) -> None:
        super().__init__(parent)
        self.library = library
        self._worker: _ExtractWorker | None = None

        self.setWindowTitle(t("assets.extract_title"))
        self.resize(620, 420)

        root = QVBoxLayout(self)
        root.setContentsMargins(12, 12, 12, 12)
        root.setSpacing(8)

        self.summary = QLabel(self._summary_text())
        self.summary.setWordWrap(True)
        root.addWidget(self.summary)

        self.bar = QProgressBar()
        self.bar.setMinimum(0)
        self.bar.setMaximum(100)
        self.bar.setValue(0)
        root.addWidget(self.bar)

        self.log = QTextEdit()
        self.log.setReadOnly(True)
        self.log.setMinimumHeight(160)
        root.addWidget(self.log, 1)

        buttons = QHBoxLayout()
        self.start_btn = QPushButton(t("assets.extract_start"))
        self.start_btn.setMinimumHeight(30)
        self.start_btn.clicked.connect(self.start)
        self.stop_btn = QPushButton(t("assets.extract_stop"))
        self.stop_btn.setMinimumHeight(30)
        self.stop_btn.setEnabled(False)
        self.stop_btn.clicked.connect(self.stop)
        close_btn = QPushButton(t("common.close"))
        close_btn.setMinimumHeight(30)
        close_btn.clicked.connect(self.close)
        buttons.addWidget(self.start_btn)
        buttons.addWidget(self.stop_btn)
        buttons.addStretch(1)
        buttons.addWidget(close_btn)
        root.addLayout(buttons)

    def _summary_text(self) -> str:
        if self.library is None:
            return t("picker.no_library")
        ok, reason = self.library.probe()
        if not ok:
            return t("assets.not_ready", reason=reason)
        chars, views = self.library.total_counts()
        return t("assets.summary", characters=chars, views=views)

    def start(self) -> None:
        if self.library is None or self._worker is not None:
            return
        ok, reason = self.library.probe()
        if not ok:
            self.log.append(t("assets.not_ready", reason=reason))
            return
        self.start_btn.setEnabled(False)
        self.stop_btn.setEnabled(True)
        self.bar.setRange(0, 0)  # 未知总量时先走「忙碌」样式
        self.log.append(t("assets.started"))
        self._worker = _ExtractWorker(self.library, self)
        self._worker.progressed.connect(self._on_progress)
        self._worker.finished_with.connect(self._on_finished)
        self._worker.start()

    def stop(self) -> None:
        if self._worker is not None:
            self._worker.request_stop()
            self.log.append(t("assets.stopping"))

    def _on_progress(self, done: int, total: int, text: str) -> None:
        if total <= 0:
            return
        if self.bar.maximum() != total:
            self.bar.setRange(0, total)
        self.bar.setValue(done)
        self.bar.setFormat(t("assets.extract_progress", done=done, total=total))
        if text:
            self.log.append(f"{done}/{total}  {text}")

    def _on_finished(self, report: dict) -> None:
        self.start_btn.setEnabled(True)
        self.stop_btn.setEnabled(False)
        self._worker = None
        if not report.get("ok"):
            self.bar.setRange(0, 100)
            self.bar.setValue(0)
            self.log.append(t("assets.extract_failed", reason=report.get("reason", "")))
        else:
            done = int(report.get("done") or 0)
            failed = report.get("failed") or []
            self.bar.setRange(0, max(1, done))
            self.bar.setValue(done)
            if done == 0:
                self.log.append(t("assets.extract_none"))
            else:
                self.log.append(
                    t("assets.extract_done", done=done, failed=len(failed))
                )
            for path in failed[:20]:
                self.log.append(f"  ! {path}")
        self.summary.setText(self._summary_text())

    def closeEvent(self, event) -> None:  # noqa: N802 - Qt 命名
        """还在解包时不允许直接销毁：QThread 运行中被回收会直接崩进程。"""
        if self._worker is not None:
            self._worker.request_stop()
            if not self._worker.wait(15000):
                self.log.append(t("assets.stopping"))
                event.ignore()
                return
        super().closeEvent(event)
