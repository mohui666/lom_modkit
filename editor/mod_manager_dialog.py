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
    QTextBrowser,
    QVBoxLayout,
)

from game_install import GameInstallError, GameInstallManager
from i18n import t


class RuntimeDoctorDialog(QDialog):
    """Read-only installation report with one explicit safe-repair action."""

    def __init__(self, manager: GameInstallManager, parent=None):
        super().__init__(parent)
        self.manager = manager
        self.setWindowTitle(t("doctor.title"))
        self.resize(820, 580)
        layout = QVBoxLayout(self)
        intro = QLabel(t("doctor.intro"))
        intro.setWordWrap(True)
        layout.addWidget(intro)
        self.report_view = QTextBrowser()
        self.report_view.setOpenExternalLinks(False)
        layout.addWidget(self.report_view, 1)
        action_row = QHBoxLayout()
        self.repair_btn = QPushButton(t("doctor.repair"))
        self.repair_btn.clicked.connect(self._repair)
        refresh_btn = QPushButton(t("doctor.refresh"))
        refresh_btn.clicked.connect(self.refresh)
        action_row.addWidget(self.repair_btn)
        action_row.addWidget(refresh_btn)
        action_row.addStretch(1)
        layout.addLayout(action_row)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)
        self.refresh()

    def refresh(self) -> None:
        report = self.manager.diagnose_installation()
        icons = {"ok": "✓", "warning": "!", "error": "×"}
        colors = {"ok": "#65c18c", "warning": "#e6b450", "error": "#ef6b73"}
        lines = []
        for item in report.findings:
            paths = "".join(
                "<br><code>%s</code>" % str(path).replace("&", "&amp;").replace("<", "&lt;")
                for path in item.paths
            )
            fix = " <b>%s</b>" % t("doctor.fixable") if item.fixable else ""
            lines.append(
                '<p><span style="color:%s"><b>%s %s</b></span>%s<br>%s%s</p>'
                % (colors[item.severity], icons[item.severity], item.title, fix,
                   item.detail.replace("&", "&amp;").replace("<", "&lt;"), paths)
            )
        self.report_view.setHtml("".join(lines))
        self.repair_btn.setEnabled(report.fixable_count > 0)
        self.repair_btn.setText(t("doctor.repair_count", count=report.fixable_count))

    def _repair(self) -> None:
        try:
            actions = self.manager.apply_installation_doctor_fixes()
        except GameInstallError as exc:
            QMessageBox.critical(self, t("doctor.repair_fail"), str(exc))
            self.refresh()
            return
        QMessageBox.information(
            self,
            t("doctor.repair_done"),
            "\n".join(actions) if actions else t("doctor.no_repairs"),
        )
        self.refresh()


def apply_steam_launch_fix_ui(parent, manager: GameInstallManager) -> None:
    """帮助 / 安装管理共用：把 Steam 普通启动修进当前游戏目录。"""
    try:
        actions = manager.apply_steam_launch_fix()
    except GameInstallError as exc:
        QMessageBox.warning(parent, t("install.steam_fail"), str(exc))
        return
    QMessageBox.information(
        parent,
        t("install.steam_ok_title"),
        "\n".join("· " + line for line in actions) + "\n\n" + t("install.steam_ok_msg"),
    )


