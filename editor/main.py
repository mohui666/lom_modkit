# -*- coding: utf-8 -*-
"""活侠传 Mod 剧情编辑器（PySide6）— 主窗口与入口。

三栏布局：左=剧情章节与步骤，中=当前步骤属性，右=画面预览与高级编译结果。
工具栏只保留高频操作（新建/导入/体检/试玩/导出/帮助），其余入口在菜单。
无论从仓库根还是 editor/ 启动，路径都基于本文件所在目录推导项目根。

v4 起支持多剧情脚本管理（项目 = 多个 story + manifest，对应 .lommod 包）、
快照式撤销/重做（连续输入合并为一步）与脏标记（标题 * + 关闭/破坏操作确认）。
"""

from __future__ import annotations

import copy
import faulthandler
import sys
import tempfile
import traceback
from pathlib import Path

from PySide6.QtCore import QTimer, Qt
from PySide6.QtGui import QAction, QIcon, QKeySequence
from PySide6.QtWidgets import (
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
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QMainWindow,
    QMenu,
    QMessageBox,
    QPushButton,
    QSplitter,
    QTabWidget,
    QTableWidget,
    QTableWidgetItem,
    QTextBrowser,
    QToolBar,
    QVBoxLayout,
    QWidget,
)

from glass_theme import apply_glass_theme, mark_primary
from game_install import (
    PREVIEW_PACKAGE_NAME,
    GameInstallError,
    GameInstallManager,
)
from flow_graph import FlowGraphPanel
from help_content import HELP_HTML
import models
from mod_manager_dialog import ModManagerDialog
import package_io
from preflight import PreflightIssue, apply_safe_fixes, run_preflight
from preflight_dialog import PreflightDialog
from lua_preview import LuaPreview, compile_story, lomc_available, get_lomc
from node_form import NodeForm
from preview import CRASH_LOG, StagePreview, load_preview_map, log_crash

EDITOR_DIR = models.editor_dir()
PROJECT_ROOT = models.project_root()
if str(EDITOR_DIR) not in sys.path:
    sys.path.insert(0, str(EDITOR_DIR))
# lomc 编译器：sys.path 引入 <项目根>/compiler（不 pip 安装；冻结态由 PYZ 解析）
if str(PROJECT_ROOT / "compiler") not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT / "compiler"))
# 文件对话框默认目录：冻结态用用户 CWD（解包目录/ exe 目录不可当作工作目录）
WORK_DIR = Path.cwd() if models.FROZEN else PROJECT_ROOT

APP_TITLE = "活侠传 Mod 剧情编辑器"
UNDO_LIMIT = 100  # 撤销栈最大步数（快照式，超限丢最旧）

_crash_logging_installed = False


