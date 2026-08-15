# -*- coding: utf-8 -*-
"""活侠传 Mod 剧情编辑器（PySide6）— 主窗口与入口。

三栏布局：左=剧情结构（章节 + 步骤树），中=当前对象属性，右=画面预览与编译结果。
菜单栏承载完整/低频功能；工具栏只留「试玩」「导出 Mod」两个高频动作。
无论从仓库根还是 editor/ 启动，路径都基于本文件所在目录推导项目根。

v4 起支持多剧情脚本管理（项目 = 多个 story + manifest，对应 .lommod 包）、
快照式撤销/重做（连续输入合并为一步）与脏标记（标题 * + 关闭/破坏操作确认）。
"""

from __future__ import annotations

import copy
from datetime import datetime
import faulthandler
import json
import os
import sys
import tempfile
import traceback
from pathlib import Path

from PySide6.QtCore import QSize, QTimer, Qt, Signal
from PySide6.QtGui import QAction, QDrag, QFont, QIcon, QKeySequence
from PySide6.QtWidgets import (
    QAbstractItemView,
    QApplication,
    QCheckBox,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFileDialog,
    QFormLayout,
    QGroupBox,
    QHBoxLayout,
    QHeaderView,
    QInputDialog,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QMainWindow,
    QMenu,
    QMessageBox,
    QPushButton,
    QSizePolicy,
    QSplitter,
    QStackedWidget,
    QTabWidget,
    QTableWidget,
    QTableWidgetItem,
    QTextBrowser,
    QToolBar,
    QToolButton,
    QVBoxLayout,
    QWidget,
)

import models

EDITOR_DIR = models.editor_dir()
PROJECT_ROOT = models.project_root()
if str(EDITOR_DIR) not in sys.path:
    sys.path.insert(0, str(EDITOR_DIR))
# lomc 编译器：sys.path 引入 <项目根>/compiler（不 pip 安装；冻结态由 PYZ 解析）
# 必须在 import package_io / node_form 之前，因为它们会加载 content_registry → lomc.content。
if str(PROJECT_ROOT / "compiler") not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT / "compiler"))

from help_content import current_help_html
from i18n import LANGUAGES, current_language, init_language, install_qt_translator, set_language, t
from glass_theme import apply_glass_theme, mark_primary
from game_install import (
    PREVIEW_PACKAGE_NAME,
    GameInstallError,
    GameInstallManager,
    build_story_read_keys,
    reset_story_read_state,
)
from flow_graph import FlowGraphPanel
from mod_manager_dialog import ModManagerDialog, apply_steam_launch_fix_ui
import content_registry
from diagnostic_bundle import export_diagnostic_bundle
import package_io
from preflight import PreflightIssue, apply_safe_fixes, run_preflight
from preflight_dialog import PreflightDialog
import stage_guard
from lua_preview import LuaPreview, compile_story, lomc_available, get_lomc
from node_form import NodeForm
from story_localization import StoryLocalizationDialog, apply_localization_settings
from schema_versions import manifest_versions
from preview import (
    CRASH_LOG,
    StagePreview,
    build_playtest_prelude,
    load_preview_map,
    log_crash,
)
# 文件对话框默认目录：冻结态用用户 CWD（解包目录/ exe 目录不可当作工作目录）
WORK_DIR = Path.cwd() if models.FROZEN else PROJECT_ROOT

def _app_title() -> str:
    return t("app.title")


UNDO_LIMIT = 100  # 撤销栈最大步数（快照式，超限丢最旧）
_ROLE_KIND = Qt.ItemDataRole.UserRole


class StepListWidget(QListWidget):
    """步骤树：可拖动重排；第 0 行「章节设置」固定不参与拖动。"""

    steps_moved = Signal(int, int)  # from_index, insert_index（均为 nodes[] 下标语义）

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setDragEnabled(True)
        self.setAcceptDrops(True)
        self.setDropIndicatorShown(True)
        self.setDefaultDropAction(Qt.DropAction.MoveAction)
        self.setDragDropMode(QAbstractItemView.DragDropMode.InternalMove)
        self.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)

    @staticmethod
    def _is_chapter(item: QListWidgetItem | None) -> bool:
        return item is not None and item.data(_ROLE_KIND) == "chapter"

    def startDrag(self, supported_actions) -> None:  # noqa: N802
        item = self.currentItem()
        if self._is_chapter(item) or item is None:
            return
        src = item.data(_ROLE_KIND)
        if not isinstance(src, int):
            return
        indexes = self.selectedIndexes()
        mime = self.model().mimeData(indexes) if indexes else None
        if mime is None:
            return
        drag = QDrag(self)
        drag.setMimeData(mime)
        rect = self.visualItemRect(item)
        grabbed = self.viewport().grab(rect)
        if not grabbed.isNull():
            drag.setPixmap(grabbed)
            drag.setHotSpot(rect.center() - rect.topLeft())
        # 自己 exec，不要走 QListWidget.startDrag：它在 MoveAction 成功后会删源行。
        drag.exec(Qt.DropAction.MoveAction, Qt.DropAction.MoveAction)

    def dragEnterEvent(self, event) -> None:  # noqa: N802
        if event.source() is self:
            event.setDropAction(Qt.DropAction.MoveAction)
            event.accept()
            return
        event.ignore()

    def dragMoveEvent(self, event) -> None:  # noqa: N802
        super().dragMoveEvent(event)
        if event.source() is self:
            event.setDropAction(Qt.DropAction.MoveAction)
            event.accept()
            return
        event.ignore()

    def dropEvent(self, event) -> None:  # noqa: N802
        if event.source() is not self:
            event.ignore()
            return
        src_item = self.currentItem()
        if self._is_chapter(src_item) or src_item is None:
            event.ignore()
            return
        src = src_item.data(_ROLE_KIND)
        if not isinstance(src, int):
            event.ignore()
            return
        dest = self._drop_insert_index(event)
        if dest is None:
            dest = src
        event.setDropAction(Qt.DropAction.MoveAction)
        event.accept()
        QTimer.singleShot(0, lambda s=src, d=dest: self.steps_moved.emit(s, d))

    def _drop_insert_index(self, event) -> int | None:
        pos = event.position().toPoint()
        item = self.itemAt(pos)
        node_indices = [
            int(self.item(row).data(_ROLE_KIND))
            for row in range(self.count())
            if isinstance(self.item(row).data(_ROLE_KIND), int)
        ]
        node_count = max(node_indices, default=-1) + 1
        if item is None:
            return node_count
        if self._is_chapter(item):
            return 0
        idx = item.data(_ROLE_KIND)
        if isinstance(idx, tuple) and len(idx) >= 4 and idx[0] == "structure":
            return int(idx[3])
        if not isinstance(idx, int):
            return node_count
        indicator = self.dropIndicatorPosition()
        if indicator == QAbstractItemView.DropIndicatorPosition.BelowItem:
            return idx + 1
        return idx

_crash_logging_installed = False


def new_editor_story(story_id: str = "main", editor_data: dict | None = None) -> dict:
    """创建可立即通过编译检查的新手项目模板。

    models.new_story 的 登场+对白 双节点结构是 story_api 的既有契约（先登场再
    动作，否则游戏黑屏）；图形编辑器在其后补一个“结束剧情”，避免新用户什么都
    没做就先看到“最后节点无法结束”的红色错误。
    """
    story = models.new_story(story_id, editor_data)
    story["nodes"][1]["text"] = t("new_story_text")  # nodes[1] 是 say
    story["nodes"].append(models.new_node("end", models.make_node_id(story, "end"), editor_data))
    return story


def _excepthook(exc_type, exc, tb) -> None:
    """未捕获异常 → crash.log（PySide6 槽/虚函数里的异常经 PyErr_Print 也会走这里）。"""
    try:
        log_crash(
            "未捕获异常：\n" + "".join(traceback.format_exception(exc_type, exc, tb))
        )
    except Exception:
        pass
    try:
        if sys.__excepthook__ is not None:
            sys.__excepthook__(exc_type, exc, tb)
    except Exception:
        pass  # pythonw 下 stderr 为 None，默认钩子自身也可能再抛，兜住


def install_crash_logging() -> None:
    """安装崩溃取证：未捕获异常 + 致命错误（段错误等）都追加写 editor/crash.log。

    pythonw 启动时 stderr/stdout 为 None，一并重定向进日志文件，
    否则 Qt/PySide 打印的错误会被吞掉（甚至引发二次异常）。
    """
    global _crash_logging_installed
    if _crash_logging_installed:
        return
    _crash_logging_installed = True
    sys.excepthook = _excepthook
    try:
        if sys.stderr is None:
            sys.stderr = open(CRASH_LOG, "a", encoding="utf-8", buffering=1)
        if sys.stdout is None:
            sys.stdout = (
                sys.stderr
                if sys.stderr is not None
                else open(CRASH_LOG, "a", encoding="utf-8", buffering=1)
            )
    except Exception:
        pass
    try:
        # 独立文件句柄给 faulthandler（进程存活期间保持打开）
        faulthandler.enable(file=open(CRASH_LOG, "a", encoding="utf-8"))
    except Exception:
        try:
            faulthandler.enable()
        except Exception:
            pass


install_crash_logging()  # 模块导入即生效（测试脚本直接 import main 也有取证）


class HelpDialog(QDialog):
    """离线内置的快速入门与排错文档。"""

    def __init__(self, parent=None, manager: GameInstallManager | None = None):
        super().__init__(parent)
        self._manager = manager
        self.setWindowTitle(t("app.help_title"))
        self.resize(760, 620)
        layout = QVBoxLayout(self)
        browser = QTextBrowser()
        browser.setOpenExternalLinks(False)
        browser.setHtml(current_help_html())
        layout.addWidget(browser)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        buttons.rejected.connect(self.reject)
        close_btn = buttons.button(QDialogButtonBox.StandardButton.Close)
        if close_btn is not None:
            close_btn.setText(t("help.close"))
        if self._manager is not None:
            fix_btn = QPushButton(t("help.steam_fix"))
            fix_btn.setToolTip(t("help.steam_fix_tip"))
            fix_btn.clicked.connect(self._apply_steam_fix)
            buttons.addButton(fix_btn, QDialogButtonBox.ButtonRole.ActionRole)
        layout.addWidget(buttons)

    def _apply_steam_fix(self) -> None:
        if self._manager is None:
            return
        apply_steam_launch_fix_ui(self, self._manager)


