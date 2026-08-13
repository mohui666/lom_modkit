# -*- coding: utf-8 -*-
"""节点属性表单：按节点 type 依据 models.NODE_SCHEMAS 动态生成控件。

- 人物/表情/站位/场景/音乐/stat key/mode/朝向 → 下拉框（数据来自 editor_data）
- text → 多行编辑框；整数/小数 → spinbox；bool → 勾选框
- choice.options / branch.cases → 可增删行的表格，goto 列为节点 id 下拉框
任何编辑都会把值写回节点 dict 并发出 node_changed 信号。
"""

from __future__ import annotations

from PySide6.QtCore import QTimer, Signal
from PySide6.QtGui import QFontDatabase
from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDoubleSpinBox,
    QFormLayout,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QPlainTextEdit,
    QPushButton,
    QScrollArea,
    QSpinBox,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

import models


class NodeForm(QScrollArea):
    """中栏：单个节点的属性编辑表单。"""

    node_changed = Signal()  # 当前节点内容被用户修改

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

    def set_node(self, node: dict | None) -> None:
        """展示并编辑给定节点；None 时清空。"""
        self._loading = True
        try:
            self._node = node
            body = QWidget()
            layout = QVBoxLayout(body)
            if node is None:
                layout.addWidget(QLabel("← 在左侧选择一个节点"))
                layout.addStretch(1)
            else:
                layout.addLayout(self._build_form(node))
                layout.addStretch(1)
            self.setWidget(body)
        finally:
            self._loading = False

    # ------------------------------------------------------------------ 构建
    def _build_form(self, node: dict) -> QFormLayout:
        form = QFormLayout()
        node_type = node.get("type", "")
        schema = models.NODE_SCHEMAS.get(node_type)

        type_cn = models.NODE_TYPE_CN.get(node_type, node_type)
        head = QLabel(f"<b>{node.get('id', '')}</b>　类型：{type_cn}（{node_type}）")
        form.addRow(head)

        if schema is None:
            form.addRow(QLabel("未知节点类型，无法编辑"))
            return form

        for key, label, kind, optional in schema["fields"]:
            widget = self._make_widget(node, key, kind)
            form.addRow(label + ("（可选）" if optional else ""), widget)

        # choice/branch/dice/end/goto_scene 之外的节点允许显式 goto 覆盖顺序流（契约 §3/§4）
        if node_type not in ("choice", "branch", "dice", "end", "goto_scene"):
            goto = self._make_goto_combo(node.get("goto", ""), allow_empty=True)
            goto.currentTextChanged.connect(
                lambda text, c=goto: self._apply(
                    node, "goto", self._combo_value(c, text).strip() or None
                )
            )
            form.addRow("goto（可选）", goto)
        return form

    def _make_widget(self, node: dict, key: str, kind: str) -> QWidget:
        value = node.get(key)
        if kind == "character":
            w = self._make_combo(
                models.list_items(self._editor_data, "characters"),
                value or "",
                editable=True,
            )
            w.currentTextChanged.connect(
                lambda t: self._on_character_changed(node, key, t)
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
        if kind in ("position", "view", "music", "stat"):
            # schema 2 清单：{id,name} 对象数组，显示 "名字（id）"
            data_key = {
                "position": "positions",
                "view": "views",
                "music": "music",
                "stat": "stats",
            }[kind]
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
            return self._combo_from_items(node, key, [("Options", "Options")], value or "Options")
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
            return self._combo_from_items(
                node, key, models.dice_check_items(self._editor_data), value
            )
        if kind == "item":
            # 物品清单随 kind 字段（book/misc/special）切换；切换时表单已重建
            data_key = f"items_{node.get('kind', 'misc')}"
            return self._combo_from_items(
                node, key, models.list_items(self._editor_data, data_key), value
            )
        if kind == "goto_scene_key":
            # 场景参数清单随 scene 字段切换（战斗/战役/结局 id）
            scene = node.get("scene", "Free")
            data_key = {
                "Combat": "combat_ids",
                "Battle": "battle_ids",
                "GameOver": "ending_ids",
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
            w.textChanged.connect(lambda t: self._apply(node, key, t))
            return w
        if kind == "multiline":
            w = QPlainTextEdit("" if value is None else str(value))
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
            # source=mod 时 value 只能 1/2 且最多两行（契约 §3.1）
            mod_mode = node.get("source", "mod") == "mod"
            return self._make_goto_table(
                node,
                key,
                columns=("value", "goto"),
                min_rows=1,
                max_rows=2 if mod_mode else None,
            )
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
        """dice.options：大成功/成功/失败 三列 goto（结果带按检查点官方元数据发射）。

        骰子范围与结果带文本/条件来自 data/editor_data.json 的 dice_meta，
        表单底部展示所选检查点的结果带供作者参考。
        """
        rows: list[dict] = node.setdefault(key, [])
        if not rows:
            rows.append({"goto_大成功": "", "goto_成功": "", "goto_失败": ""})
        box = QWidget()
        v = QVBoxLayout(box)
        v.setContentsMargins(0, 0, 0, 0)
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
        meta = (self._editor_data.get("dice_meta") or {}).get(
            str(node.get("check", "")), {}
        )
        bands = meta.get("bands") or []
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

    def _on_character_changed(self, node: dict, key: str, text: str) -> None:
        """人物变化：写回，并就地刷新同节点内表情下拉框的清单（不重建表单，避免打断输入）。"""
        sender = self.sender()
        char_id = (
            self._combo_value(sender, text) if isinstance(sender, QComboBox) else text
        )
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
        """branch.source 切换：写回并重建表单（cases 的 value 列控件随模式切换）。"""
        if self._loading:
            return
        src = combo.currentData() or combo.currentText()
        if src == node.get(key, "mod"):
            return
        node[key] = src
        if src == "mod":
            # 契约：mod 模式下 value 只能 1/2、最多两行，丢弃其它取值
            seen: set[int] = set()
            norm: list[dict] = []
            for c in node.get("cases", []):
                v = c.get("value")
                if v in (1, 2) and v not in seen:
                    seen.add(v)
                    norm.append(c)
            node["cases"] = (norm or [{"value": 1, "goto": ""}])[:2]
        self._rebuild_current()  # 延迟重建，避免删除正在发信号的控件
        self._emit_changed()

    def _emit_changed(self) -> None:
        if not self._loading:
            self.node_changed.emit()
