# -*- coding: utf-8 -*-
"""Project file I/O and recent-project session management."""

from __future__ import annotations

import copy
import json
import os
from pathlib import Path
import tempfile

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

    def open_story_folder(self) -> None:
        folder = QFileDialog.getExistingDirectory(
            self, t("menu.open_folder"), self._last_dir("last_story_dir")
        )
        if folder and self._confirm_discard():
            if self._load_story_folder(Path(folder)):
                self.game_manager.save_pref("last_story_dir", folder)

    def _load_story_folder(self, folder: Path) -> bool:
        """Load all chapters before replacing the currently edited project."""
        folder = Path(folder)
        try:
            if not folder.is_dir():
                raise ValueError(f"找不到剧情文件夹：{folder}")
            manifest_path = folder / "manifest.json"
            manifest = {}
            if manifest_path.is_file():
                manifest = json.loads(manifest_path.read_text(encoding="utf-8-sig"))
                if not isinstance(manifest, dict):
                    raise ValueError("manifest.json 顶层必须是 JSON 对象")
            story_dir = folder / "story" if (folder / "story").is_dir() else folder
            stories, paths = {}, {}
            for path in sorted(story_dir.iterdir()):
                if (
                    not path.is_file()
                    or path.suffix.lower() != ".json"
                    or path.name == "manifest.json"
                ):
                    continue
                try:
                    document = json.loads(path.read_text(encoding="utf-8-sig"))
                    # Settings/content JSON beside stories is not a chapter.
                    if not isinstance(document, dict) or "nodes" not in document:
                        continue
                    story = models.load_story(path)
                    sid = story["id"]
                    if not isinstance(sid, str) or not models.ID_PATTERN.fullmatch(sid):
                        raise ValueError("章节 ID 必须为 1～64 个字母、数字、下划线或连字符")
                    if sid in stories:
                        raise ValueError(f"章节 ID {sid!r} 重复：{paths[sid].name} 与 {path.name}")
                    stories[sid], paths[sid] = story, path
                except Exception as exc:
                    raise ValueError(f"{path.name}：{exc}") from exc
            if not stories:
                raise ValueError("文件夹中没有可打开的剧情 JSON（应含 nodes 数组）；也可选择含 story/ 的项目根目录")
            entry = manifest.get("entry")
            if entry is not None and not isinstance(entry, str):
                raise ValueError("manifest.json 的 entry 必须是章节 ID 字符串")
        except Exception as exc:
            QMessageBox.critical(self, t("app.title"), t("error.open", error=exc))
            return False

        saved = copy.deepcopy(stories)
        repaired = models.normalize_character_ids(stories, self.editor_data)
        self._stories = stories
        self._current_id = (
            entry if entry in stories else ("main" if "main" in stories else sorted(stories)[0])
        )
        self.manifest = manifest
        self.manifest_base = copy.deepcopy(manifest)
        self._story_paths = paths
        self._set_project_source("folder", folder)
        self._saved_snapshot = saved
        self._undo_stack.clear()
        self._redo_stack.clear()
        self._pending_before = None
        self._commit_timer.stop()
        self._refresh_all()
        self._remember_project("folder", folder, str(manifest.get("name") or folder.name))
        note = f"；已自动修复 {repaired} 个人物内部 ID" if repaired else ""
        self.statusBar().showMessage(f"已打开 {len(stories)} 个剧情章节：{folder}{note}", 6000)
        return True

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
            and item.get("kind") in ("story", "lommod", "folder")
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
            tag = "Mod" if kind == "lommod" else ("剧情文件夹" if kind == "folder" else "剧本")
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
        if not (target.is_dir() if kind == "folder" else target.is_file()):
            recents = [item for item in self._load_recents() if item.get("path") != path]
            self.game_manager.save_pref(
                "recent_projects", json.dumps(recents, ensure_ascii=False)
            )
            self._rebuild_recent_menu()
            QMessageBox.warning(
                self, t("app.title"), f"找不到项目，已从最近列表移除：\n{path}"
            )
            return
        if not self._confirm_discard():
            return
        if kind == "lommod":
            self._import_lommod_path(target)
        elif kind == "folder":
            self._load_story_folder(target)
        else:
            self._load_story_path(target)

    def restore_last_project(self) -> bool:
        kind = self.game_manager.load_pref("last_open_kind")
        path = self.game_manager.load_pref("last_open_path")
        story_id = self.game_manager.load_pref("last_open_story_id")
        if kind not in ("story", "lommod", "folder") or not path:
            return False
        target = Path(path)
        if not (target.is_dir() if kind == "folder" else target.is_file()):
            return False
        if kind == "lommod":
            ok = self._import_lommod_path(target)
        elif kind == "folder":
            ok = self._load_story_folder(target)
        else:
            ok = self._load_story_path(target)
        if not ok:
            return False
        if story_id and story_id in self._stories and story_id != self._current_id:
            self._current_id = story_id
            self._refresh_all()
            self._remember_current_chapter()
        return True

    def _load_story_path(self, path: Path) -> bool:
        try:
            story = models.load_story(path)
        except Exception as exc:
            QMessageBox.critical(self, t("app.title"), t("error.open", error=exc))
            return False
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
        return True

    def save_story(self) -> bool:
        self._flush_pending()
        if self._source_kind == "folder" and self._source_path is not None:
            return self._write_story_folder(self._source_path)
        if len(self._stories) > 1:
            return self.save_story_folder()
        path = self.story_path
        return self.save_story_as() if path is None else self._write_current_story(path)

    def save_story_as(self) -> bool:
        if len(self._stories) > 1 or self._source_kind == "folder":
            return self.save_story_folder()
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

    def save_story_folder(self) -> bool:
        initial = (
            str(self._source_path)
            if self._source_kind == "folder" and self._source_path
            else self._last_dir("last_story_dir")
        )
        folder = QFileDialog.getExistingDirectory(
            self, t("menu.save_folder"), initial,
        )
        if not folder:
            return False
        if self._write_story_folder(Path(folder)):
            self.game_manager.save_pref("last_story_dir", folder)
            return True
        return False

    def _write_story_folder(self, folder: Path) -> bool:
        """Save every chapter atomically per file; retain dirty state on failure."""
        self._flush_pending()
        folder = Path(folder)
        story_dir = folder / "story" if (folder / "story").is_dir() else folder
        paths = {}
        target = folder
        try:
            owned = {path.resolve() for path in self._story_paths.values() if path is not None}
            seen = set()
            for sid in self._stories:
                if not models.ID_PATTERN.fullmatch(sid):
                    raise ValueError(f"章节 ID {sid!r} 不能用作文件名")
                original = self._story_paths.get(sid)
                target = (
                    original
                    if original and original.parent.resolve() == story_dir.resolve()
                    else story_dir / f"{sid}.json"
                )
                key = str(target.resolve()).casefold()
                if key in seen:
                    raise ValueError(f"多个章节指向同一文件：{target.name}，请另存到空文件夹")
                seen.add(key)
                if target.exists() and target.resolve() not in owned:
                    raise ValueError(f"目标已存在不属于当前项目的文件：{target.name}，请选择空文件夹")
                paths[sid] = target
            target = folder / "manifest.json"
            same_source = self._source_kind == "folder" and self._source_path == folder.resolve()
            if target.exists() and not same_source:
                raise ValueError("目标已存在其他项目的 manifest.json，请选择空文件夹")
            manifest = copy.deepcopy(self.manifest_base or self.manifest or {})
            manifest.setdefault("entry", "main" if "main" in self._stories else self._current_id)
            # Prepare metadata before touching any files.
            payload = json.dumps(manifest, ensure_ascii=False, indent=2) + "\n"
            for sid, target in paths.items():
                models.save_story(self._stories[sid], target)
            target = folder / "manifest.json"
            temp_path = None
            try:
                fd, name = tempfile.mkstemp(prefix="manifest.", suffix=".tmp", dir=folder)
                temp_path = Path(name)
                with os.fdopen(fd, "w", encoding="utf-8", newline="") as stream:
                    stream.write(payload)
                    stream.flush()
                    os.fsync(stream.fileno())
                os.replace(temp_path, target)
                temp_path = None
            finally:
                if temp_path is not None:
                    temp_path.unlink(missing_ok=True)
        except Exception as exc:
            QMessageBox.critical(self, t("app.title"), t("error.save", error=f"{target}：{exc}"))
            return False
        self._story_paths.update(paths)
        self.manifest = manifest
        self.manifest_base = copy.deepcopy(manifest)
        self._set_project_source("folder", folder)
        self._saved_snapshot = self._snapshot()
        self._set_dirty(False)
        self._remember_project("folder", folder, str(manifest.get("name") or folder.name))
        self.statusBar().showMessage(f"已保存全部 {len(paths)} 个剧情章节：{folder}", 5000)
        return True

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
