# -*- coding: utf-8 -*-
"""“安装管理”对话框。"""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QAbstractItemView,
    QApplication,
    QDialog,
    QDialogButtonBox,
    QFileDialog,
    QGroupBox,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QMessageBox,
    QProgressDialog,
    QPushButton,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
)

from game_install import GameInstallError, GameInstallManager


def apply_steam_launch_fix_ui(parent, manager: GameInstallManager) -> None:
    """帮助 / 安装管理共用：把 Steam 普通启动修进当前游戏目录。"""
    try:
        actions = manager.apply_steam_launch_fix()
    except GameInstallError as exc:
        QMessageBox.warning(parent, "无法修复 Steam 启动", str(exc))
        return
    QMessageBox.information(
        parent,
        "Steam 启动修复已写入",
        "\n".join("· " + line for line in actions)
        + "\n\n请完全退出游戏后，从 Steam 普通点「开始」（不要管理员）。"
        "进游戏按 F8 打开 Mod 菜单。",
    )


class ModManagerDialog(QDialog):
    """配置一次游戏目录，之后自动安装运行时并管理 Mod 启停。"""

    def __init__(self, manager: GameInstallManager, parent=None):
        super().__init__(parent)
        self.manager = manager
        self._loading = False
        self.setWindowTitle("安装管理")
        self.resize(900, 600)

        layout = QVBoxLayout(self)
        intro = QLabel(
            "首次选择《活侠传》文件夹后，可一键安装 BepInEx 和 Mod 运行时。"
            "启用的 Mod 会在下次启动游戏时加载。"
        )
        intro.setWordWrap(True)
        layout.addWidget(intro)

        game_box = QGroupBox("游戏位置")
        game_layout = QVBoxLayout(game_box)
        path_row = QHBoxLayout()
        self.path_edit = QLineEdit()
        self.path_edit.setReadOnly(True)
        self.path_edit.setPlaceholderText("尚未选择游戏文件夹")
        choose_btn = QPushButton("选择游戏文件夹…")
        detect_btn = QPushButton("自动查找")
        choose_btn.clicked.connect(self._choose_game_dir)
        detect_btn.clicked.connect(self._detect_game_dir)
        path_row.addWidget(self.path_edit, 1)
        path_row.addWidget(choose_btn)
        path_row.addWidget(detect_btn)
        game_layout.addLayout(path_row)
        status_row = QHBoxLayout()
        self.status_label = QLabel("未配置")
        self.status_label.setWordWrap(True)
        self.install_btn = QPushButton("重新安装运行时")
        self.install_btn.clicked.connect(self._install_runtime)
        self.bepinex_btn = QPushButton("安装 BepInEx")
        self.bepinex_btn.setToolTip("从 BepInEx 官方下载站安装已验证的 Mono x86 版本")
        self.bepinex_btn.clicked.connect(self._install_bepinex)
        self.steam_fix_btn = QPushButton("修复 Steam 无法加载")
        self.steam_fix_btn.setToolTip(
            "Steam 普通启动时 Doorstop 可能被环境变量跳过。"
            "此按钮会改 doorstop 配置并把代理换成 version.dll。"
        )
        self.steam_fix_btn.clicked.connect(self._apply_steam_fix)
        status_row.addWidget(self.status_label, 1)
        status_row.addWidget(self.steam_fix_btn)
        status_row.addWidget(self.bepinex_btn)
        status_row.addWidget(self.install_btn)
        game_layout.addLayout(status_row)
        layout.addWidget(game_box)

        mods_box = QGroupBox("已安装 Mod")
        mods_layout = QVBoxLayout(mods_box)
        tip = QLabel("勾选表示启用；取消勾选会保留文件，但游戏不再加载。切换后请重启游戏。")
        tip.setWordWrap(True)
        mods_layout.addWidget(tip)
        self.table = QTableWidget(0, 5)
        self.table.setHorizontalHeaderLabels(["启用", "名称", "版本", "作者", "文件"])
        self.table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.table.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        self.table.verticalHeader().setVisible(False)
        self.table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        for col in (1, 3, 4):
            self.table.horizontalHeader().setSectionResizeMode(col, QHeaderView.ResizeMode.Stretch)
        self.table.horizontalHeader().setSectionResizeMode(2, QHeaderView.ResizeMode.ResizeToContents)
        self.table.itemChanged.connect(self._toggle_mod)
        mods_layout.addWidget(self.table, 1)
        action_row = QHBoxLayout()
        add_btn = QPushButton("安装已有 .lommod…")
        refresh_btn = QPushButton("刷新列表")
        add_btn.clicked.connect(self._add_mod)
        refresh_btn.clicked.connect(self.refresh)
        action_row.addWidget(add_btn)
        action_row.addWidget(refresh_btn)
        action_row.addStretch(1)
        mods_layout.addLayout(action_row)
        layout.addWidget(mods_box, 1)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        buttons.rejected.connect(self.reject)
        close_btn = buttons.button(QDialogButtonBox.StandardButton.Close)
        if close_btn is not None:
            close_btn.setText("完成")
        layout.addWidget(buttons)
        self.refresh()

    def _configured(self) -> bool:
        try:
            self.manager.require_game_dir()
            return True
        except GameInstallError:
            return False

    def refresh(self) -> None:
        self._loading = True
        try:
            root = self.manager.load_game_dir()
            self.path_edit.setText(str(root) if root else "")
            if not self._configured():
                self.status_label.setText(
                    "未连接：请选择包含 Mortal.exe 的《活侠传》游戏目录。"
                )
                self.bepinex_btn.setEnabled(False)
                self.bepinex_btn.setText("安装 BepInEx")
                self.steam_fix_btn.setEnabled(False)
                self.install_btn.setEnabled(False)
                self.table.setRowCount(0)
                return
            self.bepinex_btn.setEnabled(True)
            root = self.manager.require_game_dir()
            has_bepinex = self.manager.bepinex_installed(root)
            self.bepinex_btn.setText(
                "重新安装 / 更新 BepInEx" if has_bepinex else "安装 BepInEx"
            )
            self.steam_fix_btn.setEnabled(has_bepinex)
            self.install_btn.setEnabled(has_bepinex)
            if not has_bepinex:
                self.status_label.setText(
                    "已找到游戏；尚未安装兼容的 BepInEx。点击“安装 BepInEx”即可继续。"
                )
                self.table.setRowCount(0)
                return
            runtime_target = self.manager.plugin_dir() / "MortalModHost.dll"
            state = "运行时已安装" if runtime_target.is_file() else "运行时尚未安装"
            steam = (
                "Steam 普通启动修复已就绪"
                if self.manager.steam_launch_fix_applied(root)
                else "若 Steam 点开始没有 Mod，点“修复 Steam 无法加载”"
            )
            self.status_label.setText(f"已连接：{state}。{steam}。")
            records = self.manager.list_mods()
            self.table.setRowCount(len(records))
            for row, record in enumerate(records):
                enabled_item = QTableWidgetItem("启用" if record.enabled else "停用")
                enabled_item.setFlags(
                    Qt.ItemFlag.ItemIsEnabled
                    | Qt.ItemFlag.ItemIsSelectable
                    | Qt.ItemFlag.ItemIsUserCheckable
                )
                enabled_item.setCheckState(
                    Qt.CheckState.Checked if record.enabled else Qt.CheckState.Unchecked
                )
                enabled_item.setData(Qt.ItemDataRole.UserRole, str(record.path))
                self.table.setItem(row, 0, enabled_item)
                values = (record.name, record.version, record.author, record.path.name)
                for col, value in enumerate(values, 1):
                    item = QTableWidgetItem(value)
                    detail = record.error or record.description
                    if detail:
                        item.setToolTip(detail)
                    self.table.setItem(row, col, item)
                if record.error:
                    enabled_item.setText("损坏")
                    enabled_item.setFlags(Qt.ItemFlag.ItemIsEnabled | Qt.ItemFlag.ItemIsSelectable)
                    for col in range(5):
                        self.table.item(row, col).setToolTip(record.error)
        except GameInstallError as exc:
            self.status_label.setText(f"无法读取安装状态：{exc}")
            self.table.setRowCount(0)
        finally:
            self._loading = False

    def _choose_game_dir(self) -> None:
        current = self.manager.load_game_dir()
        path = QFileDialog.getExistingDirectory(
            self, "选择《活侠传》游戏文件夹", str(current or Path.home())
        )
        if path:
            self._configure(Path(path))

    def _detect_game_dir(self) -> None:
        found = self.manager.detect_game_dir()
        if found is None:
            QMessageBox.warning(
                self,
                "没有自动找到游戏",
                "请点击“选择游戏文件夹”，选择包含 Mortal.exe 的目录。",
            )
            return
        self._configure(found)

    def _configure(self, path: Path) -> None:
        try:
            self.manager.save_game_dir(path)
        except GameInstallError as exc:
            QMessageBox.critical(self, "无法连接游戏", str(exc))
            return
        if self.manager.bepinex_installed(path):
            try:
                target, changed = self.manager.install_runtime()
            except GameInstallError as exc:
                QMessageBox.critical(self, "运行时安装失败", str(exc))
                self.refresh()
                return
            QMessageBox.information(
                self,
                "游戏已连接",
                ("运行时已安装到：" if changed else "运行时已经是最新版：") + str(target),
            )
        else:
            QMessageBox.information(
                self,
                "已找到游戏",
                "游戏目录已经保存。下一步点击“安装 BepInEx”，编辑器会自动完成其余安装。",
            )
        self.refresh()

    def _install_bepinex(self) -> None:
        try:
            root = self.manager.require_game_dir()
            architecture = self.manager.game_architecture(root)
        except GameInstallError as exc:
            QMessageBox.critical(self, "无法安装 BepInEx", str(exc))
            return
        answer = QMessageBox.question(
            self,
            "安装 BepInEx",
            "将从 BepInEx 官方下载站安装已验证的 BepInEx 6 build 692 "
            f"（Unity Mono {architecture}）。\n\n"
            "不会删除已有配置、插件或 Mod。是否继续？",
        )
        if answer != QMessageBox.StandardButton.Yes:
            return

        dialog = QProgressDialog("正在准备下载…", "取消", 0, 0, self)
        dialog.setWindowTitle("安装 BepInEx")
        dialog.setWindowModality(Qt.WindowModality.WindowModal)
        dialog.setMinimumDuration(0)
        dialog.setAutoClose(False)
        dialog.show()

        def report(message: str, current: int, total: int) -> None:
            dialog.setLabelText(message)
            if total > 0:
                dialog.setRange(0, total)
                dialog.setValue(min(current, total))
            else:
                dialog.setRange(0, 0)
            QApplication.processEvents()
            if dialog.wasCanceled():
                raise GameInstallError("安装已取消，现有游戏文件没有被删除。")

        try:
            version, source = self.manager.install_bepinex(report)
            target, _changed = self.manager.install_runtime()
        except GameInstallError as exc:
            dialog.close()
            QMessageBox.critical(self, "BepInEx 安装失败", str(exc))
            self.refresh()
            return
        dialog.close()
        QMessageBox.information(
            self,
            "安装完成",
            f"已安装 BepInEx {version} 和 MortalModHost。\n"
            f"运行时位置：{target}\n\n官方来源：{source}\n\n"
            "已同时写入 Steam 普通启动修复（ignore_disable_switch + version.dll）。",
        )
        self.refresh()

    def _apply_steam_fix(self) -> None:
        apply_steam_launch_fix_ui(self, self.manager)
        self.refresh()

    def _install_runtime(self) -> None:
        try:
            target, changed = self.manager.install_runtime()
        except GameInstallError as exc:
            QMessageBox.critical(self, "安装失败", str(exc))
            return
        QMessageBox.information(
            self,
            "安装完成",
            ("已更新：" if changed else "无需更新，文件已经一致：") + str(target),
        )
        self.refresh()

    def _add_mod(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, "安装 .lommod", str(Path.home()), "活侠传 Mod (*.lommod)"
        )
        if not path:
            return
        try:
            target = self.manager.install_mod(Path(path), enabled=True)
        except GameInstallError as exc:
            QMessageBox.critical(self, "Mod 安装失败", str(exc))
            return
        QMessageBox.information(self, "Mod 已安装", f"已启用：{target.name}\n重启游戏后生效。")
        self.refresh()

    def _toggle_mod(self, item: QTableWidgetItem) -> None:
        if self._loading or item.column() != 0:
            return
        path_value = item.data(Qt.ItemDataRole.UserRole)
        if not path_value:
            return
        enabled = item.checkState() == Qt.CheckState.Checked
        try:
            self.manager.set_enabled(Path(str(path_value)), enabled)
        except GameInstallError as exc:
            QMessageBox.critical(self, "无法切换 Mod", str(exc))
        self.refresh()