class ModManagerDialog(QDialog):
    """配置一次游戏目录，之后自动安装运行时并管理 Mod 启停。"""

    def __init__(self, manager: GameInstallManager, parent=None):
        super().__init__(parent)
        self.manager = manager
        self._loading = False
        self.setWindowTitle(t("install.title"))
        self.resize(900, 600)

        layout = QVBoxLayout(self)
        intro = QLabel(t("install.intro"))
        intro.setWordWrap(True)
        layout.addWidget(intro)

        game_box = QGroupBox(t("install.game_box"))
        game_layout = QVBoxLayout(game_box)
        path_row = QHBoxLayout()
        self.path_edit = QLineEdit()
        self.path_edit.setReadOnly(True)
        self.path_edit.setPlaceholderText(t("install.path_ph"))
        choose_btn = QPushButton(t("install.choose"))
        detect_btn = QPushButton(t("install.detect"))
        choose_btn.clicked.connect(self._choose_game_dir)
        detect_btn.clicked.connect(self._detect_game_dir)
        path_row.addWidget(self.path_edit, 1)
        path_row.addWidget(choose_btn)
        path_row.addWidget(detect_btn)
        game_layout.addLayout(path_row)
        status_row = QHBoxLayout()
        self.status_label = QLabel(t("install.unconfigured"))
        self.status_label.setWordWrap(True)
        self.install_btn = QPushButton(t("install.reinstall_runtime"))
        self.install_btn.clicked.connect(self._install_runtime)
        self.doctor_btn = QPushButton(t("doctor.open"))
        self.doctor_btn.clicked.connect(self._show_runtime_doctor)
        self.rollback_btn = QPushButton(t("install.rollback"))
        self.rollback_btn.clicked.connect(self._restore_runtime)
        self.bepinex_btn = QPushButton(t("install.bepinex"))
        self.bepinex_btn.setToolTip(t("install.bepinex_tip"))
        self.bepinex_btn.clicked.connect(self._install_bepinex)
        self.steam_fix_btn = QPushButton(t("install.steam_fix"))
        self.steam_fix_btn.setToolTip(t("install.steam_fix_tip"))
        self.steam_fix_btn.clicked.connect(self._apply_steam_fix)
        status_row.addWidget(self.status_label, 1)
        status_row.addWidget(self.steam_fix_btn)
        status_row.addWidget(self.bepinex_btn)
        status_row.addWidget(self.install_btn)
        status_row.addWidget(self.rollback_btn)
        status_row.addWidget(self.doctor_btn)
        game_layout.addLayout(status_row)
        layout.addWidget(game_box)

        mods_box = QGroupBox(t("install.mods_box"))
        mods_layout = QVBoxLayout(mods_box)
        tip = QLabel(t("install.mods_tip"))
        tip.setWordWrap(True)
        mods_layout.addWidget(tip)
        self.table = QTableWidget(0, 5)
        self.table.setHorizontalHeaderLabels(
            [
                t("install.col.enabled"),
                t("install.col.name"),
                t("install.col.version"),
                t("install.col.author"),
                t("install.col.file"),
            ]
        )
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
        add_btn = QPushButton(t("install.add_mod"))
        refresh_btn = QPushButton(t("install.refresh"))
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
            close_btn.setText(t("install.done"))
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
                self.status_label.setText(t("install.not_connected"))
                self.bepinex_btn.setEnabled(False)
                self.bepinex_btn.setText(t("install.bepinex"))
                self.steam_fix_btn.setEnabled(False)
                self.install_btn.setEnabled(False)
                self.rollback_btn.setEnabled(False)
                self.doctor_btn.setEnabled(True)
                self.table.setRowCount(0)
                return
            self.bepinex_btn.setEnabled(True)
            root = self.manager.require_game_dir()
            has_bepinex = self.manager.bepinex_installed(root)
            self.bepinex_btn.setText(
                t("install.reinstall_bepinex") if has_bepinex else t("install.bepinex")
            )
            self.steam_fix_btn.setEnabled(has_bepinex)
            self.install_btn.setEnabled(has_bepinex)
            self.rollback_btn.setEnabled(self.manager.runtime_rollback_available())
            self.doctor_btn.setEnabled(True)
            if not has_bepinex:
                self.status_label.setText(t("install.need_bepinex"))
                self.table.setRowCount(0)
                return
            runtime_target = self.manager.plugin_dir() / "MortalModHost.dll"
            state = (
                t("install.runtime_ok")
                if runtime_target.is_file()
                else t("install.runtime_missing")
            )
            steam = (
                t("install.steam_ok")
                if self.manager.steam_launch_fix_applied(root)
                else t("install.steam_need")
            )
            self.status_label.setText(t("install.connected", state=state, steam=steam))
            records = self.manager.list_mods()
            self.table.setRowCount(len(records))
            for row, record in enumerate(records):
                enabled_item = QTableWidgetItem(
                    t("install.enabled") if record.enabled else t("install.disabled")
                )
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
                    enabled_item.setText(t("install.broken"))
                    enabled_item.setFlags(Qt.ItemFlag.ItemIsEnabled | Qt.ItemFlag.ItemIsSelectable)
                    for col in range(5):
                        self.table.item(row, col).setToolTip(record.error)
        except GameInstallError as exc:
            self.status_label.setText(t("install.read_fail", err=exc))
            self.table.setRowCount(0)
        finally:
            self._loading = False

    def _choose_game_dir(self) -> None:
        current = self.manager.load_game_dir()
        path = QFileDialog.getExistingDirectory(
            self, t("install.choose_dir"), str(current or Path.home())
        )
        if path:
            self._configure(Path(path))

    def _detect_game_dir(self) -> None:
        found = self.manager.detect_game_dir()
        if found is None:
            QMessageBox.warning(
                self,
                t("install.not_found"),
                t("install.not_found_msg"),
            )
            return
        self._configure(found)

    def _configure(self, path: Path) -> None:
        try:
            self.manager.save_game_dir(path)
        except GameInstallError as exc:
            QMessageBox.critical(self, t("install.connect_fail"), str(exc))
            return
        if self.manager.bepinex_installed(path):
            try:
                target, changed = self.manager.install_runtime()
            except GameInstallError as exc:
                QMessageBox.critical(self, t("install.runtime_fail"), str(exc))
                self.refresh()
                return
            QMessageBox.information(
                self,
                t("install.connected_title"),
                t("install.runtime_changed" if changed else "install.runtime_current", path=target),
            )
        else:
            QMessageBox.information(
                self,
                t("install.found_title"),
                t("install.found_message"),
            )
        self.refresh()

    def _install_bepinex(self) -> None:
        try:
            root = self.manager.require_game_dir()
            architecture = self.manager.game_architecture(root)
        except GameInstallError as exc:
            QMessageBox.critical(self, t("install.bepinex_fail"), str(exc))
            return
        answer = QMessageBox.question(
            self,
            t("install.bepinex_confirm_title"),
            t("install.bepinex_confirm", architecture=architecture),
        )
        if answer != QMessageBox.StandardButton.Yes:
            return

        dialog = QProgressDialog(t("install.download_prepare"), t("common.cancel"), 0, 0, self)
        dialog.setWindowTitle(t("install.bepinex_confirm_title"))
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
                raise GameInstallError(t("install.cancelled"))

        try:
            version, source = self.manager.install_bepinex(report)
            target, _changed = self.manager.install_runtime()
        except GameInstallError as exc:
            dialog.close()
            QMessageBox.critical(self, t("install.bepinex_install_fail"), str(exc))
            self.refresh()
            return
        dialog.close()
        QMessageBox.information(
            self,
            t("install.complete"),
            t("install.bepinex_done", version=version, target=target, source=source),
        )
        self.refresh()

    def _apply_steam_fix(self) -> None:
        apply_steam_launch_fix_ui(self, self.manager)
        self.refresh()

    def _install_runtime(self) -> None:
        try:
            target, changed = self.manager.install_runtime()
        except GameInstallError as exc:
            QMessageBox.critical(self, t("install.fail"), str(exc))
            return
        QMessageBox.information(
            self,
            t("install.complete"),
            t("install.updated" if changed else "install.already_current", path=target),
        )
        self.refresh()

    def _show_runtime_doctor(self) -> None:
        RuntimeDoctorDialog(self.manager, self).exec()
        self.refresh()

    def _restore_runtime(self) -> None:
        if self.manager.is_game_running():
            QMessageBox.warning(self, t("install.rollback_fail"), t("install.exit_before_rollback"))
            return
        answer = QMessageBox.question(
            self,
            t("install.rollback_title"),
            t("install.rollback_confirm"),
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
            QMessageBox.StandardButton.No,
        )
        if answer != QMessageBox.StandardButton.Yes:
            return
        try:
            restored = self.manager.restore_previous_runtime()
        except GameInstallError as exc:
            QMessageBox.critical(self, t("install.rollback_runtime_fail"), str(exc))
            self.refresh()
            return
        QMessageBox.information(
            self,
            t("install.rollback_done"),
            t("install.restored_files", files="\n".join(str(path) for path in restored)),
        )
        self.refresh()

    def _add_mod(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, t("install.choose_mod"), str(Path.home()), t("install.mod_filter")
        )
        if not path:
            return
        try:
            target = self.manager.install_mod(Path(path), enabled=True)
        except GameInstallError as exc:
            QMessageBox.critical(self, t("install.mod_fail"), str(exc))
            return
        QMessageBox.information(self, t("install.mod_done"), t("install.mod_enabled", name=target.name))
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
            QMessageBox.critical(self, t("install.mod_toggle_fail"), str(exc))
        self.refresh()