class ManifestDialog(QDialog):
    """导出 .lommod 时填写 manifest 元信息（契约 §2，含 campaign 战役区）。

    base_manifest：导入 .lommod 时读到的原 manifest，用于回填（campaign 往返）。
    """

    def __init__(
        self,
        story_id: str,
        editor_data: dict,
        story_ids: list[str],
        base_manifest: dict | None = None,
        parent=None,
    ):
        super().__init__(parent)
        base = base_manifest or {}
        campaign = base.get("campaign") or {}
        self._editor_data = editor_data
        self._story_ids = list(story_ids)

        self.setWindowTitle(t("export.title"))
        self.resize(920, 560)
        layout = QVBoxLayout(self)
        intro = QLabel(t("export.intro"))
        intro.setWordWrap(True)
        layout.addWidget(intro)
        form = QFormLayout()
        self.id_edit = QLineEdit(str(base.get("id") or "my_mod"))
        self.name_edit = QLineEdit(str(base.get("name") or t("export.default_name")))
        self.version_edit = QLineEdit(str(base.get("version") or "1.0.0"))
        self.author_edit = QLineEdit(str(base.get("author") or ""))
        self.desc_edit = QLineEdit(str(base.get("description") or ""))
        self.id_edit.setPlaceholderText(t("export.id_ph"))
        self.author_edit.setPlaceholderText(t("export.author_ph"))
        self.desc_edit.setPlaceholderText(t("export.desc_ph"))
        form.addRow(t("export.id"), self.id_edit)
        form.addRow(t("export.name"), self.name_edit)
        form.addRow(t("export.version"), self.version_edit)
        form.addRow(t("export.author"), self.author_edit)
        form.addRow(t("export.desc"), self.desc_edit)
        # 多剧情：入口脚本从包内全部剧情脚本里选（回填 base entry / 当前 story）
        self.entry_combo = QComboBox()
        self.entry_combo.setEditable(False)
        for sid in self._story_ids:
            self.entry_combo.addItem(sid, sid)
        default_entry = str(base.get("entry") or story_id)
        idx = self.entry_combo.findData(default_entry)
        self.entry_combo.setCurrentIndex(idx if idx >= 0 else 0)
        form.addRow(t("export.entry"), self.entry_combo)
        layout.addLayout(form)

        # ------------------------------------------------------ campaign 区
        camp_box = QGroupBox(t("export.campaign"))
        cv = QVBoxLayout(camp_box)
        self.new_game_check = QCheckBox(t("export.new_game"))
        self.new_game_check.setChecked(bool(campaign.get("new_game")))
        cv.addWidget(self.new_game_check)
        self.disable_events_check = QCheckBox(t("export.disable_events"))
        self.disable_events_check.setChecked(
            bool(campaign.get("disable_official_events"))
        )
        cv.addWidget(self.disable_events_check)
        trigger_help = QLabel(t("export.trigger_help"))
        trigger_help.setWordWrap(True)
        cv.addWidget(trigger_help)
        self.triggers_table = QTableWidget(0, 7)
        self.triggers_table.setHorizontalHeaderLabels(
            [
                t("export.col.position"),
                t("export.col.script"),
                t("export.col.flag_set"),
                t("export.col.flag_clear"),
                t("export.col.month"),
                t("export.col.stage"),
                t("export.col.affinity"),
            ]
        )
        self.triggers_table.horizontalHeader().setSectionResizeMode(
            0, QHeaderView.ResizeMode.ResizeToContents
        )
        for c in (1, 2, 3, 4, 5, 6):
            self.triggers_table.horizontalHeader().setSectionResizeMode(
                c, QHeaderView.ResizeMode.Stretch
            )
        self.triggers_table.setMinimumHeight(120)
        cv.addWidget(self.triggers_table)
        btns = QHBoxLayout()
        add_btn = QPushButton(t("export.add_trigger"))
        del_btn = QPushButton(t("export.del_trigger"))
        add_btn.clicked.connect(lambda: self._add_trigger_row({}))
        del_btn.clicked.connect(self._del_trigger_row)
        btns.addWidget(add_btn)
        btns.addWidget(del_btn)
        btns.addStretch(1)
        cv.addLayout(btns)
        layout.addWidget(camp_box)
        for trig in campaign.get("triggers") or []:
            if isinstance(trig, dict):
                self._add_trigger_row(trig)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        ok_btn = buttons.button(QDialogButtonBox.StandardButton.Ok)
        cancel_btn = buttons.button(QDialogButtonBox.StandardButton.Cancel)
        if ok_btn is not None:
            ok_btn.setText(t("export.continue"))
            mark_primary(ok_btn)  # 对话框唯一主操作：accent 染色玻璃
        if cancel_btn is not None:
            cancel_btn.setText(t("export.cancel"))
        layout.addWidget(buttons)

    # ---------------------------------------------------------- triggers 表
    def _add_trigger_row(self, trig: dict) -> None:
        table = self.triggers_table
        r = table.rowCount()
        table.insertRow(r)
        # 位置：free_positions 清单（schema 2 显示 "名字（id）"）
        pos = QComboBox()
        pos.setEditable(True)
        for pid, disp in models.list_items(self._editor_data, "free_positions"):
            pos.addItem(disp, pid)
        self._set_combo_value(pos, str(trig.get("position", "")))
        table.setCellWidget(r, 0, pos)
        # 脚本：包内 story id
        script = QComboBox()
        script.setEditable(False)
        for sid in self._story_ids:
            script.addItem(sid, sid)
        self._set_combo_value(script, str(trig.get("script", "")))
        table.setCellWidget(r, 1, script)
        table.setItem(r, 2, QTableWidgetItem(str(trig.get("when_flag_set", ""))))
        table.setItem(r, 3, QTableWidgetItem(str(trig.get("when_flag_clear", ""))))
        # 月份/旬：int 条件；空则不写
        wm = trig.get("when_month")
        table.setItem(r, 4, QTableWidgetItem("" if wm is None else str(wm)))
        ws = trig.get("when_stage")
        table.setItem(r, 5, QTableWidgetItem("" if ws is None else str(ws)))
        # 好感：{"character": <人物>, "min": <数值>} → "人物:数值"（如 brother4:3）
        wa = trig.get("when_affinity")
        aff_text = ""
        if isinstance(wa, dict) and wa.get("character"):
            aff_text = "%s:%s" % (wa["character"], wa.get("min", ""))
        table.setItem(r, 6, QTableWidgetItem(aff_text))

    @staticmethod
    def _set_combo_value(combo: QComboBox, value: str) -> None:
        """可编辑下拉框回填：空值显式置空（否则默认停在第一项，空行变有效行）。"""
        idx = combo.findData(value)
        if idx >= 0:
            combo.setCurrentIndex(idx)
        elif value and combo.isEditable():
            combo.setCurrentText(value)
        elif value:
            combo.setCurrentIndex(-1)
        else:
            combo.setCurrentIndex(-1)

    def _del_trigger_row(self) -> None:
        rows = self.triggers_table.rowCount()
        if rows:
            self.triggers_table.removeRow(rows - 1)

    @staticmethod
    def _cell_text(table: QTableWidget, row: int, col: int) -> str:
        item = table.item(row, col)
        return item.text().strip() if item else ""

    def manifest(self) -> dict:
        m = {
            **manifest_versions(),
            "id": self.id_edit.text().strip() or "my_mod",
            "name": self.name_edit.text().strip(),
            "version": self.version_edit.text().strip() or "1.0.0",
            "author": self.author_edit.text().strip(),
            "description": self.desc_edit.text().strip(),
            "entry": self.entry_combo.currentText().strip() or "main",
        }
        # campaign：勾了 new_game 或有有效触发器时才写出（契约 §2 可选段）
        triggers = []
        table = self.triggers_table
        for r in range(table.rowCount()):
            pos_combo = table.cellWidget(r, 0)
            script_combo = table.cellWidget(r, 1)
            if not (
                isinstance(pos_combo, QComboBox) and isinstance(script_combo, QComboBox)
            ):
                continue  # 异常行防御（cellWidget 静态类型是 QWidget）
            position = str(pos_combo.currentData() or pos_combo.currentText()).strip()
            script = script_combo.currentText().strip()
            if not position or not script:
                continue  # 位置/脚本缺一不可，缺了跳过该行
            trig: dict = {"type": "position", "position": position, "script": script}
            flag_set = self._cell_text(table, r, 2)
            flag_clear = self._cell_text(table, r, 3)
            if flag_set:
                trig["when_flag_set"] = flag_set
            if flag_clear:
                trig["when_flag_clear"] = flag_clear
            # 月份/旬：尝试转 int（转不了原样写出，交给 lomc validate 报错）
            month_text = self._cell_text(table, r, 4)
            if month_text:
                try:
                    trig["when_month"] = int(month_text)
                except ValueError:
                    trig["when_month"] = month_text
            stage_text = self._cell_text(table, r, 5)
            if stage_text:
                try:
                    trig["when_stage"] = int(stage_text)
                except ValueError:
                    trig["when_stage"] = stage_text
            # 好感："角色:数值" 拆 character/min；无冒号时整串当 character
            aff_text = self._cell_text(table, r, 6)
            if aff_text:
                if ":" in aff_text:
                    char_part, _, min_part = aff_text.partition(":")
                    min_part = min_part.strip()
                    try:
                        min_val = int(min_part)
                    except ValueError:
                        min_val = min_part  # 非法值原样给 lomc 报错
                else:
                    char_part, min_val = aff_text, ""
                trig["when_affinity"] = {
                    "character": char_part.strip(),
                    "min": min_val,
                }
            triggers.append(trig)
        campaign: dict = {}
        if self.new_game_check.isChecked():
            campaign["new_game"] = True
        if self.disable_events_check.isChecked():
            campaign["disable_official_events"] = True
        if triggers:
            campaign["triggers"] = triggers
        if campaign:
            m["campaign"] = campaign
        return m

    def accept(self) -> None:
        """在关闭窗口前指出可修正的问题，避免选完保存位置后才打包失败。"""
        mod_id = self.id_edit.text().strip()
        if not models.MOD_ID_PATTERN.fullmatch(mod_id):
            QMessageBox.warning(
                self,
                t("export.bad_id"),
                t("export.bad_id_msg"),
            )
            self.id_edit.setFocus()
            return
        for widget, label in (
            (self.name_edit, t("export.name")),
            (self.author_edit, t("export.author")),
            (self.desc_edit, t("export.desc")),
        ):
            if not widget.text().strip():
                QMessageBox.warning(self, t("export.incomplete"), t("export.fill", label=label))
                widget.setFocus()
                return
        entry = self.entry_combo.currentText().strip()
        if entry not in self._story_ids:
            QMessageBox.warning(self, t("export.bad_entry"), t("export.bad_entry_msg"))
            self.entry_combo.setFocus()
            return
        manifest = self.manifest()
        for trig in (manifest.get("campaign") or {}).get("triggers", []):
            if trig.get("script") not in self._story_ids:
                QMessageBox.warning(
                    self,
                    t("export.bad_trigger"),
                    t("export.bad_trigger_msg", script=repr(trig.get("script"))),
                )
                return
        lomc, err = get_lomc()
        if lomc is None:
            QMessageBox.warning(self, t("export.no_compiler"), t("export.compiler_err", err=err))
            return
        try:
            lomc.validate_manifest(manifest, "导出设置")
        except Exception as exc:
            QMessageBox.warning(self, t("export.need_fix"), str(exc))
            return
        super().accept()