def new_editor_story(story_id: str = "main", editor_data: dict | None = None) -> dict:
    """创建可立即通过编译检查的新手项目模板。

    models.new_story 的单节点结构是 story_api 的既有契约；图形编辑器在其后补一个
    “结束剧情”，避免新用户什么都没做就先看到“最后节点无法结束”的红色错误。
    """
    story = models.new_story(story_id, editor_data)
    story["nodes"][0]["text"] = "在这里填写第一句对白。"
    story["nodes"].append(models.new_node("end", "n2", editor_data))
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

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("帮助 — 活侠传 Mod 剧情编辑器")
        self.resize(760, 620)
        layout = QVBoxLayout(self)
        browser = QTextBrowser()
        browser.setOpenExternalLinks(False)
        browser.setHtml(HELP_HTML)
        layout.addWidget(browser)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        buttons.rejected.connect(self.reject)
        close_btn = buttons.button(QDialogButtonBox.StandardButton.Close)
        if close_btn is not None:
            close_btn.setText("关闭")
        layout.addWidget(buttons)


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

        self.setWindowTitle("导出 Mod")
        self.resize(920, 560)
        layout = QVBoxLayout(self)
        intro = QLabel("填写玩家能看到的信息。带“可选”的战役设置不需要时可以保持关闭。")
        intro.setWordWrap(True)
        layout.addWidget(intro)
        form = QFormLayout()
        self.id_edit = QLineEdit(str(base.get("id") or "my_mod"))
        self.name_edit = QLineEdit(str(base.get("name") or "我的剧情 Mod"))
        self.version_edit = QLineEdit(str(base.get("version") or "1.0.0"))
        self.author_edit = QLineEdit(str(base.get("author") or ""))
        self.desc_edit = QLineEdit(str(base.get("description") or ""))
        self.id_edit.setPlaceholderText("例如：my_story（小写英文、数字、_ 或 -）")
        self.author_edit.setPlaceholderText("作者名")
        self.desc_edit.setPlaceholderText("用一句话介绍这段剧情")
        form.addRow("Mod 标识（英文）", self.id_edit)
        form.addRow("Mod 名称", self.name_edit)
        form.addRow("版本", self.version_edit)
        form.addRow("作者", self.author_edit)
        form.addRow("简介", self.desc_edit)
        # 多剧情：入口脚本从包内全部剧情脚本里选（回填 base entry / 当前 story）
        self.entry_combo = QComboBox()
        self.entry_combo.setEditable(False)
        for sid in self._story_ids:
            self.entry_combo.addItem(sid, sid)
        default_entry = str(base.get("entry") or story_id)
        idx = self.entry_combo.findData(default_entry)
        self.entry_combo.setCurrentIndex(idx if idx >= 0 else 0)
        form.addRow("开始章节", self.entry_combo)
        layout.addLayout(form)

        # ------------------------------------------------------ campaign 区
        camp_box = QGroupBox("新战役与地图触发（可选）")
        cv = QVBoxLayout(camp_box)
        self.new_game_check = QCheckBox(
            "允许从游戏内“开始新战役”启动（使用独立存档，不影响普通存档）"
        )
        self.new_game_check.setChecked(bool(campaign.get("new_game")))
        cv.addWidget(self.new_game_check)
        self.disable_events_check = QCheckBox(
            "本战役关闭原版剧情事件（只触发这个 Mod 设置的地图剧情）"
        )
        self.disable_events_check.setChecked(
            bool(campaign.get("disable_official_events"))
        )
        cv.addWidget(self.disable_events_check)
        trigger_help = QLabel(
            "地图触发：玩家在自由模式点击指定地点并满足条件时，播放所选章节。"
            "只填地点和章节即可；其余条件都可留空。"
        )
        trigger_help.setWordWrap(True)
        cv.addWidget(trigger_help)
        self.triggers_table = QTableWidget(0, 7)
        self.triggers_table.setHorizontalHeaderLabels(
            [
                "地图位置",
                "播放章节",
                "需要已有标记",
                "需要没有标记",
                "月份 1～12",
                "时段 1～3",
                "好感条件（人物:数值）",
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
        add_btn = QPushButton("添加地图触发")
        del_btn = QPushButton("删除最后一行")
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
            ok_btn.setText("继续导出")
            mark_primary(ok_btn)  # 对话框唯一主操作：accent 染色玻璃
        if cancel_btn is not None:
            cancel_btn.setText("取消")
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
            "format": 1,
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
                "Mod 标识需要修改",
                "只使用小写英文字母、数字、下划线或短横线，例如 my_story。",
            )
            self.id_edit.setFocus()
            return
        for widget, label in (
            (self.name_edit, "Mod 名称"),
            (self.author_edit, "作者"),
            (self.desc_edit, "简介"),
        ):
            if not widget.text().strip():
                QMessageBox.warning(self, "信息没有填完", f"请填写“{label}”。")
                widget.setFocus()
                return
        entry = self.entry_combo.currentText().strip()
        if entry not in self._story_ids:
            QMessageBox.warning(self, "开始章节不存在", "请从项目中的剧情章节里选择开始章节。")
            self.entry_combo.setFocus()
            return
        manifest = self.manifest()
        for trig in (manifest.get("campaign") or {}).get("triggers", []):
            if trig.get("script") not in self._story_ids:
                QMessageBox.warning(
                    self,
                    "地图触发章节不存在",
                    f"地图触发中的章节 {trig.get('script')!r} 不在当前项目里。",
                )
                return
        lomc, err = get_lomc()
        if lomc is None:
            QMessageBox.warning(self, "无法检查导出信息", f"编译器不可用：{err}")
            return
        try:
            lomc.validate_manifest(manifest, "导出设置")
        except Exception as exc:
            QMessageBox.warning(self, "导出信息需要修改", str(exc))
            return
        super().accept()


class MainWindow(QMainWindow):
    def __init__(self, editor_data: dict, is_fallback: bool, parent=None):
        super().__init__(parent)
        self.setWindowTitle(APP_TITLE)
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

    # ------------------------------------------------------------------ UI
    def _build_ui(self) -> None:
        splitter = QSplitter(Qt.Orientation.Horizontal)

        # 左栏：章节切换 + 剧情属性 + 步骤列表 + 操作按钮
        left = QWidget()
        lv = QVBoxLayout(left)
        lv.setContentsMargins(4, 4, 4, 4)
        story_row = QHBoxLayout()
        story_row.addWidget(QLabel("剧情章节"))
        self.story_combo = QComboBox()
        self.story_combo.currentIndexChanged.connect(self._on_story_switched)
        story_row.addWidget(self.story_combo, stretch=1)
        self.add_story_btn = QPushButton("新建章节")
        self.del_story_btn = QPushButton("删除章节")
        self.add_story_btn.clicked.connect(self._add_story_in_project)
        self.del_story_btn.clicked.connect(self._delete_story_in_project)
        story_row.addWidget(self.add_story_btn)
        story_row.addWidget(self.del_story_btn)
        lv.addLayout(story_row)
        quick_tip = QLabel(
            "按顺序制作：添加步骤 → 填写内容 → 体检 → 导出 Mod"
        )
        quick_tip.setWordWrap(True)
        quick_tip.setToolTip("按 F1 随时打开完整帮助")
        lv.addWidget(quick_tip)
        props = QFormLayout()
        self.story_id_edit = QLineEdit()
        self.story_title_edit = QLineEdit()
        self.start_combo = QComboBox()
        self.mood_check = QCheckBox(
            "保留官方心情气泡（关闭时每次登场/对白自动隐藏圆形情绪面板）"
        )
        self.story_id_edit.textChanged.connect(self._on_story_props_changed)
        self.story_title_edit.textChanged.connect(self._on_story_props_changed)
        self.start_combo.currentTextChanged.connect(self._on_start_changed)
        self.mood_check.toggled.connect(self._on_story_mood_changed)
        self.story_id_edit.setPlaceholderText("例如：main")
        self.story_title_edit.setPlaceholderText("玩家易懂的章节名称")
        props.addRow("章节编号", self.story_id_edit)
        props.addRow("章节名称", self.story_title_edit)
        props.addRow("从这里开始", self.start_combo)
        props.addRow("心情气泡", self.mood_check)
        lv.addLayout(props)

        self.node_list = QListWidget()
        self.node_list.currentRowChanged.connect(self._on_node_selected)
        lv.addWidget(self.node_list, stretch=1)

        btns = QHBoxLayout()
        add_btn = QPushButton("添加剧情步骤 ▾")
        add_menu = QMenu(add_btn)
        common = add_menu.addMenu("常用步骤")
        for t in models.COMMON_NODE_TYPES:
            cn = models.NODE_TYPE_CN.get(t, t)
            common.addAction(cn, lambda checked=False, t=t: self._add_node(t))
        common.addSeparator()
        common.addAction("汗青书结局", self._add_ending_card)
        add_menu.addSeparator()
        # 完整功能按用途分组；内部英文类型只放在提示中，不挤占菜单文案。
        for group_name, types in models.NODE_GROUPS:
            sub = add_menu.addMenu(group_name)
            for t in types:
                cn = models.NODE_TYPE_CN.get(t, t)
                action = sub.addAction(cn, lambda checked=False, t=t: self._add_node(t))
                action.setToolTip(f"内部类型：{t}")
        add_btn.setMenu(add_menu)
        del_btn = QPushButton("删除步骤")
        up_btn = QPushButton("上移")
        down_btn = QPushButton("下移")
        del_btn.clicked.connect(self._delete_node)
        up_btn.clicked.connect(lambda: self._move_node(-1))
        down_btn.clicked.connect(lambda: self._move_node(1))
        for b in (add_btn, del_btn, up_btn, down_btn):
            b.setMinimumHeight(30)
            btns.addWidget(b)
        lv.addLayout(btns)

        # 中栏：属性表单；右栏：页签（演出预览 / Lua 预览）
        self.form = NodeForm()
        self.preview = LuaPreview()
        self.stage = StagePreview()
        pmap, data_dir = load_preview_map(PROJECT_ROOT)
        self.stage.set_assets(pmap, data_dir)
        self.stage.set_context(self.editor_data)
        self.stage.choice_activated.connect(self._on_choice_goto)

        stage_tab = QWidget()
        sv = QVBoxLayout(stage_tab)
        sv.setContentsMargins(4, 4, 4, 4)
        sv.addLayout(self._build_stage_toolbar())
        sv.addWidget(self.stage, stretch=1)

        self.right_tabs = QTabWidget()
        self.right_tabs.addTab(stage_tab, "画面预览")
        self.flow_graph = FlowGraphPanel()
        self.flow_graph.node_activated.connect(self._on_flow_node_activated)
        self.right_tabs.addTab(self.flow_graph, "流程图")
        self.right_tabs.addTab(self.preview, "编译结果（高级）")
        self.right_tabs.currentChanged.connect(self._on_right_tab_changed)

        splitter.addWidget(left)
        splitter.addWidget(self.form)
        splitter.addWidget(self.right_tabs)
        splitter.setSizes([320, 460, 500])
        self.setCentralWidget(splitter)

        # 预览刷新防抖
        self._preview_timer = QTimer(self)
        self._preview_timer.setSingleShot(True)
        self._preview_timer.setInterval(300)
        self._preview_timer.timeout.connect(self._refresh_preview)

        self._graph_timer = QTimer(self)
        self._graph_timer.setSingleShot(True)
        self._graph_timer.setInterval(180)
        self._graph_timer.timeout.connect(self._refresh_flow_graph)

        # 撤销合并：连续编辑暂停 600ms 后提交为一步
        self._commit_timer = QTimer(self)
        self._commit_timer.setSingleShot(True)
        self._commit_timer.setInterval(600)
        self._commit_timer.timeout.connect(self._flush_pending)

        # 自动播放：1.5 秒步进，遇 choice/branch/dice 自动暂停
        self._auto_timer = QTimer(self)
        self._auto_timer.setInterval(1500)
        self._auto_timer.timeout.connect(self._auto_step)

    def _build_stage_toolbar(self) -> QHBoxLayout:
        """演出预览上方的小工具条：步进 + 自动播放。"""
        bar = QHBoxLayout()
        bar.setContentsMargins(0, 0, 0, 0)
        home_btn = QPushButton("回到开头")
        prev_btn = QPushButton("上一步")
        next_btn = QPushButton("下一步")
        self.auto_btn = QPushButton("自动播放")
        self.auto_btn.setCheckable(True)
        home_btn.clicked.connect(self._goto_start)
        prev_btn.clicked.connect(lambda: self._step_selection(-1))
        next_btn.clicked.connect(lambda: self._step_selection(1))
        self.auto_btn.toggled.connect(self._on_auto_toggled)
        for b in (home_btn, prev_btn, next_btn, self.auto_btn):
            b.setMinimumHeight(30)
            bar.addWidget(b)
        bar.addStretch(1)
        return bar

    def _build_menu(self) -> None:
        menu = self.menuBar().addMenu("文件(&F)")
        menu.addAction("新建", self.new_story, QKeySequence.StandardKey.New)
        menu.addAction(
            "打开…", self.open_story, QKeySequence.StandardKey.Open
        )
        menu.addAction(
            "保存", self.save_story, QKeySequence.StandardKey.Save
        )
        menu.addAction(
            "另存为…",
            self.save_story_as,
            QKeySequence.StandardKey.SaveAs,
        )
        menu.addSeparator()
        menu.addAction("导入 Mod…", self.import_lommod)
        menu.addAction("导出 Mod…", self.export_lommod)
        menu.addAction("安装管理…", self._show_mod_manager)
        menu.addSeparator()
        menu.addAction("退出", self.close)
        edit = self.menuBar().addMenu("编辑(&E)")
        edit.addAction("撤销", self._undo, QKeySequence.StandardKey.Undo)
        edit.addAction("重做", self._redo, QKeySequence.StandardKey.Redo)
        run_menu = self.menuBar().addMenu("试玩(&R)")
        run_menu.addAction(
            "试玩",
            self.play_from_current_node,
        )
        run_menu.addAction("流程图", self._show_flow_graph, QKeySequence("F7"))
        help_menu = self.menuBar().addMenu("帮助(&H)")
        help_menu.addAction("帮助", self._show_help)

    def _build_toolbar(self) -> None:
        """只放高频操作；流程图有专属页签、安装管理在文件菜单，不再重复占位。"""
        bar = QToolBar("常用操作", self)
        bar.setMovable(False)
        bar.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextOnly)

        def add(text: str, slot, tip: str, shortcut=None) -> QAction:
            action = QAction(text, self)
            action.setToolTip(tip)
            if shortcut is not None:
                action.setShortcut(shortcut)
            action.triggered.connect(slot)
            bar.addAction(action)
            return action

        add("新建", self.new_story, "创建一段可直接检查的示例剧情")
        add("导入 Mod", self.import_lommod, "打开已有 .lommod 继续编辑")
        bar.addSeparator()
        self.check_action = add(
            "体检", self._check_project, "检查错误、断路、占位文字和图片素材", QKeySequence("F6")
        )
        add(
            "试玩",
            self.play_from_current_node,
            "临时安装当前项目并自动从选中的步骤进入游戏（F5）",
            QKeySequence("F5"),
        )
        export_action = add("导出 Mod", self.export_lommod, "检查并生成可安装的 .lommod")
        bar.addSeparator()
        add("帮助", self._show_help, "打开快速入门、结局写法和排错说明", QKeySequence("F1"))
        self.addToolBar(bar)
        export_btn = bar.widgetForAction(export_action)
        if export_btn is not None:
            # 液态玻璃：工具栏内唯一 accent 主操作（规范：每屏最多一个染色背景按钮）
            mark_primary(export_btn)

    def _show_help(self) -> None:
        HelpDialog(self).exec()

    def _show_mod_manager(self) -> None:
        ModManagerDialog(self.game_manager, self).exec()

    def _show_flow_graph(self) -> None:
        self._refresh_flow_graph()
        self.right_tabs.setCurrentWidget(self.flow_graph)

    def _refresh_flow_graph(self) -> None:
        self.flow_graph.set_story(self.story, self.editor_data)

    def _on_flow_node_activated(self, node_id: str) -> None:
        row = self._node_row(node_id)
        if row < 0:
            return
        self.node_list.setCurrentRow(row)
        self.node_list.scrollToItem(self.node_list.item(row))
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
                f"体检完成：还有 {remaining_errors} 个错误需要修改", 5000
            )
            return False
        self.statusBar().showMessage("体检通过，可以导出 Mod", 5000)
        return True

    def _preflight_issues(self) -> list[PreflightIssue]:
        entry = self.manifest_base.get("entry") or self.manifest.get("entry")
        if entry not in self._stories:
            entry = self._current_id
        return run_preflight(self._stories, self.editor_data, str(entry))

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
        self.node_list.setFocus()
        self.statusBar().showMessage(
            f"已定位：章节 {issue.story_id} / 步骤 {issue.node_id or '剧情设置'}",
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
        self._refresh_all()
        self.statusBar().showMessage(f"已切换到剧情章节 {sid}", 2000)

    def _add_story_in_project(self) -> None:
        """项目内新建剧情脚本（多剧情），并切换到它。"""
        sid = models.make_story_id(self._stories)
        self._record_discrete()
        self._stories[sid] = new_editor_story(sid, self.editor_data)
        self._current_id = sid
        self._refresh_all()
        self.statusBar().showMessage(f"已新建剧情章节 {sid}", 3000)

    def _delete_story_in_project(self) -> None:
        """从项目删除当前剧情脚本（可撤销）。"""
        if len(self._stories) <= 1:
            QMessageBox.warning(self, APP_TITLE, "项目中至少要保留一个剧情章节")
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

    def _reload_node_list(self, select_row: int = -1) -> None:
        cur = self.node_list.currentRow()
        # 全程屏蔽信号：clear() 会把 currentRow 变 -1，若放开信号，恢复选中时
        # 会重入 _on_node_selected → 重建表单；当本方法是由表单控件自己的信号
        # 链（如 textChanged）触发的，重建会删除正在发信号的控件 → 段错误。
        # 选中变化后的表单/预览刷新由调用方显式做（_refresh_all 等）。
        self.node_list.blockSignals(True)
        self.node_list.clear()
        for n in self.story.get("nodes", []):
            item = QListWidgetItem(
                f"{n.get('id', '')}  {models.node_summary(n, self.editor_data)}"
            )
            self.node_list.addItem(item)
        row = select_row if select_row >= 0 else cur
        if self.node_list.count():
            self.node_list.setCurrentRow(min(max(0, row), self.node_list.count() - 1))
        self.node_list.blockSignals(False)

    def _current_node(self) -> dict | None:
        row = self.node_list.currentRow()
        nodes = self.story.get("nodes", [])
        return nodes[row] if 0 <= row < len(nodes) else None

    def _load_form(self) -> None:
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
        self.node_list.setCurrentRow(max(0, row))

    def _step_selection(self, delta: int) -> None:
        """上一步/下一步：按节点数组序移动选中。"""
        row = self.node_list.currentRow()
        to = row + delta
        if 0 <= to < self.node_list.count():
            self.node_list.setCurrentRow(to)

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
            row = self.node_list.currentRow()
            if row + 1 >= self.node_list.count():
                self._stop_auto()
                self.statusBar().showMessage("自动播放：已到末尾，暂停", 3000)
                return
            self.node_list.setCurrentRow(row + 1)
            node = self._current_node()
            # choice/branch/dice 都是分支点：暂停等人选
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
                self.node_list.setCurrentRow(row)
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
        self.setWindowTitle(f"{APP_TITLE} — {name}{star}")

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
        box.setWindowTitle(APP_TITLE)
        box.setText("有未保存的修改，如何处理？")
        box.setInformativeText("导出 Mod 会保存全部章节；也可以放弃这次修改。")
        save_btn = box.addButton("导出 Mod…", QMessageBox.ButtonRole.AcceptRole)
        discard_btn = box.addButton("放弃修改", QMessageBox.ButtonRole.DestructiveRole)
        box.addButton("取消", QMessageBox.ButtonRole.RejectRole)
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
            node_type, models.make_node_id(self.story), self.editor_data
        )
        self._insert_node(node)

    def _add_ending_card(self) -> None:
        """新手预设：原版 EndGamePanel 汗青书样式，确认后回标题画面。"""
        node = models.new_node(
            "goto_scene", models.make_node_id(self.story), self.editor_data
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
        row = self.node_list.currentRow()
        at = row + 1 if 0 <= row < len(nodes) else len(nodes)
        nodes.insert(at, node)
        self._refresh_all(select_row=at)
        type_cn = models.NODE_TYPE_CN.get(node.get("type", ""), node.get("type", ""))
        self.statusBar().showMessage(
            message or f"已添加 {type_cn}（{node.get('id', '')}）", 3000
        )

    def _delete_node(self) -> None:
        nodes = self.story.get("nodes", [])
        row = self.node_list.currentRow()
        if not (0 <= row < len(nodes)):
            return
        if len(nodes) <= 1:
            QMessageBox.warning(self, APP_TITLE, "至少保留一个节点")
            return
        self._record_discrete()
        removed = nodes.pop(row)
        if self.story.get("start") == removed.get("id"):
            self.story["start"] = nodes[0].get("id", "")
        self._refresh_all(select_row=min(row, len(nodes) - 1))

    def _move_node(self, delta: int) -> None:
        nodes = self.story.get("nodes", [])
        row = self.node_list.currentRow()
        to = row + delta
        if not (0 <= row < len(nodes) and 0 <= to < len(nodes)):
            return
        self._record_discrete()
        nodes[row], nodes[to] = nodes[to], nodes[row]
        self._refresh_all(select_row=to)

    # -------------------------------------------------------------- 信号槽
    def _on_node_selected(self, _row: int) -> None:
        if not self._loading:
            self._load_form()
            self._refresh_stage()  # 演出预览随选中刷新
            self._schedule_preview()  # Lua 页签也刷新（防抖）

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
            not models.ID_PATTERN.match(new_id)
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
            QMessageBox.warning(self, APP_TITLE, "请先在左侧选择一个剧情步骤。")
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
        stories[script_id]["start"] = node_id
        preview_id = "lom_modkit_preview"
        title = str(self.story.get("title") or script_id)
        manifest = {
            "format": 1,
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
            QMessageBox.critical(self, APP_TITLE, f"无法开始试玩：{exc}")
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

    def open_story(self) -> None:
        if not self._confirm_discard():
            return
        path, _ = QFileDialog.getOpenFileName(
            self, "打开", str(WORK_DIR), "story JSON (*.json)"
        )
        if not path:
            return
        self._load_story_path(Path(path))

    def _load_story_path(self, path: Path) -> None:
        try:
            story = models.load_story(path)
        except Exception as exc:
            QMessageBox.critical(self, APP_TITLE, f"打开失败：{exc}")
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
            current or str(WORK_DIR / f"{self._current_id}.json"),
            "story JSON (*.json)",
        )
        if not path:
            return False
        return self._write_current_story(Path(path))

    def _write_current_story(self, path: Path) -> bool:
        """写入已确定的路径，不弹出对话框。"""
        try:
            models.save_story(self.story, path)
        except Exception as exc:
            QMessageBox.critical(self, APP_TITLE, f"保存失败：{exc}")
            return False
        self._story_paths[self._current_id] = path
        self._mark_saved()
        self.statusBar().showMessage(f"已保存 {path}", 3000)
        return True

    def import_lommod(self) -> None:
        if not self._confirm_discard():
            return
        path, _ = QFileDialog.getOpenFileName(
            self, "导入 Mod", str(WORK_DIR), "LoM Mod 包 (*.lommod)"
        )
        if not path:
            return
        try:
            manifest, stories = package_io.import_lommod(path)
        except package_io.PackError as exc:
            QMessageBox.critical(self, APP_TITLE, str(exc))
            return
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
        self.statusBar().showMessage(
            f"已导入 {manifest.get('name', manifest.get('id'))}{extra}", 5000
        )

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
                APP_TITLE,
                f"Mod 标识 {manifest['id']!r} 不可用（只允许小写英文、数字、_ 和 -）",
            )
            return False
        path, _ = QFileDialog.getSaveFileName(
            self,
            "导出 Mod",
            str(WORK_DIR / f"{manifest['id']}.lommod"),
            "LoM Mod 包 (*.lommod)",
        )
        if not path:
            return False
        try:
            report = package_io.export_lommod(path, manifest, self._stories)
        except package_io.PackError as exc:
            QMessageBox.critical(self, APP_TITLE, str(exc))
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
            APP_TITLE,
            f"导出成功：{path}\n\n" + "\n".join(report) + install_note,
        )
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
    app.setApplicationName("活侠传 Mod 剧情编辑器")
    app.setOrganizationName("lom_modkit")
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
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
