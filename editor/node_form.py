# -*- coding: utf-8 -*-
"""节点属性表单：按节点 type 依据 models.NODE_SCHEMAS 动态生成控件。

- 人物/表情/站位/场景/音乐/stat key/mode/朝向 → 下拉框（数据来自 editor_data）
- text → 多行编辑框；整数/小数 → spinbox；bool → 勾选框
- choice.options / branch.cases → 可增删行的表格，goto 列为节点 id 下拉框
任何编辑都会把值写回节点 dict 并发出 node_changed 信号。
"""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import QTimer, Qt, Signal
from PySide6.QtGui import QFont, QFontDatabase
from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDoubleSpinBox,
    QFileDialog,
    QFormLayout,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPlainTextEdit,
    QPushButton,
    QScrollArea,
    QSpinBox,
    QTableWidget,
    QTableWidgetItem,
    QToolButton,
    QVBoxLayout,
    QWidget,
)

from asset_store import AssetStoreError, import_image_file
import content_registry
from i18n import t
import models


class NodeForm(QScrollArea):
    """中栏：单个节点的属性编辑表单。"""

    node_changed = Signal()  # 当前节点内容被用户修改
    id_change_requested = Signal(str)  # 用户改了步骤编号，由主窗口同步全部引用

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWidgetResizable(True)
        self._node: dict | None = None
        self._editor_data: dict = models.FALLBACK_EDITOR_DATA
        self._node_ids: list[str] = []
        self._story_ids: list[str] = []  # 包内剧情脚本 id（end.next_script 下拉用）
        self._loading = False  # 重建表单期间屏蔽信号

    # ------------------------------------------------------------------ 对外
    def set_context(
        self, editor_data: dict, node_ids: list[str], story_ids: list[str] | None = None
    ) -> None:
        """更新下拉框数据来源（editor_data / 全部节点 id / 包内剧情脚本 id）。"""
        self._editor_data = editor_data
        self._node_ids = list(node_ids)
        self._story_ids = list(story_ids or [])

    def _emit_id_change(self, edit: QLineEdit, original: str) -> None:
        if self._loading or self._node is None:
            return
        new_id = edit.text().strip()
        if not new_id or new_id == original:
            if new_id != original:
                edit.setText(original)
            return
        self.id_change_requested.emit(new_id)

    def set_node(self, node: dict | None) -> None:
        """展示并编辑给定节点；None 时清空。"""
        self._loading = True
        try:
            self._node = node
            body = QWidget()
            layout = QVBoxLayout(body)
            layout.setContentsMargins(12, 12, 12, 12)
            if node is None:
                layout.addWidget(QLabel(t("form.select_step")))
                layout.addStretch(1)
            else:
                layout.addWidget(self._build_form(node))
                layout.addStretch(1)
            self.setWidget(body)
        finally:
            self._loading = False

    # ------------------------------------------------------------------ 构建
    def _build_form(self, node: dict) -> QWidget:
        """主参数常驻；高级参数（如跳转）默认折叠。"""
        wrap = QWidget()
        outer = QVBoxLayout(wrap)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(8)

        form = QFormLayout()
        form.setSpacing(10)
        node_type = node.get("type", "")
        schema = models.NODE_SCHEMAS.get(node_type)

        type_cn = models.NODE_TYPE_CN.get(node_type, node_type)
        head = QLabel(type_cn)
        hf = QFont(head.font())
        hf.setBold(True)
        hf.setPointSize(hf.pointSize() + 2)
        head.setFont(hf)
        head.setToolTip(f"{node.get('id', '')} · 内部类型：{node_type}")
        outer.addWidget(head)
        id_row = QHBoxLayout()
        id_row.setContentsMargins(0, 0, 0, 0)
        id_label = QLabel(t("field.node_id"))
        id_label.setProperty("context_help", True)
        id_edit = QLineEdit(str(node.get("id", "")))
        id_edit.setObjectName("nodeIdEdit")
        id_edit.setPlaceholderText(t("nav.rename_prompt"))
        id_edit.editingFinished.connect(
            lambda edit=id_edit, original=str(node.get("id", "")): self._emit_id_change(
                edit, original
            )
        )
        id_row.addWidget(id_label)
        id_row.addWidget(id_edit, 1)
        outer.addLayout(id_row)

        help_text = models.NODE_HELP.get(node_type)
        if help_text:
            hint = QLabel(help_text)
            hint.setWordWrap(True)
            hint.setProperty("context_help", True)
            outer.addWidget(hint)

        if schema is None:
            form.addRow(QLabel(t("form.unknown_type")))
            outer.addLayout(form)
            return wrap

        for key, label, kind, optional in schema["fields"]:
            # branch 的键字段按 source 显示：stat 来源显示属性下拉，其余显示 flag
            if node_type == "branch":
                src = node.get("source", "mod")
                if key == "stat" and src != "stat":
                    continue
                if key == "flag" and src == "stat":
                    continue
            if not self._field_visible(node_type, key, node):
                continue
            widget = self._make_widget(node, key, kind)
            shown = t(f"field.{key}", default=label)
            if optional:
                shown += t("field.optional")
            form.addRow(shown, widget)
        outer.addLayout(form)

        # choice/branch/dice/end/death/goto_scene 之外允许显式 goto（契约 §3/§4）
        if node_type not in ("choice", "branch", "dice", "end", "death", "goto_scene"):
            adv_btn = QToolButton()
            adv_btn.setText(t("form.advanced"))
            adv_btn.setCheckable(True)
            adv_btn.setAutoRaise(True)
            adv_btn.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextOnly)
            adv_body = QWidget()
            adv_form = QFormLayout(adv_body)
            adv_form.setContentsMargins(8, 4, 0, 0)
            goto = self._make_goto_combo(node.get("goto", ""), allow_empty=True)
            goto.currentTextChanged.connect(
                lambda text, c=goto: self._apply(
                    node, "goto", self._combo_value(c, text).strip() or None
                )
            )
            adv_form.addRow(t("field.goto"), goto)
            has_goto = bool(node.get("goto"))
            adv_body.setVisible(has_goto)
            if has_goto:
                adv_btn.setChecked(True)
                adv_btn.setText(t("form.advanced_on"))

            def _toggle(on: bool, btn=adv_btn, body=adv_body) -> None:
                body.setVisible(on)
                btn.setText(t("form.advanced_on") if on else t("form.advanced"))

            adv_btn.toggled.connect(_toggle)
            outer.addWidget(adv_btn)
            outer.addWidget(adv_body)
        return wrap

    @staticmethod
    def _field_visible(node_type: str, key: str, node: dict) -> bool:
        """按当前选择隐藏无效字段，避免让新手填写游戏根本不会读取的值。"""
        if node_type == "say" and key in ("character", "portrait"):
            return node.get("mode", "character") not in ("narrative", "center")
        if node_type == "intro":
            custom = node.get("intro_source", "official") == "custom"
            if key == "character":
                return not custom
            if key in (
                "title",
                "name",
                "text",
                "image",
                "image_scale",
                "image_x",
                "image_y",
            ):
                return custom
        if node_type == "goto_scene":
            scene = node.get("scene", "Free")
            if key == "key":
                return scene in ("Combat", "Battle", "GameOver", "End")
            if key == "next":
                # 仅战斗/战役会读取 CurrentNextScene；GameOver 与汗青书结局
                # 的返回按钮/标准收尾都由原版固定。
                return scene in ("Combat", "Battle")
            if key in ("title", "desc"):
                return scene in ("GameOver", "End")
            if key == "image":
                return scene == "End"
        if node_type == "death" and key == "next":
            return False
        return True

    def _make_widget(self, node: dict, key: str, kind: str) -> QWidget:
        value = node.get(key)
        if kind == "character":
            w = self._make_combo(
                models.list_items(self._editor_data, "characters"),
                value or "",
                editable=True,
            )
            w.currentTextChanged.connect(
                lambda t, c=w: self._on_character_changed(node, key, c, t)
            )
            return w
        if kind == "portrait":
            char_id = node.get("character", "")
            items = models.character_portraits(self._editor_data, char_id)
            w = self._make_combo(
                [(p, p) for p in items], value or "normal", editable=True
            )
            w.currentTextChanged.connect(lambda t: self._apply(node, key, t))
            # 记录起来，人物变化时刷新表情清单
            w.setProperty("portrait_for", key)
            return w
        if kind == "voice":
            return self._make_voice_picker(node, key, value)
        if kind == "music":
            return self._make_audio_combo(node, key, value, audio_kind="music")
        if kind == "sound_name":
            sound_kind = node.get("kind", "sound")
            return self._make_audio_combo(
                node,
                key,
                value,
                audio_kind="env" if sound_kind == "env" else "sound",
            )
        if kind in ("position", "view", "stat"):
            # schema 2 清单：{id,name} 对象数组，显示 "名字（id）"
            data_key = {
                "position": "positions",
                "view": "views",
                "stat": "stats",
            }[kind]
            if kind == "position":
                items = models.list_items(self._editor_data, data_key)
                items = [
                    (
                        item_id,
                        display + t("form.back_position")
                        if item_id in ("LB2", "RB2")
                        else display,
                    )
                    for item_id, display in items
                ]
                return self._combo_from_items(node, key, items, value)
            return self._list_combo(node, key, data_key, value)
        if kind == "mode":
            items = [
                (m, f"{models.MODE_CN.get(m, m)}（{m}）")
                for m in self._editor_data.get("modes") or models.MODE_CN
            ]
            w = self._make_combo(items, value or "character", editable=True)
            w.currentTextChanged.connect(
                lambda t, c=w: self._apply(node, key, self._combo_value(c, t))
            )
            return w
        if kind == "facing":
            items = [(v, f"{cn}（{v}）") for v, cn in models.FACING_CN]
            w = self._make_combo(items, value or "right")
            w.currentTextChanged.connect(
                lambda _t, c=w: self._apply(node, key, c.currentData())
            )
            return w
        if kind == "branch_source":
            w = self._make_combo(list(models.BRANCH_SOURCES), value or "mod")
            w.currentTextChanged.connect(
                lambda _t, c=w: self._on_source_changed(node, key, c)
            )
            return w
        if kind.startswith("enum:"):
            # 固定枚举：显示 "中文（值）"；部分枚举切换后要重建表单
            set_name = kind.split(":", 1)[1]
            options = [(v, f"{cn}（{v}）") for v, cn in models.ENUM_SETS[set_name]]
            w = self._make_combo(options, value or (options[0][0] if options else ""))
            w.currentTextChanged.connect(
                lambda _t, c=w, s=set_name: self._on_enum_changed(node, key, s, c)
            )
            return w
        if kind == "menu_dialog":
            # choice 皮肤只有 Options 安全：其余（Talk/Section_*/Kitchen 等）是自由
            # 场景 break 格式菜单，纯文本选项会触发 BreakOptionButton 越界崩溃
            return self._combo_from_items(
                node, key, [("Options", "Options")], value or "Options"
            )
        if kind == "effect":
            return self._combo_from_items(
                node, key, models.list_items(self._editor_data, "effects"), value
            )
        if kind == "camera":
            return self._combo_from_items(
                node, key, [(p, p) for p in models.CAMERA_PRESETS], value
            )
        if kind == "talent":
            return self._combo_from_items(
                node, key, models.list_items(self._editor_data, "talents"), value
            )
        if kind == "game_flag":
            return self._combo_from_items(
                node, key, models.list_items(self._editor_data, "game_flags"), value
            )
        if kind == "dice_check":
            # 仅含带官方元数据的检查点（无元数据会在游戏内骰子菜单崩溃）
            w = self._make_combo(
                models.dice_check_items(self._editor_data), value or "", editable=True
            )
            w.currentTextChanged.connect(
                lambda t, c=w: self._on_dice_check_changed(node, key, c, t)
            )
            return w
        if kind == "death_id":
            # mod 专属死亡画面 id（9+官方 id，如 910021）：官方 id 会触发结局解锁与记录
            return self._make_death_id_widget(node, key, value)
        if kind == "item":
            # 物品清单随 kind 字段（book/misc/special）切换；切换时表单已重建
            data_key = f"items_{node.get('kind', 'misc')}"
            return self._combo_from_items(
                node, key, models.list_items(self._editor_data, data_key), value
            )
        if kind == "goto_scene_key":
            # 场景参数清单随 scene 字段切换（战斗/战役/死亡画面/结局 id）
            scene = node.get("scene", "Free")
            data_key = {
                "Combat": "combat_ids",
                "Battle": "battle_ids",
                "GameOver": "death_ids",
                "End": "ending_ids",
            }.get(scene)
            items = models.list_items(self._editor_data, data_key) if data_key else []
            return self._combo_from_items(node, key, items, value)
        if kind == "story_ref":
            # end.next_script：包内剧情脚本 id 下拉（可编辑，允许指向未创建脚本）
            return self._combo_from_items(
                node, key, [(sid, sid) for sid in self._story_ids], value
            )
        if kind == "line":
            w = QLineEdit("" if value is None else str(value))
            if node.get("type") == "death" and key == "title":
                # 死亡文本两段式：短标题缺省「勝敗乃兵家常事」
                w.setPlaceholderText("缺省「勝敗乃兵家常事」")
            elif node.get("type") == "goto_scene" and key == "title":
                w.setPlaceholderText("例如：浪迹江湖")
            elif node.get("type") == "intro" and key == "title":
                w.setPlaceholderText("例如：无名游侠（可选）")
            elif node.get("type") == "intro" and key == "name":
                w.setPlaceholderText("填写人物姓名")
            w.textChanged.connect(lambda t: self._apply(node, key, t))
            return w
        if kind == "affinity_character":
            w = self._make_combo(
                models.affinity_character_items(self._editor_data),
                value or "",
                editable=False,
            )
            w.currentTextChanged.connect(
                lambda t, c=w: self._apply(node, key, self._combo_value(c, t))
            )
            return w
        if kind in ("ending_image", "intro_image"):
            placeholder = (
                "assets/ending.png（可选；留空时游戏内用原版图占位）"
                if kind == "ending_image"
                else "选择 PNG/JPG 人物图片（可选；建议透明背景立绘）"
            )
            return self._make_image_picker(node, key, value, placeholder)
        if kind == "multiline":
            w = QPlainTextEdit("" if value is None else str(value))
            if node.get("type") == "death" and key == "text":
                w.setPlaceholderText("填写死亡原因或人物最后的结局")
            elif node.get("type") == "goto_scene" and key == "desc":
                w.setPlaceholderText("填写汗青书或死亡画面的正文，可以换行")
            elif node.get("type") == "message":
                w.setPlaceholderText("系统提示文本（原文显示，不走本地化 key）")
            elif node.get("type") == "intro" and key == "text":
                w.setPlaceholderText("填写人物介绍，可以换行")
            else:
                w.setPlaceholderText("对话/独白/旁白文本")
            w.textChanged.connect(lambda: self._apply(node, key, w.toPlainText()))
            return w
        if kind == "code":
            # raw 节点：大号等宽多行编辑框
            w = QPlainTextEdit("" if value is None else str(value))
            w.setPlaceholderText("原生 Lua 代码，原样插入编译产物（多行合法）")
            w.setMinimumHeight(180)
            w.setProperty("code_edit", True)  # 测试/调试定位用
            font = QFontDatabase.systemFont(QFontDatabase.SystemFont.FixedFont)
            font.setPointSize(10)
            w.setFont(font)
            w.textChanged.connect(lambda: self._apply(node, key, w.toPlainText()))
            return w
        if kind == "int":
            w = QSpinBox()
            w.setRange(-999999, 999999)
            try:
                w.setValue(int(value or 0))
            except (TypeError, ValueError):
                w.setValue(0)
            w.valueChanged.connect(
                lambda v: self._apply(node, key, v)
            )  # QSpinBox 发射 int
            return w
        if kind == "float":
            w = QDoubleSpinBox()
            w.setRange(-9999, 9999)
            w.setDecimals(2)
            w.setSingleStep(0.1)
            try:
                w.setValue(float(value or 0))
            except (TypeError, ValueError):
                w.setValue(0)
            w.valueChanged.connect(lambda v: self._apply(node, key, v))
            return w
        if kind == "percent_scale":
            w = QSpinBox()
            w.setRange(40, 160)
            w.setSingleStep(5)
            w.setSuffix(" %")
            w.setValue(int(value if value is not None else 100))
            w.setToolTip("100% 是自动适配后的推荐大小；可在 40%～160% 间调整")
            w.valueChanged.connect(lambda v: self._apply(node, key, int(v)))
            return w
        if kind == "percent_offset":
            w = QSpinBox()
            w.setRange(-30, 30)
            w.setSingleStep(1)
            w.setSuffix(" %")
            w.setValue(int(value or 0))
            w.setToolTip("相对屏幕位置微调；正数向右或向上，负数向左或向下")
            w.valueChanged.connect(lambda v: self._apply(node, key, int(v)))
            return w
        if kind == "bool":
            w = QCheckBox("是")
            w.setChecked(bool(value))
            w.toggled.connect(lambda b: self._apply(node, key, bool(b)))
            return w
        if kind == "options":
            return self._make_goto_table(
                node, key, columns=("text", "goto"), min_rows=2, max_rows=4
            )
        if kind == "cases":
            return self._make_branch_cases_table(node, key)
        if kind == "vars":
            return self._make_vars_table(node, key)
        if kind == "dice_options":
            return self._make_dice_table(node, key)
        return QLabel(f"（暂不支持的字段类型 {kind}）")

    # ------------------------------------------------------------ 基础控件
    def _make_combo(
        self, items: list[tuple[str, str]], current: str, editable: bool = False
    ) -> QComboBox:
        """items 为 (值, 显示文本)；可编辑下拉框允许填入清单外的值。"""
        w = QComboBox()
        w.setEditable(editable)
        for val, text in items:
            w.addItem(text, val)
        idx = w.findData(current)
        if idx >= 0:
            w.setCurrentIndex(idx)
        else:
            w.setCurrentText(current)
        return w

    def _make_image_picker(
        self, node: dict, key: str, value, placeholder: str
    ) -> QWidget:
        """图片路径输入框 + 新手友好的文件选择；选中后托管到 AppData。"""
        box = QWidget()
        row = QHBoxLayout(box)
        row.setContentsMargins(0, 0, 0, 0)
        edit = QLineEdit("" if value is None else str(value))
        edit.setPlaceholderText(placeholder)
        choose = QPushButton("选择图片…")
        choose.setMinimumHeight(28)
        edit.textChanged.connect(lambda text: self._apply(node, key, text))

        def pick() -> None:
            path, _ = QFileDialog.getOpenFileName(
                self,
                "选择人物图片" if node.get("type") == "intro" else "选择结局插图",
                str(Path.home()),
                "图片 (*.png *.jpg *.jpeg)",
            )
            if not path:
                return
            try:
                relative, _stored = import_image_file(Path(path))
            except AssetStoreError as exc:
                QMessageBox.critical(self, "无法使用图片", str(exc))
                return
            edit.setText(relative)

        choose.clicked.connect(pick)
        row.addWidget(edit, 1)
        row.addWidget(choose)
        return box

    def _make_audio_combo(
        self, node: dict, key: str, current, audio_kind: str
    ) -> QWidget:
        """官方 / 用户内容分组；空列表时仍可下拉，并带导入按钮。"""
        user_items: list[tuple[str, str]] = []
        try:
            for rec in content_registry.list_contents(
                content_type="audio", audio_kind=audio_kind
            ):
                user_items.append((rec.ref, "用户 · %s（%s）" % (rec.name, rec.ref)))
        except Exception:
            user_items = []
        official_items: list[tuple[str, str]] = []
        if audio_kind == "music":
            official_items = [
                (item_id, "官方 · %s" % display)
                for item_id, display in models.list_items(self._editor_data, "music")
            ]
        items: list[tuple[str, str]] = []
        if user_items:
            items.extend(user_items)
        elif audio_kind != "music":
            items.append(("", "（还没有自定义音效，点右侧导入）"))
        items.extend(official_items)
        w = self._make_combo(items, str(current or ""), editable=True)
        if user_items and official_items:
            w.insertSeparator(len(user_items))
        if not user_items and audio_kind != "music":
            model = w.model()
            if model is not None and model.rowCount() > 0:
                item = model.item(0)
                if item is not None:
                    item.setEnabled(False)
        if audio_kind != "music" and w.lineEdit() is not None:
            if not current:
                w.setCurrentText("")
            w.lineEdit().setPlaceholderText(
                "选择用户内容，或手填官方音效名（例如 鈴鐺_001）"
            )
        w.currentTextChanged.connect(
            lambda t, c=w: self._apply(node, key, self._combo_value(c, t))
        )

        box = QWidget()
        row = QHBoxLayout(box)
        row.setContentsMargins(0, 0, 0, 0)
        import_btn = QPushButton("导入…")
        import_btn.setMinimumHeight(28)
        import_btn.setToolTip("导入本地 ogg/wav 到用户内容库，并填入此步骤")

        def pick() -> None:
            path, _ = QFileDialog.getOpenFileName(
                self,
                "选择音频",
                str(Path.home()),
                "音频 (*.ogg *.wav)",
            )
            if not path:
                return
            try:
                rec = content_registry.register_audio(
                    Path(path),
                    content_registry.suggest_content_id(Path(path).name),
                    Path(path).stem,
                    audio_kind,
                )
            except content_registry.ContentRegistryError as exc:
                QMessageBox.critical(self, "无法导入音频", str(exc))
                return
            self._apply(node, key, rec.ref)
            self._rebuild_current()

        import_btn.clicked.connect(pick)
        row.addWidget(w, 1)
        row.addWidget(import_btn)
        return box

    def _make_voice_picker(self, node: dict, key: str, current) -> QWidget:
        """对白语音：只列用户音频；空表示本句不配音。"""
        user_items: list[tuple[str, str]] = [("", "（无语音）")]
        try:
            for rec in content_registry.list_contents(content_type="audio"):
                kind_cn = {"music": "音乐", "sound": "音效", "env": "环境音"}.get(
                    rec.audio_kind or "", rec.audio_kind or ""
                )
                user_items.append(
                    (rec.ref, "%s · %s（%s）" % (kind_cn, rec.name, rec.ref))
                )
        except Exception:
            pass
        w = self._make_combo(user_items, str(current or ""), editable=True)
        if w.lineEdit() is not None:
            w.lineEdit().setPlaceholderText("选择用户内容库里的音频，或不选")
        w.currentTextChanged.connect(
            lambda t, c=w: self._apply(
                node, key, self._combo_value(c, t).strip() or None
            )
        )
        box = QWidget()
        row = QHBoxLayout(box)
        row.setContentsMargins(0, 0, 0, 0)
        import_btn = QPushButton("导入…")
        clear_btn = QPushButton("清除")
        import_btn.setMinimumHeight(28)
        clear_btn.setMinimumHeight(28)
        import_btn.setToolTip("导入本地 ogg/wav 并绑到这句对白")
        clear_btn.setToolTip("这句对白不再播放语音")

        def pick() -> None:
            path, _ = QFileDialog.getOpenFileName(
                self,
                "选择对白语音",
                str(Path.home()),
                "音频 (*.ogg *.wav)",
            )
            if not path:
                return
            try:
                rec = content_registry.register_audio(
                    Path(path),
                    content_registry.suggest_content_id(Path(path).name),
                    Path(path).stem,
                    "sound",
                )
            except content_registry.ContentRegistryError as exc:
                QMessageBox.critical(self, "无法导入音频", str(exc))
                return
            self._apply(node, key, rec.ref)
            self._rebuild_current()

        def clear() -> None:
            self._apply(node, key, None)
            self._rebuild_current()

        import_btn.clicked.connect(pick)
        clear_btn.clicked.connect(clear)
        row.addWidget(w, 1)
        row.addWidget(import_btn)
        row.addWidget(clear_btn)
        return box

    def _list_combo(self, node: dict, key: str, data_key: str, current) -> QComboBox:
        """schema 2 清单下拉框（{id,name} 显示 "名字（id）"，可编辑容错）。"""
        return self._combo_from_items(
            node, key, models.list_items(self._editor_data, data_key), current
        )

    def _combo_from_items(
        self, node: dict, key: str, items: list[tuple[str, str]], current
    ) -> QComboBox:
        w = self._make_combo(items, str(current or ""), editable=True)
        w.currentTextChanged.connect(
            lambda t, c=w: self._apply(node, key, self._combo_value(c, t))
        )
        return w

    def _rebuild_current(self) -> None:
        """延迟重建表单（回到事件循环后执行）。

        枚举/来源下拉在自己的信号里不能直接 set_node——setWidget 会立即删除
        正在发信号的控件（use-after-free 段错误），必须延迟到信号返回后。
        """
        QTimer.singleShot(0, lambda: self.set_node(self._node))

    def _on_enum_changed(
        self, node: dict, key: str, set_name: str, combo: QComboBox
    ) -> None:
        """固定枚举写回；item.kind / goto_scene.scene 等切换后重建表单刷新联动清单。"""
        if self._loading:
            return
        val = combo.currentData()
        if val is None:
            val = combo.currentText()
        if val == node.get(key):
            return
        node[key] = val
        if set_name in models.REBUILD_ENUMS:
            self._rebuild_current()
        self._emit_changed()

    def _make_death_id_widget(self, node: dict, key: str, value) -> QWidget:
        """death.death_id：mod 专属死亡画面 id（9+官方 id，如 910021）。

        输入框 + 官方参考只读标签：官方 id 仅供查死亡画面标题参考，
        直接用官方 id 会触发官方结局解锁与记录（污染玩家存档）。
        """
        box = QWidget()
        v = QVBoxLayout(box)
        v.setContentsMargins(0, 0, 0, 0)
        w = QLineEdit("910021" if value in (None, "") else str(value))
        official = models.list_items(self._editor_data, "death_ids")
        ref = "　".join("%s %s" % (i, n) for i, n in official[:5])
        w.setToolTip(
            "mod 专属死亡画面 id：官方 id + 前綴 9（官方 10021 → 910021），"
            "保证与官方 1xxxx/2xxxx/4xxxx 不撞。官方 id 仅供参考（用官方 id 会"
            "触发官方结局解锁与记录，污染存档）。官方参考：%s" % (ref or "无")
        )
        w.setPlaceholderText("910021")
        w.textChanged.connect(lambda t: self._apply(node, key, t))
        v.addWidget(w)
        if official:
            label = QLabel(
                "官方参考：" + " / ".join("%s %s" % (i, n) for i, n in official[:5])
            )
            label.setWordWrap(True)
            # 玻璃主题的次要文字色（原 gray 在深色背景下偏暗）
            label.setStyleSheet("color: rgba(242, 242, 247, 160);")
            v.addWidget(label)
        return box

    def _on_dice_check_changed(
        self, node: dict, key: str, combo: QComboBox, text: str
    ) -> None:
        """骰子检查点变化：写回并重建表单（结果带数变化 → band_texts 行数/提示刷新）。"""
        if self._loading:
            return
        val = self._combo_value(combo, text)
        if val == node.get(key):
            return
        self._apply(node, key, val)
        self._rebuild_current()
        self._emit_changed()

    def _make_goto_combo(self, current: str, allow_empty: bool = True) -> QComboBox:
        """goto 目标：节点 id 下拉框（可编辑，允许指向尚未创建的节点）。"""
        items = [("", "（无）")] if allow_empty else []
        items += [(nid, nid) for nid in self._node_ids]
        return self._make_combo(items, current or "", editable=True)

    @staticmethod
    def _combo_value(combo: QComboBox, text: str) -> str:
        """可编辑下拉框取值：选中清单项时取 userData，手输时取文本。"""
        data = combo.currentData()
        return str(data) if data is not None else text

    def _make_goto_table(
        self,
        node: dict,
        key: str,
        columns: tuple[str, str],
        min_rows: int,
        max_rows: int | None,
    ) -> QWidget:
        """choice.options / branch.cases 的表格编辑器。"""
        rows: list[dict] = node.setdefault(key, [])
        while len(rows) < min_rows:  # 契约下限：options≥2、cases≥1
            rows.append(
                {"text": "", "goto": ""}
                if key == "options"
                else {"value": len(rows) + 1, "goto": ""}
            )

        box = QWidget()
        v = QVBoxLayout(box)
        v.setContentsMargins(0, 0, 0, 0)
        table = QTableWidget(len(rows), 2)
        table.setHorizontalHeaderLabels(
            ["选项文本", "goto 目标"]
            if key == "options"
            else ["分支值(value)", "goto 目标"]
        )
        table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        table.horizontalHeader().setSectionResizeMode(
            1, QHeaderView.ResizeMode.ResizeToContents
        )
        table.setMinimumHeight(min(4, max(2, len(rows))) * 32 + 30)

        def fill():
            table.setRowCount(0)
            table.blockSignals(True)
            try:
                for r, row in enumerate(rows):
                    table.insertRow(r)
                    if key == "options":
                        cell = QTableWidgetItem(str(row.get("text", "")))
                        table.setItem(r, 0, cell)
                    elif node.get("source", "mod") == "mod":
                        # mod 模式：value 列用下拉框（1=已设置 / 2=未设置）
                        val = row.get("value", 1)
                        if val not in (1, 2):
                            row["value"] = val = 1  # 容忍旧数据：非法值归一
                        cb = self._make_combo(
                            [(str(v), cn) for v, cn in models.BRANCH_MOD_VALUES],
                            str(val),
                        )
                        cb.currentTextChanged.connect(
                            lambda _t, row=row, c=cb: self._apply_row(
                                row, "value", int(c.currentData())
                            )
                        )
                        table.setCellWidget(r, 0, cb)
                    else:
                        # game 模式：value 是官方 Switch 数值返回值，保持数值输入
                        sp = QSpinBox()
                        sp.setRange(-999999, 999999)
                        sp.setValue(int(row.get("value", 0)))
                        sp.valueChanged.connect(
                            lambda val, row=row: self._apply_row(row, "value", int(val))
                        )
                        table.setCellWidget(r, 0, sp)
                    combo = self._make_goto_combo(
                        str(row.get("goto", "")), allow_empty=True
                    )
                    combo.currentTextChanged.connect(
                        lambda t, row=row, c=combo: self._apply_row(
                            row, "goto", self._combo_value(c, t).strip()
                        )
                    )
                    table.setCellWidget(r, 1, combo)
            finally:
                table.blockSignals(False)

        fill()
        table.itemChanged.connect(self._on_table_item)
        table.setProperty("rows_key", key)
        table.setProperty("rows_ref", id(rows))

        btns = QHBoxLayout()
        add = QPushButton("添加行")
        remove = QPushButton("删除末行")
        btns.addWidget(add)
        btns.addWidget(remove)
        btns.addStretch(1)

        def on_add():
            if max_rows is not None and len(rows) >= max_rows:
                return
            rows.append(
                {"text": "", "goto": ""}
                if key == "options"
                else {"value": len(rows) + 1, "goto": ""}
            )
            fill()
            self._emit_changed()

        def on_remove():
            if len(rows) <= min_rows:
                return
            rows.pop()
            fill()
            self._emit_changed()

        add.clicked.connect(on_add)
        remove.clicked.connect(on_remove)
        v.addWidget(table)
        v.addLayout(btns)
        return box

    def _make_branch_cases_table(self, node: dict, key: str) -> QWidget:
        """branch.cases 表格：列布局随 source 动态切换（契约 §3.1）。

        - mod：value 列 1/2 下拉（已设置/未设置），最多两行
        - condition：value 列 1/2 下拉（真/假），最多两行
        - game：value 列整数 spinbox（官方 Switch 数值返回值）
        - stat/flag_value：op 下拉（>=/>/<=/</==）+ value 整数 spinbox
        """
        rows: list[dict] = node.setdefault(key, [])
        if not rows:
            rows.append({"value": 1, "goto": ""})
        source = node.get("source", "mod")
        numeric = source in ("stat", "flag_value", "game")
        with_op = source in ("stat", "flag_value")
        two_value = source in ("mod", "condition")
        max_rows = 2 if two_value else None

        def new_row() -> dict:
            row: dict = {"value": len(rows) + 1, "goto": ""}
            if with_op:
                row["op"] = ">="
            return row

        def norm_value(row: dict):
            # 归一旧数据：mod/condition 只允许 1/2
            v = row.get("value")
            if two_value and v not in (1, 2):
                row["value"] = 1
            if with_op and row.get("op") not in (">=", ">", "<=", "<", "=="):
                row["op"] = ">="

        box = QWidget()
        v = QVBoxLayout(box)
        v.setContentsMargins(0, 0, 0, 0)
        n_cols = 3 if with_op else 2
        table = QTableWidget(len(rows), n_cols)
        headers = (
            ["运算符", "数值", "goto 目标"]
            if with_op
            else (
                ["分支值(value)", "goto 目标"]
                if numeric
                else ["真/假(value)", "goto 目标"]
            )
        )
        table.setHorizontalHeaderLabels(headers)
        table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        for c in range(1, n_cols):
            table.horizontalHeader().setSectionResizeMode(
                c, QHeaderView.ResizeMode.ResizeToContents
            )
        table.setMinimumHeight(min(4, max(2, len(rows))) * 32 + 30)

        def fill():
            table.setRowCount(0)
            table.blockSignals(True)
            try:
                for r, row in enumerate(rows):
                    norm_value(row)
                    table.insertRow(r)
                    if with_op:
                        cb = self._make_combo(
                            list(models.BRANCH_OPS), str(row.get("op", ">="))
                        )
                        cb.currentTextChanged.connect(
                            lambda _t, row=row, c=cb: self._apply_row(
                                row, "op", str(c.currentData() or c.currentText())
                            )
                        )
                        table.setCellWidget(r, 0, cb)
                    elif two_value:
                        items = (
                            models.BRANCH_MOD_VALUES
                            if source == "mod"
                            else models.BRANCH_COND_VALUES
                        )
                        cb = self._make_combo(
                            [(str(val), cn) for val, cn in items], str(row["value"])
                        )
                        cb.currentTextChanged.connect(
                            lambda _t, row=row, c=cb: self._apply_row(
                                row, "value", int(c.currentData())
                            )
                        )
                        table.setCellWidget(r, 0, cb)
                    else:
                        sp = QSpinBox()
                        sp.setRange(-999999, 999999)
                        sp.setValue(int(row.get("value", 0)))
                        sp.valueChanged.connect(
                            lambda val, row=row: self._apply_row(row, "value", int(val))
                        )
                        table.setCellWidget(r, 0, sp)
                    if with_op:
                        sp = QSpinBox()
                        sp.setRange(-999999, 999999)
                        sp.setValue(int(row.get("value", 0)))
                        sp.valueChanged.connect(
                            lambda val, row=row: self._apply_row(row, "value", int(val))
                        )
                        table.setCellWidget(r, 1, sp)
                    combo = self._make_goto_combo(
                        str(row.get("goto", "")), allow_empty=True
                    )
                    combo.currentTextChanged.connect(
                        lambda t, row=row, c=combo: self._apply_row(
                            row, "goto", self._combo_value(c, t).strip()
                        )
                    )
                    table.setCellWidget(r, n_cols - 1, combo)
            finally:
                table.blockSignals(False)

        fill()
        v.addWidget(table)
        v.addLayout(
            self._make_row_buttons(
                rows, fill, min_rows=1, max_rows=max_rows, new_row=new_row
            )
        )
        return box

    def _make_row_buttons(
        self, rows: list, fill, min_rows: int, max_rows: int | None, new_row
    ) -> QHBoxLayout:
        """表格通用的 添加行/删除末行 按钮行。"""
        btns = QHBoxLayout()
        add = QPushButton("添加行")
        remove = QPushButton("删除末行")
        btns.addWidget(add)
        btns.addWidget(remove)
        btns.addStretch(1)

        def on_add():
            if max_rows is not None and len(rows) >= max_rows:
                return
            rows.append(new_row())
            fill()
            self._emit_changed()

        def on_remove():
            if len(rows) <= min_rows:
                return
            rows.pop()
            fill()
            self._emit_changed()

        add.clicked.connect(on_add)
        remove.clicked.connect(on_remove)
        return btns

    def _make_vars_table(self, node: dict, key: str) -> QWidget:
        """block.vars：{name, value} 两列文本表格。"""
        rows: list[dict] = node.setdefault(key, [])
        box = QWidget()
        v = QVBoxLayout(box)
        v.setContentsMargins(0, 0, 0, 0)
        table = QTableWidget(0, 2)
        table.setHorizontalHeaderLabels(["变量名", "值"])
        table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        table.setMinimumHeight(min(4, max(2, len(rows) or 2)) * 32 + 30)

        def fill():
            table.setRowCount(0)
            table.blockSignals(True)
            try:
                for r, row in enumerate(rows):
                    table.insertRow(r)
                    table.setItem(r, 0, QTableWidgetItem(str(row.get("name", ""))))
                    table.setItem(r, 1, QTableWidgetItem(str(row.get("value", ""))))
            finally:
                table.blockSignals(False)

        def on_item(item: QTableWidgetItem):
            if self._loading or not (0 <= item.row() < len(rows)):
                return
            rows[item.row()]["name" if item.column() == 0 else "value"] = item.text()
            self._emit_changed()

        fill()
        table.itemChanged.connect(on_item)
        v.addWidget(table)
        v.addLayout(
            self._make_row_buttons(
                rows,
                fill,
                min_rows=0,
                max_rows=None,
                new_row=lambda: {"name": "", "value": ""},
            )
        )
        return box

    def _make_dice_table(self, node: dict, key: str) -> QWidget:
        """dice.options：选项文本覆写（band_texts）+ 大成功/成功/失败 三列 goto。

        骰子范围与结果带文本/条件来自 data/editor_data.json 的 dice_meta。
        band_texts（可选）：按当前检查点带数显示 N 个文本框，逐带覆写骰子
        菜单选项文本（留空用官方结果带文本）；全空时不写该字段。
        """
        rows: list[dict] = node.setdefault(key, [])
        if not rows:
            rows.append({"goto_大成功": "", "goto_成功": "", "goto_失败": ""})
        meta = (self._editor_data.get("dice_meta") or {}).get(
            str(node.get("check", "")), {}
        )
        bands = meta.get("bands") or []
        opt0 = rows[0]
        box = QWidget()
        v = QVBoxLayout(box)
        v.setContentsMargins(0, 0, 0, 0)

        if bands:
            v.addWidget(QLabel("选项文本（可选，留空用官方文本）："))
            current = opt0.get("band_texts") or []
            for i, band in enumerate(bands):
                txt = (
                    current[i]
                    if i < len(current) and isinstance(current[i], str)
                    else ""
                )
                le = QLineEdit(txt)
                le.setPlaceholderText("带%d 官方：%s" % (i + 1, band.get("text", "")))
                le.textChanged.connect(
                    lambda t, idx=i, c=le: self._apply_band_text(idx, t, c)
                )
                v.addWidget(le)

        table = QTableWidget(0, 3)
        table.setHorizontalHeaderLabels(
            ["失败 goto（最差带）", "成功 goto", "大成功 goto（3带检查点最优带）"]
        )
        for c in range(3):
            table.horizontalHeader().setSectionResizeMode(
                c, QHeaderView.ResizeMode.Stretch
            )
        table.setMinimumHeight(min(4, max(2, len(rows))) * 32 + 30)

        def fill():
            table.setRowCount(0)
            table.blockSignals(True)
            try:
                for r, row in enumerate(rows):
                    table.insertRow(r)
                    for c, gkey in enumerate(("goto_失败", "goto_成功", "goto_大成功")):
                        combo = self._make_goto_combo(
                            str(row.get(gkey, "")), allow_empty=True
                        )
                        combo.currentTextChanged.connect(
                            lambda t, row=row, c=combo, g=gkey: self._apply_row(
                                row, g, self._combo_value(c, t).strip()
                            )
                        )
                        table.setCellWidget(r, c, combo)
            finally:
                table.blockSignals(False)

        fill()
        v.addWidget(table)
        # 检查点元数据提示行（官方结果带：差→好）
        if bands:
            hint = "　".join(
                "带%d: %s｜%s" % (i, b.get("text", ""), b.get("cond", ""))
                for i, b in enumerate(bands, 1)
            )
            hint_label = QLabel(
                "官方结果带（骰子 0~%s）：%s" % (meta.get("max", "?"), hint)
            )
            hint_label.setWordWrap(True)
            v.addWidget(hint_label)
        v.addLayout(
            self._make_row_buttons(
                rows,
                fill,
                min_rows=1,
                max_rows=1,
                new_row=lambda: {"goto_大成功": "", "goto_成功": "", "goto_失败": ""},
            )
        )
        return box

    def _apply_band_text(self, index: int, text: str, edit: QLineEdit) -> None:
        """band_texts 写回：任一非空才写字段（全空则不写）；条目数补齐到带数。

        空条目由编译器校验报错（非空 str），作者要么全填要么全空。
        """
        if self._loading or not self._node:
            return
        node = self._node
        options = node.get("options") or []
        if not options or not isinstance(options[0], dict):
            options[:] = [{}]
        opt0 = options[0]
        meta = (self._editor_data.get("dice_meta") or {}).get(
            str(node.get("check", "")), {}
        )
        n_bands = len(meta.get("bands") or [])
        cur = list(opt0.get("band_texts") or [])
        while len(cur) <= index:
            cur.append("")
        cur[index] = text
        while len(cur) < n_bands:
            cur.append("")
        if any(t.strip() for t in cur):
            opt0["band_texts"] = cur
        else:
            opt0.pop("band_texts", None)
        self._emit_changed()

    # ------------------------------------------------------------------ 写回
    def _on_table_item(self, item: QTableWidgetItem) -> None:
        """options 表格的文本列写回。"""
        if self._loading or self._node is None or item.column() != 0:
            return
        table = item.tableWidget()
        if table.property("rows_key") != "options":
            return
        rows = self._node.get("options", [])
        if 0 <= item.row() < len(rows):
            rows[item.row()]["text"] = item.text()
            self._emit_changed()

    def _apply(self, node: dict, key: str, value) -> None:
        if self._loading:
            return
        if value is None:
            node.pop(key, None)  # 可选字段置空时不写出
        else:
            node[key] = value
        self._emit_changed()

    def _apply_row(self, row: dict, key: str, value) -> None:
        if self._loading:
            return
        row[key] = value
        self._emit_changed()

    def _on_character_changed(
        self, node: dict, key: str, combo: QComboBox, text: str
    ) -> None:
        """人物变化：写回，并就地刷新同节点内表情下拉框的清单（不重建表单，避免打断输入）。"""
        # 经 Python lambda 转发信号时 QObject.sender() 不可靠，必须显式传入
        # combo；否则会把“鸡（chicken1）”这类显示文本写进 JSON，游戏加载失败。
        char_id = self._combo_value(combo, text)
        if char_id == node.get(key):
            return
        self._apply(node, key, char_id)
        portraits = models.character_portraits(self._editor_data, char_id)
        for combo in self.findChildren(QComboBox):
            pkey = combo.property("portrait_for")
            if not pkey:
                continue
            cur = combo.currentText()
            new_p = (
                cur if cur in portraits else (portraits[0] if portraits else "normal")
            )
            combo.blockSignals(True)
            combo.clear()
            for p in portraits:
                combo.addItem(p, p)
            combo.setCurrentText(new_p)
            combo.blockSignals(False)
            if node.get(pkey) != new_p:
                self._apply(node, pkey, new_p)

    def _on_source_changed(self, node: dict, key: str, combo: QComboBox) -> None:
        """branch.source 切换：写回、归一 cases 并重建表单（列布局随来源切换）。"""
        if self._loading:
            return
        src = combo.currentData() or combo.currentText()
        if src == node.get(key, "mod"):
            return
        node[key] = src
        if src in ("mod", "condition"):
            # 契约：mod/condition 的 value 只能 1/2、最多两行，丢弃其它取值
            seen: set[int] = set()
            norm: list[dict] = []
            for c in node.get("cases", []):
                v = c.get("value")
                c.pop("op", None)  # 这两类来源没有 op 字段
                if v in (1, 2) and v not in seen:
                    seen.add(v)
                    norm.append(c)
            node["cases"] = (norm or [{"value": 1, "goto": ""}])[:2]
        elif src in ("stat", "flag_value"):
            # 数值比较来源：case 带 op（缺省 >=）；value 保持整数
            for c in node.get("cases", []):
                if c.get("op") not in (">=", ">", "<=", "<", "=="):
                    c["op"] = ">="
            if not node.get("cases"):
                node["cases"] = [{"op": ">=", "value": 0, "goto": ""}]
        elif src == "game":
            for c in node.get("cases", []):
                c.pop("op", None)
        self._rebuild_current()  # 延迟重建，避免删除正在发信号的控件
        self._emit_changed()

    def _emit_changed(self) -> None:
        if not self._loading:
            self.node_changed.emit()