class MainWindow(QMainWindow):
    def __init__(self, editor_data: dict, is_fallback: bool, parent=None):
        super().__init__(parent)
        self.setWindowTitle(_app_title())
        self.resize(1280, 760)

        self.editor_data = editor_data
        self.game_manager = GameInstallManager()
        # 多剧情项目状态：_stories = {脚本id: story dict}，story 是当前脚本的引用
        self._stories: dict[str, dict] = {}
        self._current_id = ""
        self.story = new_editor_story(editor_data=editor_data)
        self.manifest: dict = {}  # 当前项目 manifest（导入 .lommod 后生效）
        self.manifest_base: dict = {}  # 导入的原 manifest（campaign 往返用）
        self._story_paths: dict[str, Path | None] = {}
        self._loading = False
        # 撤销/重做：整项目快照式；连续输入暂停 600ms 合并为一步
        self._undo_stack: list[tuple[dict, str]] = []  # 撤销栈
        self._redo_stack: list[tuple[dict, str]] = []  # 重做栈
        self._pending_before: dict | None = None
        self._saved_snapshot: dict = self._snapshot()
        self._prev_snapshot: dict = self._snapshot()
        self._dirty = False
        self._prompt_on_discard = True  # 测试可关：有未保存修改时的确认弹窗

        self._build_ui()
        self._build_menu()
        self._build_toolbar()
        self.form.node_changed.connect(self._on_node_changed)
        self._refresh_all()

        # 状态栏：数据来源提示
        src = (
            "使用兜底数据（未找到 data/editor_data.json）"
            if is_fallback
            else "已加载 data/editor_data.json"
        )
        if not lomc_available():
            src += f"；lomc 不可用（{get_lomc()[1]}）"
        if not self.stage.has_assets():
            src += "；无预览素材，使用占位图"
        try:
            game_dir = self.game_manager.require_game_dir()
            if self.game_manager.bepinex_installed(game_dir):
                src += f"；已连接游戏：{game_dir.name}"
            else:
                src += "；已找到游戏，尚未安装 BepInEx"
        except GameInstallError:
            src += "；尚未连接游戏（可在“文件 → 安装管理”中设置）"
        self.statusBar().showMessage(src)

    # -------------------------------------------------------------- 项目模型
    @property
    def story(self) -> dict:
        """当前剧情脚本（self._stories 的引用，原地修改自动可见）。"""
        return self._stories[self._current_id]

    @story.setter
    def story(self, value: dict) -> None:
        if not isinstance(value, dict):
            raise ValueError("story 必须是 dict")
        sid = str(value.get("id") or "main")
        self._stories[sid] = value
        self._current_id = sid

    @property
    def story_path(self) -> Path | None:
        """当前剧情脚本对应的 story.json 路径。"""
        return self._story_paths.get(self._current_id)

    @story_path.setter
    def story_path(self, value: Path | None) -> None:
        self._story_paths[self._current_id] = value

    # UserRole：章节设置行存 "chapter"；步骤行存节点下标 int
    _ROLE_KIND = Qt.ItemDataRole.UserRole

    # ------------------------------------------------------------------ UI
    def _build_ui(self) -> None:
        splitter = QSplitter(Qt.Orientation.Horizontal)

        # 左栏：只管理结构——章节切换 + 步骤树 + 添加
        left = QWidget()
        left.setObjectName("leftNav")
        lv = QVBoxLayout(left)
        lv.setContentsMargins(8, 8, 8, 8)
        lv.setSpacing(6)

        head = QLabel(t("nav.story"))
        self._nav_head = head
        head_font = QFont(head.font())
        head_font.setBold(True)
        head.setFont(head_font)
        lv.addWidget(head)

        story_row = QHBoxLayout()
        story_row.setSpacing(4)
        self.story_combo = QComboBox()
        self.story_combo.setSizePolicy(
            QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed
        )
        self.story_combo.currentIndexChanged.connect(self._on_story_switched)
        story_row.addWidget(self.story_combo, stretch=1)
        self.add_story_btn = QToolButton()
        self.add_story_btn.setText("+")
        self.add_story_btn.setToolTip(t("nav.new_chapter"))
        self.add_story_btn.clicked.connect(self._add_story_in_project)
        self.story_more_btn = QToolButton()
        self.story_more_btn.setText("…")
        self.story_more_btn.setToolTip(t("nav.chapter_ops"))
        story_menu = QMenu(self.story_more_btn)
        story_menu.addAction(t("nav.chapter_settings"), self._select_chapter_settings)
        story_menu.addAction(t("nav.duplicate_chapter"), self._duplicate_story_in_project)
        story_menu.addSeparator()
        story_menu.addAction(t("nav.delete_chapter"), self._delete_story_in_project)
        self._story_menu = story_menu
        self.story_more_btn.setMenu(story_menu)
        self.story_more_btn.setPopupMode(QToolButton.ToolButtonPopupMode.InstantPopup)
        story_row.addWidget(self.add_story_btn)
        story_row.addWidget(self.story_more_btn)
        lv.addLayout(story_row)

        self.node_list = StepListWidget()
        self.node_list.setObjectName("stepTree")
        self.node_list.setUniformItemSizes(False)
        self.node_list.setSpacing(2)
        self.node_list.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.node_list.customContextMenuRequested.connect(self._on_node_context_menu)
        self.node_list.currentRowChanged.connect(self._on_node_selected)
        self.node_list.itemDoubleClicked.connect(self._toggle_structure_item)
        self.node_list.steps_moved.connect(self._on_steps_moved)
        self.node_list.setToolTip(t("nav.drag_tip"))
        rename_shortcut = QAction(self)
        rename_shortcut.setShortcut(QKeySequence("F2"))
        rename_shortcut.setShortcutContext(Qt.ShortcutContext.WidgetShortcut)
        rename_shortcut.triggered.connect(self._rename_current_node)
        self.node_list.addAction(rename_shortcut)
        lv.addWidget(self.node_list, stretch=1)

        add_btn = QPushButton(t("nav.add_step"))
        self._add_step_btn = add_btn
        add_menu = QMenu(add_btn)
        common = add_menu.addMenu(t("nav.common_steps"))
        for node_type in models.COMMON_NODE_TYPES:
            cn = models.NODE_TYPE_CN.get(node_type, node_type)
            common.addAction(cn, lambda checked=False, t=node_type: self._add_node(t))
        common.addSeparator()
        common.addAction(t("nav.ending_card"), self._add_ending_card)
        add_menu.addSeparator()
        for group_name, types in models.NODE_GROUPS:
            sub = add_menu.addMenu(group_name)
            for node_type in types:
                cn = models.NODE_TYPE_CN.get(node_type, node_type)
                action = sub.addAction(
                    cn, lambda checked=False, t=node_type: self._add_node(t)
                )
                action.setToolTip(t("nav.internal_type", type=node_type))
        add_btn.setMenu(add_menu)
        self._add_step_menu = add_menu
        add_btn.setMinimumHeight(32)
        lv.addWidget(add_btn)

        # 中栏：Inspector（章节设置 / 步骤属性）
        self.form = NodeForm()
        self.form.id_change_requested.connect(self._on_id_change_requested)
        self.chapter_panel = self._build_chapter_panel()
        self.inspector = QStackedWidget()
        self.inspector.addWidget(self.chapter_panel)  # 0
        self.inspector.addWidget(self.form)  # 1

        # 右栏：预览 / 流程图 / 编译
        self.preview = LuaPreview()
        self.stage = StagePreview()
        pmap, data_dir = load_preview_map(PROJECT_ROOT)
        self.stage.set_assets(pmap, data_dir)
        self.stage.set_context(self.editor_data)
        self.stage.choice_activated.connect(self._on_choice_goto)

        stage_tab = QWidget()
        sv = QVBoxLayout(stage_tab)
        sv.setContentsMargins(4, 4, 4, 4)
        sv.addWidget(self.stage, stretch=1)
        sv.addLayout(self._build_stage_toolbar())

        self.right_tabs = QTabWidget()
        self.right_tabs.addTab(stage_tab, t("tab.preview"))
        self.flow_graph = FlowGraphPanel()
        self.flow_graph.node_activated.connect(self._on_flow_node_activated)
        self.right_tabs.addTab(self.flow_graph, t("tab.flow"))
        self.right_tabs.addTab(self.preview, t("tab.compile"))
        self.right_tabs.currentChanged.connect(self._on_right_tab_changed)

        splitter.addWidget(left)
        splitter.addWidget(self.inspector)
        splitter.addWidget(self.right_tabs)
        splitter.setSizes([280, 420, 560])
        self.setCentralWidget(splitter)

        self._preview_timer = QTimer(self)
        self._preview_timer.setSingleShot(True)
        self._preview_timer.setInterval(300)
        self._preview_timer.timeout.connect(self._refresh_preview)

        self._graph_timer = QTimer(self)
        self._graph_timer.setSingleShot(True)
        self._graph_timer.setInterval(180)
        self._graph_timer.timeout.connect(self._refresh_flow_graph)

        self._commit_timer = QTimer(self)
        self._commit_timer.setSingleShot(True)
        self._commit_timer.setInterval(600)
        self._commit_timer.timeout.connect(self._flush_pending)

        self._auto_timer = QTimer(self)
        self._auto_timer.setInterval(1500)
        self._auto_timer.timeout.connect(self._auto_step)

    def _build_chapter_panel(self) -> QWidget:
        """中栏「章节设置」：编号 / 名称 / 起始步骤 / 心情气泡。"""
        panel = QWidget()
        root = QVBoxLayout(panel)
        root.setContentsMargins(12, 12, 12, 12)
        title = QLabel(t("chapter.title"))
        self._chapter_title = title
        tf = QFont(title.font())
        tf.setBold(True)
        tf.setPointSize(tf.pointSize() + 2)
        title.setFont(tf)
        root.addWidget(title)

        props = QFormLayout()
        props.setSpacing(10)
        self.story_id_edit = QLineEdit()
        self.story_title_edit = QLineEdit()
        self.start_combo = QComboBox()
        self.mood_check = QCheckBox(t("chapter.mood_check"))
        self.mood_check.setToolTip(t("chapter.mood_tip"))
        self.story_id_edit.textChanged.connect(self._on_story_props_changed)
        self.story_title_edit.textChanged.connect(self._on_story_props_changed)
        self.start_combo.currentTextChanged.connect(self._on_start_changed)
        self.mood_check.toggled.connect(self._on_story_mood_changed)
        self.story_id_edit.setPlaceholderText(t("chapter.id_placeholder"))
        self.story_title_edit.setPlaceholderText(t("chapter.name_placeholder"))
        props.addRow(t("chapter.id"), self.story_id_edit)
        props.addRow(t("chapter.name"), self.story_title_edit)
        props.addRow(t("chapter.start"), self.start_combo)
        props.addRow(t("chapter.mood"), self.mood_check)
        self._chapter_form = props
        root.addLayout(props)
        root.addStretch(1)
        return panel

    def _build_stage_toolbar(self) -> QHBoxLayout:
        """预览底栏：与主题一致的文字按钮（不用 Unicode 符号，避免变成彩色 emoji）。"""
        bar = QHBoxLayout()
        bar.setContentsMargins(0, 6, 0, 2)
        bar.setSpacing(8)
        bar.addStretch(1)

        def text_btn(label: str, tip: str) -> QPushButton:
            b = QPushButton(label)
            b.setToolTip(tip)
            b.setMinimumHeight(28)
            b.setMinimumWidth(56)
            return b

        home_btn = text_btn(t("stage.home"), t("stage.home_tip"))
        prev_btn = text_btn(t("stage.prev"), t("stage.prev_tip"))
        self.stage_pos_label = QLabel("– / –")
        self.stage_pos_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.stage_pos_label.setMinimumWidth(48)
        next_btn = text_btn(t("stage.next"), t("stage.next_tip"))
        self.auto_btn = QPushButton(t("stage.play"))
        self.auto_btn.setToolTip(t("stage.play_tip"))
        self.auto_btn.setCheckable(True)
        self.auto_btn.setMinimumHeight(28)
        self.auto_btn.setMinimumWidth(64)
        home_btn.clicked.connect(self._goto_start)
        prev_btn.clicked.connect(lambda: self._step_selection(-1))
        next_btn.clicked.connect(lambda: self._step_selection(1))
        self.auto_btn.toggled.connect(self._on_auto_toggled)
        for w in (home_btn, prev_btn, self.stage_pos_label, next_btn, self.auto_btn):
            bar.addWidget(w)
        bar.addStretch(1)
        return bar

    def _build_menu(self) -> None:
        self.menuBar().clear()
        menu = self.menuBar().addMenu(t("menu.file"))
        menu.addAction(t("menu.new"), self.new_story, QKeySequence.StandardKey.New)
        menu.addAction(
            t("menu.open"), self.open_story, QKeySequence.StandardKey.Open
        )
        self._recent_menu = menu.addMenu(t("menu.recent"))
        self._rebuild_recent_menu()
        menu.addAction(
            t("menu.save"), self.save_story, QKeySequence.StandardKey.Save
        )
        menu.addAction(
            t("menu.save_as"),
            self.save_story_as,
            QKeySequence.StandardKey.SaveAs,
        )
        menu.addSeparator()
        menu.addAction(t("menu.import_mod"), self.import_lommod)
        menu.addAction(t("menu.export_mod"), self.export_lommod)
        menu.addAction(t("menu.install"), self._show_mod_manager)
        menu.addAction(
            t("menu.content_library"),
            self._show_content_library,
            QKeySequence("Ctrl+L"),
        )
        menu.addSeparator()
        menu.addAction(t("menu.quit"), self.close)
        edit = self.menuBar().addMenu(t("menu.edit"))
        edit.addAction(t("menu.undo"), self._undo, QKeySequence.StandardKey.Undo)
        edit.addAction(t("menu.redo"), self._redo, QKeySequence.StandardKey.Redo)
        edit.addSeparator()
        edit.addAction(
            t("menu.global_search"), self._show_global_search,
            QKeySequence("Ctrl+Shift+F"),
        )
        edit.addAction(t("menu.bulk_edit"), self._show_bulk_edit)
        edit.addAction(t("menu.node_templates"), self._show_node_templates)
        edit.addAction(t("menu.story_sections"), self._show_story_sections)
        edit.addAction(t("menu.cross_story_transfer"), self._show_cross_story_transfer)
        edit.addAction(t("menu.variable_manager"), self._show_variable_manager)
        edit.addAction(t("menu.condition_inspector"), self._show_condition_inspector)
        edit.addAction(t("menu.story_localization"), self._show_story_localization)
        run_menu = self.menuBar().addMenu(t("menu.run"))
        run_menu.addAction(
            t("menu.play"),
            self.play_from_current_node,
            QKeySequence("F5"),
        )
        run_menu.addAction(
            t("menu.preflight"),
            self._check_project,
            QKeySequence("F6"),
        )
        run_menu.addAction(t("menu.flow"), self._show_flow_graph, QKeySequence("F7"))
        run_menu.addAction(t("menu.path_simulator"), self._show_path_simulator)
        run_menu.addAction(t("menu.story_tests"), self._show_story_tests)
        run_menu.addSeparator()
        run_menu.addAction(t("menu.reset_read"), self._reset_read_state)
        lang_menu = self.menuBar().addMenu(t("lang.menu"))
        for code, _native in LANGUAGES:
            action = lang_menu.addAction(t(f"lang.{code}"))
            action.setCheckable(True)
            action.setChecked(current_language() == code)
            action.triggered.connect(lambda checked=False, c=code: self._change_language(c))
        help_menu = self.menuBar().addMenu(t("menu.help"))
        help_menu.addAction(t("menu.help_item"), self._show_help, QKeySequence("F1"))
        help_menu.addAction(t("menu.diagnostic_bundle"), self._export_diagnostic_bundle)

    def _build_toolbar(self) -> None:
        """工具栏只留高频：试玩 + 导出。其余走菜单。"""
        for old in list(self.findChildren(QToolBar)):
            self.removeToolBar(old)
        bar = QToolBar(t("toolbar.name"), self)
        bar.setMovable(False)
        bar.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextOnly)

        play = QAction(t("toolbar.play"), self)
        play.setToolTip(t("toolbar.play_tip"))
        play.setShortcut(QKeySequence("F5"))
        play.triggered.connect(self.play_from_current_node)
        bar.addAction(play)

        library = QAction(t("toolbar.library"), self)
        library.setToolTip(t("toolbar.library_tip"))
        library.setShortcut(QKeySequence("Ctrl+L"))
        library.triggered.connect(self._show_content_library)
        bar.addAction(library)

        export_action = QAction(t("toolbar.export"), self)
        export_action.setToolTip(t("toolbar.export_tip"))
        export_action.triggered.connect(self.export_lommod)
        bar.addAction(export_action)

        self.addToolBar(bar)
        export_btn = bar.widgetForAction(export_action)
        if export_btn is not None:
            mark_primary(export_btn)

    def _change_language(self, code: str) -> None:
        if code == current_language():
            return
        set_language(code)
        models.refresh_labels()
        self.game_manager.save_pref("language", code)
        app = QApplication.instance()
        if app is not None:
            install_qt_translator(app)
            app.setApplicationName(_app_title())
        self._build_menu()
        self._build_toolbar()
        if hasattr(self, "_nav_head"):
            self._nav_head.setText(t("nav.story"))
        if hasattr(self, "_add_step_btn"):
            self._add_step_btn.setText(t("nav.add_step"))
            add_menu = QMenu(self._add_step_btn)
            common = add_menu.addMenu(t("nav.common_steps"))
            for node_type in models.COMMON_NODE_TYPES:
                cn = models.NODE_TYPE_CN.get(node_type, node_type)
                common.addAction(cn, lambda checked=False, nt=node_type: self._add_node(nt))
            common.addSeparator()
            common.addAction(t("nav.ending_card"), self._add_ending_card)
            add_menu.addSeparator()
            for group_name, types in models.NODE_GROUPS:
                sub = add_menu.addMenu(group_name)
                for node_type in types:
                    cn = models.NODE_TYPE_CN.get(node_type, node_type)
                    action = sub.addAction(
                        cn, lambda checked=False, nt=node_type: self._add_node(nt)
                    )
                    action.setToolTip(t("nav.internal_type", type=node_type))
            self._add_step_btn.setMenu(add_menu)
        if hasattr(self, "_chapter_title"):
            self._chapter_title.setText(t("chapter.title"))
        self.mood_check.setText(t("chapter.mood_check"))
        self.mood_check.setToolTip(t("chapter.mood_tip"))
        self.right_tabs.setTabText(0, t("tab.preview"))
        self.right_tabs.setTabText(1, t("tab.flow"))
        self.right_tabs.setTabText(2, t("tab.compile"))
        self.node_list.setToolTip(t("nav.drag_tip"))
        keep = self._selected_node_index()
        self._refresh_all(select_row=keep)
        self._update_title()
        self.statusBar().showMessage(t("lang.restart_hint"), 4000)

    def _show_help(self) -> None:
        HelpDialog(self, self.game_manager).exec()

    def _export_diagnostic_bundle(self) -> None:
        """Export only the fixed, privacy-sanitized diagnostic allowlist."""
        self._flush_pending()
        default_name = "lom_modkit_diagnostics_%s.zip" % datetime.now().strftime(
            "%Y%m%d_%H%M%S"
        )
        path, _ = QFileDialog.getSaveFileName(
            self,
            t("diagnostics.title"),
            str(Path(self._last_dir("last_diagnostic_dir")) / default_name),
            t("diagnostics.filter"),
        )
        if not path:
            return
        self._remember_dir("last_diagnostic_dir", path)
        manifest = dict(self.manifest_base or self.manifest or {})
        manifest.setdefault("entry", self._current_id)
        try:
            issues = self._preflight_issues()
        except Exception as exc:
            issues = [PreflightIssue(
                "error", "validation_crash", "", "",
                "F6 validation crashed while collecting diagnostics: " + str(exc),
            )]
        try:
            result = export_diagnostic_bundle(
                Path(path),
                self._stories,
                self.editor_data,
                manifest,
                issues,
                game_manager=self.game_manager,
                crash_log=CRASH_LOG,
            )
        except Exception as exc:
            QMessageBox.critical(
                self, t("diagnostics.title"),
                t("diagnostics.failed", error=str(exc)),
            )
            return
        QMessageBox.information(
            self, t("diagnostics.title"),
            t("diagnostics.done", path=str(result)),
        )

    def _show_mod_manager(self) -> None:
        ModManagerDialog(self.game_manager, self).exec()

    def _show_content_library(self) -> None:
        from content_library_dialog import ContentLibraryDialog

        ContentLibraryDialog(self._stories, self, self.editor_data).exec()
        self._load_form()

    def _show_global_search(self) -> None:
        from global_search import GlobalSearchDialog

        self._flush_pending()
        GlobalSearchDialog(
            self._stories, self._locate_search_result, self,
            manifest=self.manifest_base,
        ).exec()

    def _show_bulk_edit(self) -> None:
        from bulk_edit import BulkEditDialog, apply_bulk_edit

        self._flush_pending()
        dialog = BulkEditDialog(self.story, self.editor_data, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        indices = dialog.selected_indices()
        try:
            # Validate against a disposable copy before recording an undo point.
            # The real mutation below should therefore be unable to leave an
            # empty/stray undo entry when a malformed value is rejected.
            apply_bulk_edit(
                copy.deepcopy(self.story),
                indices,
                dialog.selected_field(),
                dialog.selected_value(),
            )
            self._record_discrete()
            count = apply_bulk_edit(
                self.story, indices, dialog.selected_field(), dialog.selected_value()
            )
        except ValueError as exc:
            QMessageBox.warning(self, t("bulk.title"), str(exc))
            return
        self._refresh_all(select_row=indices[0] if indices else 0)
        self.statusBar().showMessage(t("bulk.done", count=count), 3500)

    def _show_node_templates(self) -> None:
        from node_templates import NodeTemplateDialog, instantiate_template

        self._flush_pending()
        current = self._selected_node_index()
        dialog = NodeTemplateDialog(self.story, current, self.editor_data, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        template = dialog.chosen_template()
        if template is None:
            return
        nodes = self.story.get("nodes") or []
        insert_at = current + 1 if 0 <= current < len(nodes) else len(nodes)
        try:
            # Validate and allocate on a copy before the undo checkpoint.
            instantiate_template(copy.deepcopy(self.story), template, insert_at)
            self._record_discrete()
            first, count, _mapping = instantiate_template(self.story, template, insert_at)
        except ValueError as exc:
            QMessageBox.warning(self, t("template.title"), str(exc))
            return
        self._refresh_all(select_row=first)
        self.statusBar().showMessage(
            t("template.inserted", name=template.get("name", ""), count=count), 4000
        )

    def _show_story_sections(self) -> None:
        from story_sections import StorySectionsDialog, get_sections, set_sections

        self._flush_pending()
        dialog = StorySectionsDialog(self.story, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        updated = copy.deepcopy(get_sections(dialog.story))
        if updated == get_sections(self.story):
            return
        selected = self._selected_node_index()
        self._record_discrete()
        set_sections(self.story, updated)
        self._refresh_all(select_row=selected)
        self.statusBar().showMessage(t("sections.updated"), 3000)

    def _show_cross_story_transfer(self) -> None:
        from cross_story_transfer import CrossStoryTransferDialog, copy_nodes_between_stories

        self._flush_pending()
        if len(self._stories) < 2:
            QMessageBox.information(self, t("transfer.title"), t("transfer.need_two"))
            return
        dialog = CrossStoryTransferDialog(self._stories, self._current_id, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        params = dialog.parameters()
        try:
            copy_nodes_between_stories(copy.deepcopy(self._stories), *params)
            self._record_discrete()
            result = copy_nodes_between_stories(self._stories, *params)
        except ValueError as exc:
            QMessageBox.warning(self, t("transfer.title"), str(exc))
            return
        self._current_id = result.target_story
        self._refresh_all(select_row=result.first_index)
        self.statusBar().showMessage(
            t("transfer.done", count=result.count, story=result.target_story), 4000
        )
        if result.warnings:
            QMessageBox.warning(
                self,
                t("transfer.warning_title"),
                t("transfer.warning_intro") + "\n\n" + "\n".join("• " + warning for warning in result.warnings),
            )

    def _show_variable_manager(self) -> None:
        from variable_manager import VariableManagerDialog

        self._flush_pending()
        VariableManagerDialog(
            self._stories, self._locate_search_result, self,
            manifest=self.manifest_base,
        ).exec()

    def _show_condition_inspector(self) -> None:
        from condition_inspector import ConditionInspectorDialog

        self._flush_pending()
        ConditionInspectorDialog(
            self._stories, self._locate_search_result, self,
            manifest=self.manifest_base,
        ).exec()

    def _show_path_simulator(self) -> None:
        from path_simulator import PathSimulatorDialog

        self._flush_pending()
        PathSimulatorDialog(
            self._stories, self._locate_search_result, self,
            manifest=self.manifest_base,
        ).exec()

    def _show_story_tests(self) -> None:
        from story_test_runner import StoryTestRunnerDialog, get_story_tests, set_story_tests

        self._flush_pending()
        dialog = StoryTestRunnerDialog(self._stories, self._current_id, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        tests = dialog.saved_tests()
        if tests is None or tests == get_story_tests(self.story):
            return
        self._record_discrete()
        set_story_tests(self.story, tests)
        self._refresh_all(select_row=self._selected_node_index())
        self.statusBar().showMessage(t("tests.saved", count=len(tests)), 3000)

    def _show_story_localization(self) -> None:
        dialog = StoryLocalizationDialog(self.story, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        config = dialog.result_config()
        before = copy.deepcopy(self.story.get("localization"))
        if before == config or ("localization" not in self.story and config is None):
            return
        self._record_discrete()
        apply_localization_settings(self.story, config)
        self._refresh_all(select_row=max(0, self._selected_node_index()))
        self.statusBar().showMessage(t("localization.saved"), 3000)

    def _locate_search_result(self, story_id: str, node_id: str | None) -> None:
        if story_id not in self._stories:
            return
        self._current_id = story_id
        row = 0
        if node_id:
            for index, node in enumerate(self.story.get("nodes", [])):
                if node.get("id") == node_id:
                    row = index
                    break
        self._refresh_all(select_row=row)
        if node_id is None:
            self._select_chapter_settings()
        self.node_list.setFocus()
        self.statusBar().showMessage(
            t("search.located", story=story_id, node=node_id or t("chapter.title")),
            3500,
        )

    def _show_flow_graph(self) -> None:
        self._refresh_flow_graph()
        self.right_tabs.setCurrentWidget(self.flow_graph)

    def _refresh_flow_graph(self) -> None:
        self.flow_graph.set_story(self.story, self.editor_data)

    def _on_flow_node_activated(self, node_id: str) -> None:
        row = self._node_row(node_id)
        if row < 0:
            return
        self._select_node_index(row)
        self.statusBar().showMessage(f"已定位到步骤 {node_id}", 2500)

    def _check_project(self) -> bool:
        """打开可定位、可保守修复的体检报告。"""
        self._flush_pending()
        issues = self._preflight_issues()
        dialog = PreflightDialog(
            issues,
            self._locate_preflight_issue,
            self._apply_preflight_fixes,
            self,
        )
        dialog.exec()
        remaining_errors = sum(issue.severity == "error" for issue in dialog.issues)
        if remaining_errors:
            self.statusBar().showMessage(
                t("preflight.done_errors", n=remaining_errors), 5000
            )
            return False
        self.statusBar().showMessage(t("preflight.passed"), 5000)
        return True

    def _preflight_issues(self) -> list[PreflightIssue]:
        entry = self.manifest_base.get("entry") or self.manifest.get("entry")
        if not entry:
            entry = self._current_id
        effective_manifest = dict(self.manifest_base or self.manifest or {})
        effective_manifest["entry"] = entry
        return run_preflight(
            self._stories,
            self.editor_data,
            str(entry),
            manifest=effective_manifest,
            content_root=content_registry.repository_root(),
        )

    def _locate_preflight_issue(self, issue: PreflightIssue) -> None:
        if issue.story_id not in self._stories:
            return
        self._current_id = issue.story_id
        row = 0
        if issue.node_id:
            for index, node in enumerate(self.story.get("nodes", [])):
                if node.get("id") == issue.node_id:
                    row = index
                    break
            self._refresh_all(select_row=row)
        else:
            self._refresh_all(select_row=0)
            self._select_chapter_settings()
        self.node_list.setFocus()
        self.statusBar().showMessage(
            t(
                "preflight.located",
                story=issue.story_id,
                node=issue.node_id or t("preflight.chapter_settings"),
            ),
            5000,
        )

    def _apply_preflight_fixes(self) -> tuple[list[str], list[PreflightIssue]]:
        before = self._snapshot()
        fixes = apply_safe_fixes(self._stories, self.editor_data)
        if fixes:
            self._flush_pending()
            self._undo_stack.append((before, self._current_id))
            self._trim_undo()
            self._redo_stack.clear()
            self._refresh_all(select_row=max(0, self.node_list.currentRow()))
            self._set_dirty(True)
        return fixes, self._preflight_issues()

    # -------------------------------------------------------------- 刷新
    def _refresh_all(self, select_row: int = 0) -> None:
        """story/项目变更后整体刷新（脚本切换条 + 三栏 + 标题/脏标记/撤销基线）。"""
        self._loading = True
        try:
            self._refresh_story_combo()
            self.story_id_edit.setText(self.story.get("id", ""))
            self.story_title_edit.setText(self.story.get("title", ""))
            self.mood_check.setChecked(bool(self.story.get("mood", False)))
            self._reload_start_combo()
            self._reload_node_list(select_row)
        finally:
            self._loading = False
        self._load_form()
        self._refresh_stage()
        self._refresh_flow_graph()
        self._schedule_preview()
        self._prev_snapshot = self._snapshot()
        self._set_dirty(self._stories != self._saved_snapshot)

    def _refresh_story_combo(self) -> None:
        """脚本切换下拉框：列出项目内全部剧情脚本 id（带标题）。"""
        self.story_combo.blockSignals(True)
        try:
            self.story_combo.clear()
            for sid in sorted(self._stories):
                title = str(self._stories[sid].get("title") or "")
                disp = f"{sid} — {title}" if title and title != sid else sid
                self.story_combo.addItem(disp, sid)
            idx = self.story_combo.findData(self._current_id)
            self.story_combo.setCurrentIndex(max(0, idx))
        finally:
            self.story_combo.blockSignals(False)

    def _on_story_switched(self, _index: int) -> None:
        if self._loading:
            return
        sid = self.story_combo.currentData()
        if not sid or sid == self._current_id:
            return
        self._current_id = sid
        self._remember_current_chapter()
        self._refresh_all()
        self.statusBar().showMessage(f"已切换到剧情章节 {sid}", 2000)

    def _add_story_in_project(self) -> None:
        """项目内新建剧情脚本（多剧情），并切换到它。"""
        sid = models.make_story_id(self._stories)
        self._record_discrete()
        self._stories[sid] = new_editor_story(sid, self.editor_data)
        self._current_id = sid
        self._refresh_all()
        self._select_chapter_settings()
        self.statusBar().showMessage(f"已新建剧情章节 {sid}", 3000)

    def _duplicate_story_in_project(self) -> None:
        """复制当前章节为新脚本 id。"""
        self._record_discrete()
        sid = models.make_story_id(self._stories)
        clone = copy.deepcopy(self.story)
        clone["id"] = sid
        title = str(clone.get("title") or sid)
        clone["title"] = f"{title}（副本）"
        self._stories[sid] = clone
        self._current_id = sid
        self._refresh_all()
        self._select_chapter_settings()
        self.statusBar().showMessage(f"已复制章节为 {sid}", 3000)

    def _delete_story_in_project(self) -> None:
        """从项目删除当前剧情脚本（可撤销）。"""
        if len(self._stories) <= 1:
            QMessageBox.warning(self, _app_title(), "项目中至少要保留一个剧情章节")
            return
        if self._prompt_on_discard:
            answer = QMessageBox.question(
                self,
                _app_title(),
                f"确定删除章节「{self._current_id}」？可撤销。",
            )
            if answer != QMessageBox.StandardButton.Yes:
                return
        self._record_discrete()
        sid = self._current_id
        del self._stories[sid]
        self._current_id = next(iter(sorted(self._stories)))
        self._refresh_all()
        self.statusBar().showMessage(f"已删除剧情章节 {sid}（可撤销）", 3000)

    def _reload_start_combo(self) -> None:
        self.start_combo.clear()
        for n in self.story.get("nodes", []):
            self.start_combo.addItem(n.get("id", ""))
        idx = self.start_combo.findText(self.story.get("start", ""))
        self.start_combo.setCurrentIndex(max(0, idx))

    def _is_chapter_item(self, item: QListWidgetItem | None) -> bool:
        return item is not None and item.data(self._ROLE_KIND) == "chapter"

    def _is_structure_item(self, item: QListWidgetItem | None) -> bool:
        data = item.data(self._ROLE_KIND) if item is not None else None
        return isinstance(data, tuple) and len(data) >= 3 and data[0] == "structure"

    def _selected_node_index(self) -> int:
        """当前选中的步骤在 nodes[] 中的下标；选中章节设置时返回 -1。"""
        item = self.node_list.currentItem()
        if item is None or self._is_chapter_item(item):
            return -1
        data = item.data(self._ROLE_KIND)
        return int(data) if isinstance(data, int) else -1

    def _list_row_for_node_index(self, node_index: int) -> int:
        """nodes[] 下标 → 当前树列表行（折叠时可能不可见）。"""
        for row in range(self.node_list.count()):
            if self.node_list.item(row).data(self._ROLE_KIND) == node_index:
                return row
        return -1

    def _select_node_index(self, node_index: int) -> None:
        from story_sections import expand_for_node

        if expand_for_node(self.story, node_index):
            self._reload_node_list(select_row=node_index)
            return
        row = self._list_row_for_node_index(node_index)
        if 0 <= row < self.node_list.count():
            self.node_list.setCurrentRow(row)
            item = self.node_list.item(row)
            if item is not None:
                self.node_list.scrollToItem(item)

    def _select_chapter_settings(self) -> None:
        if self.node_list.count() > 0:
            self.node_list.setCurrentRow(0)

    def _reload_node_list(self, select_row: int = -1) -> None:
        """重建步骤树。select_row 为 nodes[] 下标；-1 时尽量保留当前选中。

        第 0 行固定为「章节设置」；步骤从第 1 行起。
        """
        from story_sections import expand_for_node, structure_rows

        prev_item = self.node_list.currentItem()
        prev_kind = "chapter" if self._is_chapter_item(prev_item) else "node"
        prev_structure = prev_item.data(self._ROLE_KIND) if self._is_structure_item(prev_item) else None
        prev_node = self._selected_node_index()
        if select_row >= 0:
            expand_for_node(self.story, select_row)
        self.node_list.blockSignals(True)
        self.node_list.clear()

        chapter = QListWidgetItem("  " + t("chapter.title"))
        chapter.setData(self._ROLE_KIND, "chapter")
        chapter.setToolTip(t("nav.chapter_tip"))
        chapter.setFlags(
            Qt.ItemFlag.ItemIsEnabled | Qt.ItemFlag.ItemIsSelectable
        )
        chapter.setSizeHint(QSize(0, 28))
        self.node_list.addItem(chapter)

        nodes = self.story.get("nodes", [])
        for row in structure_rows(self.story):
            if row.kind != "node":
                marker = "▸" if row.collapsed else "▾"
                kind_label = t("sections.section") if row.kind == "section" else t("sections.group")
                indent = "    " * row.depth
                item = QListWidgetItem(f"{indent}{marker} {kind_label} · {row.title}")
                item.setData(self._ROLE_KIND, ("structure", row.kind, row.item_id, row.start, row.end))
                item.setToolTip(t("sections.header_tip"))
                item.setFlags(Qt.ItemFlag.ItemIsEnabled | Qt.ItemFlag.ItemIsSelectable)
                item.setSizeHint(QSize(0, 30))
                self.node_list.addItem(item)
                continue
            index = row.node_index
            n = nodes[index]
            bullet = models.node_bullet(n.get("type", ""))
            nid = n.get("id", "")
            title, detail = models.node_list_caption(n, self.editor_data)
            indent = "    " * row.depth
            item = QListWidgetItem(f"{indent}{bullet} {nid}  {title}\n{indent}      {detail}")
            item.setData(self._ROLE_KIND, index)
            item.setToolTip(models.node_summary(n, self.editor_data))
            item.setFlags(
                Qt.ItemFlag.ItemIsEnabled
                | Qt.ItemFlag.ItemIsSelectable
                | Qt.ItemFlag.ItemIsDragEnabled
            )
            item.setSizeHint(QSize(0, 44))
            self.node_list.addItem(item)

        # 恢复选中：优先显式 select_row（节点下标）；否则保持章节/节点选择
        if select_row >= 0 and self.story.get("nodes"):
            target = min(select_row, len(self.story["nodes"]) - 1)
            self.node_list.setCurrentRow(self._list_row_for_node_index(target))
        elif prev_kind == "chapter" or not self.story.get("nodes"):
            self.node_list.setCurrentRow(0)
        elif prev_structure is not None:
            target_row = next(
                (row for row in range(self.node_list.count())
                 if self.node_list.item(row).data(self._ROLE_KIND) == prev_structure),
                0,
            )
            self.node_list.setCurrentRow(target_row)
        elif prev_node >= 0:
            target = min(prev_node, len(self.story.get("nodes", [])) - 1)
            self.node_list.setCurrentRow(self._list_row_for_node_index(target))
        elif self.node_list.count() > 1:
            self.node_list.setCurrentRow(1)
        else:
            self.node_list.setCurrentRow(0)
        self.node_list.blockSignals(False)
        self._update_stage_pos_label()

    def _update_stage_pos_label(self) -> None:
        """预览底栏「当前 / 总数」。"""
        label = getattr(self, "stage_pos_label", None)
        if label is None:
            return
        total = len(self.story.get("nodes", []))
        idx = self._selected_node_index()
        if total <= 0:
            label.setText("– / –")
        elif idx < 0:
            label.setText(f"– / {total}")
        else:
            label.setText(f"{idx + 1} / {total}")

    def _current_node(self) -> dict | None:
        idx = self._selected_node_index()
        nodes = self.story.get("nodes", [])
        return nodes[idx] if 0 <= idx < len(nodes) else None

    def _load_form(self) -> None:
        if self._is_chapter_item(self.node_list.currentItem()):
            self.inspector.setCurrentWidget(self.chapter_panel)
            self._update_stage_pos_label()
            return
        self.inspector.setCurrentWidget(self.form)
        node_ids = [n.get("id", "") for n in self.story.get("nodes", [])]
        self.form.set_context(self.editor_data, node_ids, sorted(self._stories.keys()))
        self.form.set_node(self._current_node())

    def _schedule_preview(self) -> None:
        self._preview_timer.start()

    def _on_right_tab_changed(self, _index: int) -> None:
        """切到 Lua 页立即编译，避免防抖定时器尚未触发时看到空白预览。"""
        if self.right_tabs.currentWidget() is not self.preview:
            return
        self._preview_timer.stop()
        self._refresh_preview()

    def _refresh_preview(self) -> None:
        try:
            lua, err = compile_story(self.story)
            if err is not None or lua is None:
                self.preview.show_error(err or "编译失败：无输出")
            else:
                self.preview.show_lua(lua)
        except Exception:
            # 定时器槽抛异常在 GUI/冻结版里通常只表现为“预览空白”；把完整错误
            # 留在预览框中，用户和打包自检都能直接看到真正原因。
            self.preview.show_error(traceback.format_exc(limit=8))

    def _refresh_stage(self) -> None:
        """演出预览：按当前选中节点重新推演舞台状态并重绘。"""
        node = self._current_node()
        story_path = self.story_path
        self.stage.set_story_root(
            story_path.parent.parent if story_path is not None else None
        )
        self.stage.show_node(self.story, node.get("id") if node else None)

    # ---------------------------------------------------------- 演出预览步进
    def _node_row(self, node_id: str) -> int:
        for i, n in enumerate(self.story.get("nodes", [])):
            if n.get("id") == node_id:
                return i
        return -1

    def _goto_start(self) -> None:
        """回到开头：选中起始节点。"""
        row = self._node_row(self.story.get("start", ""))
        if row >= 0:
            self._select_node_index(row)
        elif self.node_list.count() > 1:
            self._select_node_index(0)

    def _step_selection(self, delta: int) -> None:
        """上一步/下一步：按节点数组序移动选中（跳过章节设置行）。"""
        idx = self._selected_node_index()
        nodes = self.story.get("nodes", [])
        if not nodes:
            return
        if idx < 0:
            idx = 0 if delta > 0 else len(nodes) - 1
        else:
            idx = idx + delta
        if 0 <= idx < len(nodes):
            self._select_node_index(idx)

    def _on_auto_toggled(self, checked: bool) -> None:
        if checked:
            self._auto_timer.start()
        else:
            self._auto_timer.stop()

    def _stop_auto(self) -> None:
        if self.auto_btn.isChecked():
            self.auto_btn.setChecked(False)  # toggled 信号会停掉 timer
        else:
            self._auto_timer.stop()

    def _auto_step(self) -> None:
        """自动播放步进：按数组序前进；遇 choice/branch 或到底时暂停。

        QTimer 回调里任何异常都不能外抛（pythonw 下无声崩溃风险），
        记日志并停止自动播放。
        """
        try:
            idx = self._selected_node_index()
            nodes = self.story.get("nodes", [])
            if idx < 0:
                if not nodes:
                    self._stop_auto()
                    return
                self._select_node_index(0)
                idx = 0
            if idx + 1 >= len(nodes):
                self._stop_auto()
                self.statusBar().showMessage("自动播放：已到末尾，暂停", 3000)
                return
            self._select_node_index(idx + 1)
            node = self._current_node()
            if node and node.get("type") in ("choice", "branch", "dice"):
                self._stop_auto()
                self.statusBar().showMessage(
                    f"自动播放：遇到 {node['type']} 节点 {node.get('id')}，已暂停", 3000
                )
        except Exception:
            log_crash("自动播放步进异常：\n" + traceback.format_exc())
            self._stop_auto()

    def _on_choice_goto(self, node_id: str) -> None:
        """演出预览里点击选项按钮：跳转到对应 goto 节点（交互式步进）。"""
        try:
            self._stop_auto()
            row = self._node_row(node_id)
            if row >= 0:
                self._select_node_index(row)
            else:
                self.statusBar().showMessage(f"选项 goto 目标不存在：{node_id}", 3000)
        except Exception:
            log_crash(f"选项跳转异常（{node_id!r}）：\n" + traceback.format_exc())

    # ------------------------------------------------------ 撤销/重做/脏标记
    def _snapshot(self) -> dict:
        return copy.deepcopy(self._stories)

    def _trim_undo(self) -> None:
        while len(self._undo_stack) > UNDO_LIMIT:
            self._undo_stack.pop(0)

    def _flush_pending(self) -> None:
        """提交进行中的连续编辑（暂停超时/下一步操作前），压入撤销栈。"""
        if self._pending_before is not None:
            self._undo_stack.append((self._pending_before, self._current_id))
            self._trim_undo()
            self._pending_before = None
            # 提交点之后，当前状态成为下一段编辑的撤销基线
            self._prev_snapshot = self._snapshot()
        self._commit_timer.stop()

    def _record_continuous(self) -> None:
        """连续编辑（表单打字等）：首击时定格编辑前状态，暂停后合并为一步。"""
        if self._loading:
            return
        if self._pending_before is None:
            self._pending_before = self._prev_snapshot
            self._redo_stack.clear()
        self._commit_timer.start()
        self._set_dirty(True)

    def _record_discrete(self) -> None:
        """原子操作（增删移节点/改起始点/剧情增删）前调用：当前状态入撤销栈。"""
        if self._loading:
            return
        self._flush_pending()
        self._undo_stack.append((self._snapshot(), self._current_id))
        self._trim_undo()
        self._redo_stack.clear()
        self._set_dirty(True)

    def _restore(self, stories: dict, current_id: str) -> None:
        row = self.node_list.currentRow()
        self._stories = stories
        if current_id not in self._stories:
            current_id = next(iter(sorted(self._stories)))
        self._current_id = current_id
        self._refresh_all(select_row=max(0, row))

    def _undo(self) -> None:
        self._flush_pending()
        if not self._undo_stack:
            self.statusBar().showMessage("没有可撤销的操作", 2000)
            return
        stories, current_id = self._undo_stack.pop()
        self._redo_stack.append((self._snapshot(), self._current_id))
        self._restore(stories, current_id)
        self.statusBar().showMessage("已撤销", 2000)

    def _redo(self) -> None:
        self._flush_pending()
        if not self._redo_stack:
            self.statusBar().showMessage("没有可重做的操作", 2000)
            return
        stories, current_id = self._redo_stack.pop()
        self._undo_stack.append((self._snapshot(), self._current_id))
        self._restore(stories, current_id)
        self.statusBar().showMessage("已重做", 2000)

    def _set_dirty(self, dirty: bool) -> None:
        self._dirty = bool(dirty)
        self._update_title()

    def _update_title(self) -> None:
        name = (
            self.manifest.get("name")
            or self.manifest.get("id")
            or self._current_id
            or "未命名"
        )
        star = " *" if self._dirty else ""
        self.setWindowTitle(f"{_app_title()} — {name}{star}")

    def _mark_saved(self) -> None:
        """当前剧情脚本已保存：仅更新该脚本的基线（多剧情各自独立）。"""
        self._saved_snapshot[self._current_id] = copy.deepcopy(self.story)
        for key in list(self._saved_snapshot):
            if key not in self._stories:
                del self._saved_snapshot[key]
        self._set_dirty(self._stories != self._saved_snapshot)

    def _confirm_discard(self) -> bool:
        """有未保存修改时询问处理方式；返回 True 表示可以继续（保存成功或放弃）。"""
        if not self._dirty or not self._prompt_on_discard:
            return True
        box = QMessageBox(self)
        box.setWindowTitle(_app_title())
        box.setText(t("discard.title"))
        box.setInformativeText("导出 Mod 会保存全部章节；也可以放弃这次修改。")
        save_btn = box.addButton(t("discard.save"), QMessageBox.ButtonRole.AcceptRole)
        discard_btn = box.addButton(t("discard.discard"), QMessageBox.ButtonRole.DestructiveRole)
        box.addButton(t("discard.cancel"), QMessageBox.ButtonRole.RejectRole)
        box.setDefaultButton(save_btn)
        box.exec()
        clicked = box.clickedButton()
        box.deleteLater()  # 避免重复弹窗残留（findChild 会命中旧实例）
        if clicked is save_btn:
            return self.export_lommod()
        return clicked is discard_btn

    def closeEvent(self, event) -> None:
        if self._confirm_discard():
            self._commit_timer.stop()
            event.accept()
        else:
            event.ignore()

    # -------------------------------------------------------------- 节点操作
    def _add_node(self, node_type: str) -> None:
        node = models.new_node(
            node_type, models.make_node_id(self.story, node_type), self.editor_data
        )
        self._insert_node(node)

    def _add_ending_card(self) -> None:
        """新手预设：原版 EndGamePanel 汗青书样式，确认后回标题画面。"""
        node = models.new_node(
            "goto_scene", models.make_node_id(self.story, "goto_scene"), self.editor_data
        )
        node.update(
            {
                "scene": "End",
                "title": "新的结局",
                "desc": "在这里填写这段故事最后留下的文字。",
            }
        )
        self._insert_node(node, "已添加汗青书结局；请填写标题和正文")

    def _insert_node(self, node: dict, message: str | None = None) -> None:
        self._record_discrete()
        nodes = self.story.setdefault("nodes", [])
        row = self._selected_node_index()
        at = row + 1 if 0 <= row < len(nodes) else len(nodes)
        nodes.insert(at, node)
        # 登场防线：动作人物在前面未登场/已退场时，自动在它前面补一个人物登场，
        # 否则游戏会因“角色不存在”崩掉剧情协程（黑屏）。
        auto_show = None
        cid = stage_guard.missing_stage_linear(nodes, at)
        if cid:
            auto_show = stage_guard.make_show_node(self.story, cid)
            nodes.insert(at, auto_show)
            at += 1
        self._refresh_all(select_row=at)
        type_cn = models.NODE_TYPE_CN.get(node.get("type", ""), node.get("type", ""))
        if message is None:
            message = f"已添加 {type_cn}（{node.get('id', '')}）"
            if auto_show is not None:
                cname = models.character_name(self.editor_data, auto_show["character"])
                message += f"；已自动在前面补 {cname} 的登场"
        self.statusBar().showMessage(message, 3000)

    def _delete_node(self) -> None:
        nodes = self.story.get("nodes", [])
        row = self._selected_node_index()
        if not (0 <= row < len(nodes)):
            return
        if len(nodes) <= 1:
            QMessageBox.warning(self, _app_title(), "至少保留一个节点")
            return
        self._record_discrete()
        from story_sections import repair_after_delete
        repair_after_delete(self.story, str(nodes[row].get("id") or ""), nodes)
        removed = nodes.pop(row)
        if self.story.get("start") == removed.get("id"):
            self.story["start"] = nodes[0].get("id", "")
        self._refresh_all(select_row=min(row, len(nodes) - 1))

    def _move_node(self, delta: int) -> None:
        nodes = self.story.get("nodes", [])
        row = self._selected_node_index()
        to = row + delta
        if not (0 <= row < len(nodes) and 0 <= to < len(nodes)):
            return
        self._record_discrete()
        nodes[row], nodes[to] = nodes[to], nodes[row]
        self._refresh_all(select_row=to)

    def _on_steps_moved(self, from_index: int, insert_index: int) -> None:
        nodes = self.story.get("nodes", [])
        if not nodes:
            return
        if not (0 <= from_index < len(nodes)):
            self._reload_node_list(-1)
            self._load_form()
            return
        if insert_index == from_index or insert_index == from_index + 1:
            self._reload_node_list(from_index)
            self._load_form()
            return
        self._record_discrete()
        dest = models.reorder_node(self.story, from_index, insert_index)
        self._refresh_all(select_row=dest)
        self.statusBar().showMessage(t("nav.moved", n=dest + 1), 2500)

    def _rename_current_node(self) -> None:
        node = self._current_node()
        if node is None:
            return
        old_id = str(node.get("id") or "")
        new_id, ok = QInputDialog.getText(
            self,
            t("nav.rename_title"),
            t("nav.rename_prompt"),
            QLineEdit.EchoMode.Normal,
            old_id,
        )
        if not ok:
            return
        self._apply_node_rename(old_id, new_id)

    def _on_id_change_requested(self, new_id: str) -> None:
        node = self._current_node()
        if node is None:
            return
        self._apply_node_rename(str(node.get("id") or ""), new_id)

    def _apply_node_rename(self, old_id: str, new_id: str) -> None:
        new_id = (new_id or "").strip()
        if not old_id or new_id == old_id:
            return
        try:
            self._record_discrete()
            changed = models.rename_node(self.story, old_id, new_id)
            from story_sections import retarget_structure_ids
            changed += retarget_structure_ids(self.story, {old_id: new_id})
        except ValueError as exc:
            QMessageBox.warning(self, t("nav.rename_title"), str(exc))
            self._load_form()
            return
        idx = self._selected_node_index()
        self._refresh_all(select_row=idx)
        self.statusBar().showMessage(
            t("nav.rename_ok", old=old_id, new=new_id, n=changed), 4000
        )

    def _copy_node(self) -> None:
        """复制当前步骤并插入到其后。"""
        node = self._current_node()
        if node is None:
            return
        clone = copy.deepcopy(node)
        clone["id"] = models.make_node_id(self.story, str(clone.get("type") or "n"))
        self._insert_node(clone, f"已复制步骤为 {clone['id']}")

    def _on_node_context_menu(self, pos) -> None:
        item = self.node_list.itemAt(pos)
        if item is None or self._is_chapter_item(item):
            return
        self.node_list.setCurrentItem(item)
        if self._is_structure_item(item):
            data = item.data(self._ROLE_KIND)
            menu = QMenu(self)
            menu.addAction(t("sections.toggle"), lambda: self._toggle_structure(data[2]))
            menu.addAction(t("sections.expand_all"), lambda: self._set_all_structures(False))
            menu.addAction(t("sections.collapse_all"), lambda: self._set_all_structures(True))
            menu.addSeparator()
            menu.addAction(t("sections.manage"), self._show_story_sections)
            menu.exec(self.node_list.mapToGlobal(pos))
            return
        menu = QMenu(self)
        menu.addAction(t("nav.rename"), self._rename_current_node)
        menu.addAction(t("nav.copy"), self._copy_node)
        menu.addAction(t("nav.delete"), self._delete_node)
        menu.addSeparator()
        menu.addAction(t("nav.move_up"), lambda: self._move_node(-1))
        menu.addAction(t("nav.move_down"), lambda: self._move_node(1))
        menu.exec(self.node_list.mapToGlobal(pos))

    def _toggle_structure_item(self, item: QListWidgetItem) -> None:
        if not self._is_structure_item(item):
            return
        data = item.data(self._ROLE_KIND)
        self._toggle_structure(str(data[2]))

    def _toggle_structure(self, item_id: str) -> None:
        from story_sections import get_sections, set_collapsed

        current = next(
            (bool(item.get("collapsed"))
             for section in get_sections(self.story)
             for item in [section] + list(section.get("groups") or [])
             if item.get("id") == item_id),
            False,
        )
        self._record_discrete()
        set_collapsed(self.story, item_id, not current)
        self._reload_node_list()

    def _set_all_structures(self, collapsed: bool) -> None:
        from story_sections import set_all_collapsed

        if not set_all_collapsed(copy.deepcopy(self.story), collapsed):
            return
        self._record_discrete()
        set_all_collapsed(self.story, collapsed)
        self._reload_node_list()

    # -------------------------------------------------------------- 信号槽
    def _on_node_selected(self, _row: int) -> None:
        if not self._loading:
            self._load_form()
            self._refresh_stage()
            self._update_stage_pos_label()
            self._schedule_preview()

    def _on_story_props_changed(self) -> None:
        """脚本 id/标题编辑：写回 story（id 变化时同步项目键），记录撤销点。"""
        if self._loading:
            return
        new_id = self.story_id_edit.text().strip() or "main"
        new_title = self.story_title_edit.text()
        old_id = self.story.get("id", "")
        if new_id == old_id and new_title == self.story.get("title", ""):
            return
        if new_id != old_id and (
            not models.ID_PATTERN.fullmatch(new_id)
            or (new_id in self._stories and self._stories[new_id] is not self.story)
        ):
            # 非法 id 或已被其它剧情占用：回退文本
            self._loading = True
            try:
                self.story_id_edit.setText(old_id)
            finally:
                self._loading = False
            self.statusBar().showMessage(
                f"脚本 id {new_id!r} 不可用（格式 [a-zA-Z0-9_-]+ 且包内唯一）", 3000
            )
            return
        self._record_continuous()
        cur_story = self.story  # 局部引用：换键期间不经过 property（避免读到已删键）
        cur_story["id"] = new_id
        cur_story["title"] = new_title
        if new_id != old_id:
            del self._stories[old_id]
            self._stories[new_id] = cur_story
            self._current_id = new_id
            self._refresh_story_combo()
        self._schedule_preview()

    def _on_start_changed(self, text: str) -> None:
        if self._loading or not text or text == self.story.get("start"):
            return
        self._record_discrete()
        self.story["start"] = text
        self._prev_snapshot = self._snapshot()
        self._refresh_stage()  # 推演起点变了
        self._graph_timer.start()
        self._schedule_preview()

    def _on_story_mood_changed(self, checked: bool) -> None:
        """心情气泡开关：写回 story["mood"]（bool），记录撤销点并刷新 Lua 预览。"""
        if self._loading:
            return
        if bool(checked) == bool(self.story.get("mood", False)):
            return
        self._record_continuous()
        self.story["mood"] = bool(checked)
        self._schedule_preview()

    def _on_node_changed(self) -> None:
        """表单编辑后：记录撤销点 + 刷新列表摘要 + 预览。"""
        if self._loading:
            return
        self._record_continuous()
        self._reload_node_list()
        self._reload_start_combo_keep()
        self._refresh_stage()
        self._graph_timer.start()
        self._schedule_preview()

    # -------------------------------------------------------------- 一键试玩
    def play_from_current_node(self) -> bool:
        """临时打包、安装并请求运行时从当前步骤进入游戏。"""
        self._flush_pending()
        node = self._current_node()
        if node is None:
            QMessageBox.warning(self, _app_title(), "请先在左侧选择一个剧情步骤。")
            return False
        errors = [issue for issue in self._preflight_issues() if issue.severity == "error"]
        if errors:
            dialog = PreflightDialog(
                self._preflight_issues(),
                self._locate_preflight_issue,
                self._apply_preflight_fixes,
                self,
            )
            dialog.exec()
            self.statusBar().showMessage("试玩已停止：请先修复体检中的错误", 5000)
            return False

        node_id = str(node.get("id") or "")
        script_id = self._current_id
        stories = copy.deepcopy(self._stories)
        # 舞台状态前导：从原 start 推演到目标节点之前，把当时的场景与台上人物
        # 补成 scene/show 节点链；否则从依赖前置舞台状态的步骤（如 rotate 一个
        # 早前才上场的人物）进入时，游戏会因"角色不存在"崩掉剧情协程而黑屏。
        prelude = build_playtest_prelude(
            self._stories[script_id], node_id, self.editor_data
        )
        if prelude:
            stories[script_id]["nodes"].extend(prelude)
            stories[script_id]["start"] = prelude[0]["id"]
        else:
            stories[script_id]["start"] = node_id
        preview_id = "lom_modkit_preview"
        title = str(self.story.get("title") or script_id)
        manifest = {
            **manifest_versions(),
            "id": preview_id,
            "name": f"编辑器临时试玩：{title}",
            "version": "0.0.0-preview",
            "author": "lom_modkit",
            "description": f"从 {script_id}/{node_id} 开始的临时测试包",
            "entry": script_id,
            "campaign": {
                "new_game": True,
                "disable_official_events": True,
            },
        }
        try:
            game_dir = self.game_manager.require_game_dir()
            self.game_manager.validate_bepinex(game_dir)
            was_running = self.game_manager.is_game_running()
            _runtime_path, runtime_changed = self.game_manager.install_runtime()
            with tempfile.TemporaryDirectory(prefix="lom_modkit_preview_") as tmp:
                package = Path(tmp) / PREVIEW_PACKAGE_NAME
                package_io.export_lommod(package, manifest, stories)
                self.game_manager.install_mod(package, enabled=True)
            self.game_manager.request_preview(preview_id, script_id, node_id)
            started = self.game_manager.launch_game()
        except (GameInstallError, package_io.PackError, OSError) as exc:
            QMessageBox.critical(self, _app_title(), f"无法开始试玩：{exc}")
            return False

        if runtime_changed and was_running:
            QMessageBox.information(
                self,
                "试玩已经准备好",
                "运行时刚刚更新，但当前游戏仍加载着旧版本。\n\n"
                "请完整退出游戏后重新启动；启动后会自动进入这个步骤。",
            )
            message = "试玩已准备：请重启游戏，随后会自动进入选中步骤"
        elif started:
            message = f"游戏正在启动，将自动进入 {script_id}/{node_id}"
        else:
            message = f"试玩请求已发送，将在游戏进入标题或自由场景后跳到 {script_id}/{node_id}"
        self.statusBar().showMessage(message, 10000)
        return True

    # -------------------------------------------------------------- 已读重置
    def _reset_read_state(self) -> None:
        """把当前 mod 的已读文本记录重置为未读（对话不再变黄/可快进）。"""
        mod_id = str(self.manifest.get("id") or "").strip()
        if not mod_id:
            mod_id = "my_mod"
        if self.game_manager.is_game_running():
            QMessageBox.warning(
                self,
                _app_title(),
                "游戏正在运行。运行中的游戏会把内存里的旧已读清单写回存档，"
                "重置会失效。\n\n请先退出游戏，再执行此操作。",
            )
            return
        answer = QMessageBox.question(
            self,
            _app_title(),
            f"将把 mod「{mod_id}」以及 F5 试玩包的全部已读文本记录重置为未读"
            "（游戏全局存档 Save_universe.dat / .json 会被修改，自动留 .lomkit_bak 备份）。\n\n继续？",
        )
        if answer != QMessageBox.StandardButton.Yes:
            return
        extra = ["lom_modkit_preview"]
        if not mod_id:
            extra = []
        try:
            read_keys_by_id = {
                mid: build_story_read_keys(mid, self._stories)
                for mid in [mod_id, *extra]
            }
            results = reset_story_read_state(
                mod_id,
                extra_ids=extra,
                read_keys_by_id=read_keys_by_id,
            )
        except GameInstallError as exc:
            QMessageBox.critical(self, _app_title(), f"重置失败：{exc}")
            return
        if not results:
            QMessageBox.information(
                self,
                _app_title(),
                f"没有找到 mod「{mod_id}」或试玩包的已读记录（或尚未安装/游玩过）。",
            )
            return
        total = sum(count for _path, count in results)
        QMessageBox.information(
            self,
            _app_title(),
            f"已重置 {total} 条已读记录（mod「{mod_id}」及 F5 试玩包）。\n"
            "下次进入剧情时对话将显示为未读（不变黄）。",
        )

    def _reload_start_combo_keep(self) -> None:
        """节点 id 可能变了，刷新起始节点下拉框但保留选择。"""
        cur = self.story.get("start", "")
        self._loading = True
        try:
            self._reload_start_combo()
            self.story["start"] = (
                cur
                if self.start_combo.findText(cur) >= 0
                else self.start_combo.currentText()
            )
        finally:
            self._loading = False

    # -------------------------------------------------------------- 文件菜单
    def new_story(self) -> None:
        if not self._confirm_discard():
            return
        self._stories = {}
        self.story = new_editor_story(editor_data=self.editor_data)
        self.manifest = {}
        self.manifest_base = {}
        self._story_paths = {}
        self._saved_snapshot = self._snapshot()
        self._undo_stack.clear()
        self._redo_stack.clear()
        self._pending_before = None
        self._commit_timer.stop()
        self._refresh_all()
        self.statusBar().showMessage("已新建项目：修改示例对白后即可继续添加步骤", 4000)

    def _last_dir(self, key: str) -> str:
        """读取记住的上次目录；没有记录或目录已消失则退回工作目录。"""
        remembered = self.game_manager.load_pref(key)
        if remembered and Path(remembered).is_dir():
            return remembered
        return str(WORK_DIR)

    def _remember_dir(self, key: str, path: str) -> None:
        """文件对话框选完后记住所在目录，下次同类型操作默认从这里开始。"""
        self.game_manager.save_pref(key, str(Path(path).parent))

    def open_story(self) -> None:
        if not self._confirm_discard():
            return
        path, _ = QFileDialog.getOpenFileName(
            self, "打开", self._last_dir("last_story_dir"), "story JSON (*.json)"
        )
        if not path:
            return
        self._remember_dir("last_story_dir", path)
        self._load_story_path(Path(path))

    _RECENT_MAX = 10

    def _should_persist_session(self) -> bool:
        """测试/无头打包自检不要改用户的最近记录。"""
        if not getattr(self, "_prompt_on_discard", True):
            return False
        return os.environ.get("QT_QPA_PLATFORM") != "offscreen"

    def _load_recents(self) -> list[dict]:
        raw = self.game_manager.load_pref("recent_projects")
        if not raw:
            return []
        try:
            data = json.loads(raw)
        except json.JSONDecodeError:
            return []
        if not isinstance(data, list):
            return []
        out = []
        for item in data:
            if not isinstance(item, dict):
                continue
            if item.get("kind") not in ("story", "lommod"):
                continue
            if not item.get("path"):
                continue
            out.append(item)
        return out

    def _remember_project(self, kind: str, path: Path, name: str = "") -> None:
        if not self._should_persist_session():
            return
        resolved = str(Path(path).resolve())
        self.game_manager.save_pref("last_open_kind", kind)
        self.game_manager.save_pref("last_open_path", resolved)
        if self._current_id:
            self.game_manager.save_pref("last_open_story_id", self._current_id)
        recents = [item for item in self._load_recents() if item.get("path") != resolved]
        recents.insert(
            0,
            {
                "kind": kind,
                "path": resolved,
                "name": name or Path(resolved).stem,
            },
        )
        self.game_manager.save_pref(
            "recent_projects",
            json.dumps(recents[: self._RECENT_MAX], ensure_ascii=False),
        )
        self._rebuild_recent_menu()

    def _remember_current_chapter(self) -> None:
        if not self._should_persist_session() or not self._current_id:
            return
        self.game_manager.save_pref("last_open_story_id", self._current_id)

    def _rebuild_recent_menu(self) -> None:
        menu = getattr(self, "_recent_menu", None)
        if menu is None:
            return
        menu.clear()
        recents = self._load_recents()
        if not recents:
            empty = QAction(t("menu.recent_empty"), self)
            empty.setEnabled(False)
            menu.addAction(empty)
            return
        for item in recents:
            kind = item["kind"]
            path = item["path"]
            name = item.get("name") or Path(path).stem
            tag = "Mod" if kind == "lommod" else "剧本"
            action = QAction(f"{name}（{tag}）", self)
            action.setToolTip(path)
            action.triggered.connect(
                lambda _checked=False, k=kind, p=path: self._open_recent(k, p)
            )
            menu.addAction(action)
        menu.addSeparator()
        menu.addAction("清除最近记录", self._clear_recents)

    def _clear_recents(self) -> None:
        self.game_manager.save_pref("recent_projects", "[]")
        self._rebuild_recent_menu()
        self.statusBar().showMessage("已清除最近打开记录", 2500)

    def _open_recent(self, kind: str, path: str) -> None:
        target = Path(path)
        if not target.is_file():
            recents = [item for item in self._load_recents() if item.get("path") != path]
            self.game_manager.save_pref(
                "recent_projects",
                json.dumps(recents, ensure_ascii=False),
            )
            self._rebuild_recent_menu()
            QMessageBox.warning(
                self,
                _app_title(),
                f"找不到文件，已从最近列表移除：\n{path}",
            )
            return
        if not self._confirm_discard():
            return
        if kind == "lommod":
            self._import_lommod_path(target)
        else:
            self._load_story_path(target)

    def restore_last_project(self) -> bool:
        """启动时打开上次的剧本或 Mod；文件不在则保持新建项目。"""
        kind = self.game_manager.load_pref("last_open_kind")
        path = self.game_manager.load_pref("last_open_path")
        if kind not in ("story", "lommod") or not path:
            return False
        target = Path(path)
        if not target.is_file():
            return False
        if kind == "lommod":
            ok = self._import_lommod_path(target)
        else:
            self._load_story_path(target)
            ok = bool(self._story_paths)
        if not ok:
            return False
        story_id = self.game_manager.load_pref("last_open_story_id")
        if story_id and story_id in self._stories and story_id != self._current_id:
            self._current_id = story_id
            self._refresh_all()
        return True

    def _load_story_path(self, path: Path) -> None:
        try:
            story = models.load_story(path)
        except Exception as exc:
            QMessageBox.critical(self, _app_title(), f"打开失败：{exc}")
            return
        repaired = models.normalize_character_ids([story], self.editor_data)
        self._stories = {story["id"]: story}
        self._current_id = story["id"]
        self.manifest = {}
        self.manifest_base = {}
        self._story_paths = {story["id"]: path}
        self._saved_snapshot = self._snapshot()
        self._undo_stack.clear()
        self._redo_stack.clear()
        self._pending_before = None
        self._commit_timer.stop()
        self._refresh_all()
        if repaired:
            self._set_dirty(True)
        self._remember_project("story", path, str(story.get("title") or path.stem))
        repair_note = f"；已自动修复 {repaired} 个人物内部 ID" if repaired else ""
        self.statusBar().showMessage(f"已打开 {path}{repair_note}", 5000)

    def save_story(self) -> bool:
        """Ctrl+S：已有路径直接覆盖；第一次保存才询问路径。"""
        path = self.story_path
        if path is None:
            return self.save_story_as()
        return self._write_current_story(path)

    def save_story_as(self) -> bool:
        """另存为当前章节；无论是否已有路径都显示文件选择框。"""
        current = str(self.story_path) if self.story_path else ""
        path, _ = QFileDialog.getSaveFileName(
            self,
            "另存为",
            current or str(
                Path(self._last_dir("last_story_dir")) / f"{self._current_id}.json"
            ),
            "story JSON (*.json)",
        )
        if not path:
            return False
        if self._write_current_story(Path(path)):
            self._remember_dir("last_story_dir", path)
            return True
        return False

    def _write_current_story(self, path: Path) -> bool:
        """写入已确定的路径，不弹出对话框。"""
        try:
            models.save_story(self.story, path)
        except Exception as exc:
            QMessageBox.critical(self, _app_title(), f"保存失败：{exc}")
            return False
        self._story_paths[self._current_id] = path
        self._mark_saved()
        self._remember_project("story", path, str(self.story.get("title") or path.stem))
        self.statusBar().showMessage(f"已保存 {path}", 3000)
        return True

    def import_lommod(self) -> None:
        if not self._confirm_discard():
            return
        path, _ = QFileDialog.getOpenFileName(
            self, "导入 Mod", self._last_dir("last_mod_dir"), "LoM Mod 包 (*.lommod)"
        )
        if not path:
            return
        self._remember_dir("last_mod_dir", path)
        self._import_lommod_path(Path(path))

    def _import_lommod_path(self, path: Path) -> bool:
        try:
            manifest, stories = package_io.import_lommod(path)
        except package_io.PackError as exc:
            QMessageBox.critical(self, _app_title(), str(exc))
            return False
        # 包内全部剧情进项目（键以 story 内部 id 为准）
        self._stories = {str(st.get("id") or s): st for s, st in stories.items()}
        repaired = models.normalize_character_ids(self._stories, self.editor_data)
        entry = manifest.get("entry")
        self._current_id = entry if entry in self._stories else sorted(self._stories)[0]
        self.manifest = manifest
        self.manifest_base = manifest  # 保留 campaign 等，导出时回填
        self._story_paths = {}
        self._saved_snapshot = self._snapshot()
        self._undo_stack.clear()
        self._redo_stack.clear()
        self._pending_before = None
        self._commit_timer.stop()
        self._refresh_all()
        if repaired:
            self._set_dirty(True)
        extra = (
            ""
            if len(self._stories) == 1
            else f"（包内共 {len(self._stories)} 个剧情，当前打开入口 {self._current_id}）"
        )
        if manifest.get("campaign"):
            extra += "（含战役 campaign 配置）"
        if repaired:
            extra += f"（已自动修复 {repaired} 个人物内部 ID）"
        title = str(manifest.get("name") or manifest.get("id") or path.stem)
        self._remember_project("lommod", path, title)
        self.statusBar().showMessage(f"已导入 {title}{extra}", 5000)
        return True

    def export_lommod(self) -> bool:
        """导出 .lommod：包内全部剧情脚本 + manifest；成功返回 True。"""
        self._flush_pending()
        issues = self._preflight_issues()
        errors = [issue for issue in issues if issue.severity == "error"]
        if errors:
            dialog = PreflightDialog(
                issues,
                self._locate_preflight_issue,
                self._apply_preflight_fixes,
                self,
            )
            dialog.exec()
            errors = [
                issue for issue in self._preflight_issues() if issue.severity == "error"
            ]
            if errors:
                self.statusBar().showMessage(
                    f"导出已停止：请先处理 {len(errors)} 个体检错误", 5000
                )
                return False
            issues = self._preflight_issues()
        warnings = [issue for issue in issues if issue.severity == "warning"]
        if warnings:
            answer = QMessageBox.question(
                self,
                "发布前还有提醒",
                f"体检没有发现错误，但还有 {len(warnings)} 条提醒。\n\n"
                "建议先打开“体检”逐条确认。仍然继续导出吗？",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
                QMessageBox.StandardButton.No,
            )
            if answer != QMessageBox.StandardButton.Yes:
                return False
        dlg = ManifestDialog(
            self._current_id,
            self.editor_data,
            sorted(self._stories.keys()),
            self.manifest_base,
            self,
        )
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return False
        manifest = dlg.manifest()
        if not models.MOD_ID_PATTERN.fullmatch(manifest["id"]):
            QMessageBox.warning(
                self,
                _app_title(),
                f"Mod 标识 {manifest['id']!r} 不可用（只允许小写英文、数字、_ 和 -）",
            )
            return False
        path, _ = QFileDialog.getSaveFileName(
            self,
            "导出 Mod",
            str(Path(self._last_dir("last_mod_dir")) / f"{manifest['id']}.lommod"),
            "LoM Mod 包 (*.lommod)",
        )
        if not path:
            return False
        self._remember_dir("last_mod_dir", path)
        try:
            report = package_io.export_lommod(path, manifest, self._stories)
        except package_io.PackError as exc:
            QMessageBox.critical(self, _app_title(), str(exc))
            return False
        self.manifest = manifest
        self.manifest_base = manifest  # 导出成功后回填，下次导出保留 campaign 等配置
        self._saved_snapshot = self._snapshot()
        self._set_dirty(False)
        install_note = ""
        if self.game_manager.load_game_dir() is not None:
            try:
                installed = self.game_manager.install_mod(Path(path), enabled=True)
                install_note = (
                    f"\n\n已自动安装并启用：{installed}\n重启游戏后生效。"
                )
            except GameInstallError as exc:
                install_note = (
                    "\n\nMod 已成功导出，但自动安装失败："
                    f"{exc}\n可在“安装管理”中修复游戏目录后重试。"
                )
        else:
            install_note = (
                "\n\n尚未连接游戏，因此只生成了文件。"
                "在“文件 → 安装管理”配置一次后，以后导出会自动安装。"
            )
        QMessageBox.information(
            self,
            _app_title(),
            f"导出成功：{path}\n\n" + "\n".join(report) + install_note,
        )
        self._remember_project("lommod", Path(path), str(manifest.get("name") or Path(path).stem))
        self.statusBar().showMessage(f"已导出 {path}", 5000)
        return True


def main() -> int:
    # --smoke-exit：启动级自检（显示窗口 1.5 秒后自动退出，退出码 0）；
    # 供打包产物在 QT_QPA_PLATFORM=offscreen 下验证“能启动不崩”。
    args = list(sys.argv)
    smoke_exit = "--smoke-exit" in args
    if smoke_exit:
        args.remove("--smoke-exit")
    smoke_preview: Path | None = None
    if "--smoke-preview" in args:
        at = args.index("--smoke-preview")
        if at + 1 >= len(args):
            return 2
        smoke_preview = Path(args[at + 1])
        del args[at : at + 2]
    app = QApplication(args)
    app.setOrganizationName("lom_modkit")
    _pref_lang = GameInstallManager().load_pref("language")
    init_language(_pref_lang)
    models.refresh_labels()
    install_qt_translator(app)
    app.setApplicationName(_app_title())
    icon_base = (
        Path(getattr(sys, "_MEIPASS")) if models.FROZEN else EDITOR_DIR
    )
    icon_path = icon_base / "assets" / "lom_editor_icon.png"
    if icon_path.is_file():
        app.setWindowIcon(QIcon(str(icon_path)))
    apply_glass_theme(app)  # 纯样式注入：不改任何控件行为
    editor_data, is_fallback = models.load_editor_data(PROJECT_ROOT)
    win = MainWindow(editor_data, is_fallback)
    win.show()
    if smoke_preview is not None:
        win._load_story_path(smoke_preview)
        win.right_tabs.setCurrentWidget(win.preview)
        win._preview_timer.stop()
        win._refresh_preview()
        preview_text = win.preview.toPlainText()
        resource_ok = (
            win.game_manager.runtime_dll.is_file()
            and icon_path.is_file()
            and (icon_base / "assets" / "combo_arrow.svg").is_file()
        )
        preview_ok = (
            preview_text.startswith("-- Generated by lomc")
            and " = function()" in preview_text
            and resource_ok
        )
        if not preview_ok:
            log_crash(
                "冻结版 Lua 预览/安装资源自检失败"
                f"（resource_ok={resource_ok}）：\n" + preview_text
            )
        QTimer.singleShot(0, lambda: app.exit(0 if preview_ok else 3))
    elif smoke_exit:
        QTimer.singleShot(1500, app.quit)
    elif len(args) > 1:  # 支持命令行直接打开 story.json
        win._load_story_path(Path(args[1]))
    else:
        if win.restore_last_project():
            win.statusBar().showMessage(
                win.statusBar().currentMessage() or "已恢复上次打开的项目",
                5000,
            )
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
