# -*- coding: utf-8 -*-
"""Project file I/O and recent-project session management."""

from __future__ import annotations

import json
import os
from pathlib import Path

from PySide6.QtGui import QAction
from PySide6.QtWidgets import QFileDialog, QMessageBox

import models
import package_io
from i18n import t
from package_inspector import inspect_lommod
from package_inspector_dialog import PackageInspectorDialog


WORK_DIR = Path.cwd() if models.FROZEN else models.project_root()


class ProjectControllerMixin:
    """Own opening, saving, importing and recent-project preferences."""

    _RECENT_MAX = 10

    def _last_dir(self, key: str) -> str:
        remembered = self.game_manager.load_pref(key)
        if remembered and Path(remembered).is_dir():
            return remembered
        return str(WORK_DIR)

    def _remember_dir(self, key: str, path: str) -> None:
        self.game_manager.save_pref(key, str(Path(path).parent))

    def open_story(self) -> None:
        if not self._confirm_discard():
            return
        path, _ = QFileDialog.getOpenFileName(
            self, "打开", self._last_dir("last_story_dir"), "story JSON (*.json)"
        )
        if path:
            self._remember_dir("last_story_dir", path)
            self._load_story_path(Path(path))

    def _should_persist_session(self) -> bool:
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
        return [
            item for item in data
            if isinstance(item, dict)
            and item.get("kind") in ("story", "lommod")
            and item.get("path")
        ]

    def _remember_project(self, kind: str, path: Path, name: str = "") -> None:
        if not self._should_persist_session():
            return
        resolved = str(Path(path).resolve())
        self.game_manager.save_pref("last_open_kind", kind)
        self.game_manager.save_pref("last_open_path", resolved)
        if self._current_id:
            self.game_manager.save_pref("last_open_story_id", self._current_id)
        recents = [item for item in self._load_recents() if item.get("path") != resolved]
        recents.insert(0, {
            "kind": kind, "path": resolved, "name": name or Path(resolved).stem,
        })
        self.game_manager.save_pref(
            "recent_projects",
            json.dumps(recents[: self._RECENT_MAX], ensure_ascii=False),
        )
        self._rebuild_recent_menu()

    def _remember_current_chapter(self) -> None:
        if self._should_persist_session() and self._current_id:
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
            kind, path = item["kind"], item["path"]
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
                "recent_projects", json.dumps(recents, ensure_ascii=False)
            )
            self._rebuild_recent_menu()
            QMessageBox.warning(
                self, t("app.title"), f"找不到文件，已从最近列表移除：\n{path}"
            )
            return
        if not self._confirm_discard():
            return
        if kind == "lommod":
            self._import_lommod_path(target)
        else:
            self._load_story_path(target)

    def restore_last_project(self) -> bool:
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
            QMessageBox.critical(self, t("app.title"), t("error.open", error=exc))
            return
        repaired = models.normalize_character_ids([story], self.editor_data)
        self._stories = {story["id"]: story}
        self._current_id = story["id"]
        self.manifest = {}
        self.manifest_base = {}
        self._story_paths = {story["id"]: path}
        self._set_project_source("story", path)
        self._saved_snapshot = self._snapshot()
        self._undo_stack.clear()
        self._redo_stack.clear()
        self._pending_before = None
        self._commit_timer.stop()
        self._refresh_all()
        if repaired:
            self._set_dirty(True)
        self._remember_project("story", path, str(story.get("title") or path.stem))
        note = f"；已自动修复 {repaired} 个人物内部 ID" if repaired else ""
        self.statusBar().showMessage(f"已打开 {path}{note}", 5000)

    def save_story(self) -> bool:
        path = self.story_path
        return self.save_story_as() if path is None else self._write_current_story(path)

    def save_story_as(self) -> bool:
        current = str(self.story_path) if self.story_path else ""
        path, _ = QFileDialog.getSaveFileName(
            self, "另存为",
            current or str(Path(self._last_dir("last_story_dir")) / f"{self._current_id}.json"),
            "story JSON (*.json)",
        )
        if not path:
            return False
        if self._write_current_story(Path(path)):
            self._remember_dir("last_story_dir", path)
            return True
        return False

    def _write_current_story(self, path: Path) -> bool:
        try:
            models.save_story(self.story, path)
        except Exception as exc:
            QMessageBox.critical(self, t("app.title"), t("error.save", error=exc))
            return False
        self._story_paths[self._current_id] = path
        self._set_project_source("story", path)
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
        if path:
            self._remember_dir("last_mod_dir", path)
            self._import_lommod_path(Path(path))

    def inspect_lommod(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, t("inspector.choose"), self._last_dir("last_mod_dir"),
            "LoM Mod 包 (*.lommod)",
        )
        if not path:
            return
        self._remember_dir("last_mod_dir", path)
        try:
            inspection = inspect_lommod(path)
        except package_io.PackError as exc:
            QMessageBox.critical(self, t("app.title"), str(exc))
            return
        PackageInspectorDialog(inspection, self).exec()

    def _import_lommod_path(self, path: Path) -> bool:
        try:
            manifest, stories = package_io.import_lommod(path)
        except package_io.PackError as exc:
            QMessageBox.critical(self, t("app.title"), str(exc))
            return False
        self._stories = {str(st.get("id") or sid): st for sid, st in stories.items()}
        repaired = models.normalize_character_ids(self._stories, self.editor_data)
        entry = manifest.get("entry")
        self._current_id = entry if entry in self._stories else sorted(self._stories)[0]
        self.manifest = manifest
        self.manifest_base = manifest
        self._story_paths = {}
        self._set_project_source("lommod", path)
        self._saved_snapshot = self._snapshot()
        self._undo_stack.clear()
        self._redo_stack.clear()
        self._pending_before = None
        self._commit_timer.stop()
        self._refresh_all()
        if repaired:
            self._set_dirty(True)
        extra = "" if len(self._stories) == 1 else (
            f"（包内共 {len(self._stories)} 个剧情，当前打开入口 {self._current_id}）"
        )
        if manifest.get("campaign"):
            extra += "（含战役 campaign 配置）"
        if repaired:
            extra += f"（已自动修复 {repaired} 个人物内部 ID）"
        title = str(manifest.get("name") or manifest.get("id") or path.stem)
        self._remember_project("lommod", path, title)
        self.statusBar().showMessage(f"已导入 {title}{extra}", 5000)
        return True
