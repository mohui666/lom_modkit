# -*- coding: utf-8 -*-
"""Game launch and read-state operations used by the editor window.

Keeping these side-effect-heavy workflows outside ``MainWindow`` makes their
lifecycle and error handling independently testable while preserving the UI
method names used by actions and shortcuts.
"""

from __future__ import annotations

import copy
import tempfile
from pathlib import Path

from PySide6.QtWidgets import QMessageBox

import package_io
from game_install import (
    PREVIEW_PACKAGE_NAME,
    GameInstallError,
    build_story_read_keys,
    reset_story_read_state,
)
from i18n import t
from preflight_dialog import PreflightDialog
from preview import build_playtest_prelude
from schema_versions import manifest_versions


def _app_title() -> str:
    return t("app.title")


class RunControllerMixin:
    """UI adapter for preview launch and persisted read-state cleanup."""

    def play_from_current_node(self) -> bool:
        """Temporarily package, install and launch from the selected node."""
        self._flush_pending()
        node = self._current_node()
        if node is None:
            QMessageBox.warning(self, _app_title(), t("error.select_story_step"))
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
            "campaign_id": preview_id,
            "name": f"编辑器临时试玩：{title}",
            "version": "0.0.0-preview",
            "author": "lom_modkit",
            "description": f"从 {script_id}/{node_id} 开始的临时测试包",
            "entry": script_id,
            "campaign": {"new_game": True, "disable_official_events": True},
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
            QMessageBox.critical(self, _app_title(), t("error.preview_start", error=exc))
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

    def _reset_read_state(self) -> None:
        """Reset this mod's read-text records after explicit confirmation."""
        mod_id = str(self.manifest.get("id") or "").strip() or "my_mod"
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
        try:
            read_keys_by_id = {
                mid: build_story_read_keys(mid, self._stories)
                for mid in [mod_id, *extra]
            }
            results = reset_story_read_state(
                mod_id, extra_ids=extra, read_keys_by_id=read_keys_by_id
            )
        except GameInstallError as exc:
            QMessageBox.critical(self, _app_title(), t("error.reset", error=exc))
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
