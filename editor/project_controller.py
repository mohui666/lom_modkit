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

# 单个 story JSON 的读取上限：防止误把巨大的资源 JSON 当剧情读进内存
MAX_STORY_BYTES = 64 * 1024 * 1024


def story_json_candidates(root: Path) -> list[Path]:
    """扫描文件夹（含子目录）里真正是剧情的 JSON，跳过 manifest 之类的同级文件。

    判定标准很保守：顶层是对象且 ``nodes`` 是数组。story JSON 一定有 ``nodes``，
    而 manifest / 内容库 / 配置类 JSON 都没有，因此不会误读。
    """
    out: list[Path] = []
    seen: set[Path] = set()
    try:
        entries = sorted(Path(root).rglob("*.json"))
    except OSError:
        return out
    for path in entries:
        try:
            resolved = path.resolve()
        except OSError:
            continue
        if resolved in seen:
            continue
        seen.add(resolved)
        try:
            if path.stat().st_size > MAX_STORY_BYTES:
                continue
            raw = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError, UnicodeDecodeError):
            continue
        if isinstance(raw, dict) and isinstance(raw.get("nodes"), list):
            out.append(path)
    return out


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
        """打开…：默认打开一个「工作文件夹」（整个剧情目录），而非单个文件。

        用户的工作流是「一个文件夹 = 一个项目（内含多个章节剧情 JSON）」，
        所以默认打开动作直接选文件夹；单文件/多文件仍可从「打开多个文件」进入。
        """
        self.open_story_folder()

    def open_story_folder(self) -> None:
        """打开文件夹：把里面的 story JSON 一次性载入为同一项目的多个章节。

        原版只能单选一个文件，多个剧情文件要么逐个开、要么合成一个包；
        这里直接扫整个文件夹（含子目录），按文件名排序后一起载入，
        章节之间可以互相跳转、也能一次性导出。
        """
        if not self._confirm_discard():
            return
        directory = QFileDialog.getExistingDirectory(
            self, t("folder.title"), self._last_dir("last_story_dir")
        )
        if not directory:
            return
        root = Path(directory)
        paths = story_json_candidates(root)
        if not paths:
            QMessageBox.warning(self, t("app.title"), t("folder.empty"))
            return
        self.game_manager.save_pref("last_story_dir", str(root))
        self._load_story_paths(paths, folder=root)

    def open_story_files(self) -> None:
        """打开多个文件：一次选中多个 story JSON，载入为多个章节。"""
        if not self._confirm_discard():
            return
        paths, _ = QFileDialog.getOpenFileNames(
            self,
            t("menu.open_files"),
            self._last_dir("last_story_dir"),
            "story JSON (*.json)",
        )
        if not paths:
            return
        self._remember_dir("last_story_dir", paths[0])
        self._load_story_paths([Path(item) for item in paths], folder=None)

    def _load_story_paths(
        self, paths: list[Path], folder: Path | None, quiet: bool = False
    ) -> None:
        """把多个 story JSON 载入为同一项目。

        章节 id 必须唯一；不同文件撞 id 时按 ``base_2``、``base_3`` 递增改名，
        并在状态栏说明改了几个——不静默覆盖任何一边。``quiet=True`` 时（启动
        时自动恢复上个项目）不弹任何对话框。
        """
        stories: dict[str, dict] = {}
        story_paths: dict[str, Path] = {}
        failed: list[str] = []
        renamed = 0
        repaired = 0
        first_id = ""
        for path in paths:
            try:
                story = models.load_story(path)
            except Exception as exc:  # noqa: BLE001 - 单个坏文件不该拖垮整批
                failed.append(t("folder.load_fail", file=path.name, error=exc))
                continue
            repaired += models.normalize_character_ids([story], self.editor_data)
            base = str(story.get("id") or path.stem)
            sid, suffix = base, 2
            while sid in stories:
                sid = f"{base}_{suffix}"
                suffix += 1
            if sid != base:
                renamed += 1
            story["id"] = sid
            stories[sid] = story
            story_paths[sid] = path
            if not first_id:
                first_id = sid

        if not stories:
            if not quiet:
                QMessageBox.critical(
                    self, t("app.title"), t("folder.none_loaded", errors="\n".join(failed[:10]))
                )
            return

        self._install_project(
            stories,
            story_paths,
            first_id,
            "folder" if folder else "files",
            folder if folder is not None else (paths[0] if paths else None),
        )
        if repaired or renamed:
            self._set_dirty(True)

        self._remember_multi_project(folder, paths)
        names = "、".join(
            str(story.get("title") or sid) for sid, story in list(stories.items())[:5]
        )
        if len(stories) > 5:
            names += t("folder.more", rest=len(stories) - 5)
        source = str(folder) if folder else t("folder.from_files")
        note = f"（{renamed} 个同名章节已自动改名）" if renamed else ""
        if repaired:
            note += f"（已自动修复 {repaired} 个人物内部 ID）"
        kind_label = t("folder.opened") if folder else t("folder.opened_files")
        self.statusBar().showMessage(
            kind_label.format(count=len(stories), source=source, names=names) + note, 8000
        )
        if failed and not quiet:
            QMessageBox.warning(
                self,
                t("app.title"),
                t("folder.partial", failed=len(failed), errors="\n".join(failed[:10])),
            )

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
            and item.get("kind") in ("story", "lommod", "folder", "files")
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

    def _remember_multi_project(self, folder: Path | None, paths: list[Path]) -> None:
        """记录「多文件 / 文件夹」项目，供下次启动自动恢复和「最近打开」使用。"""
        if not self._should_persist_session():
            return
        kind = "folder" if folder is not None else "files"
        anchor = folder if folder is not None else (paths[0] if paths else None)
        if anchor is None:
            return
        resolved = str(Path(anchor).resolve())
        self.game_manager.save_pref("last_open_kind", kind)
        self.game_manager.save_pref("last_open_path", resolved)
        self.game_manager.save_pref(
            "last_open_files", json.dumps([str(p) for p in paths], ensure_ascii=False)
        )
        if self._current_id:
            self.game_manager.save_pref("last_open_story_id", self._current_id)
        name = Path(resolved).name or resolved
        recents = [item for item in self._load_recents() if item.get("path") != resolved]
        recents.insert(0, {"kind": kind, "path": resolved, "name": name})
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
            tag = {
                "lommod": "Mod",
                "folder": t("recent.tag_folder"),
                "files": t("recent.tag_files"),
            }.get(kind, "剧本")
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
        if kind in ("folder", "files"):
            # 文件夹按当前内容重新扫描；多文件按记录的文件列表逐个校验
            if kind == "folder":
                if not target.is_dir():
                    self._forget_recent(path)
                    return
                candidates = story_json_candidates(target)
            else:
                try:
                    items = json.loads(self.game_manager.load_pref("last_open_files") or "[]")
                except (ValueError, TypeError):
                    items = []
                candidates = [
                    Path(item)
                    for item in items
                    if isinstance(item, str) and Path(item).is_file()
                ]
            if not candidates:
                self._forget_recent(path)
                return
            if not self._confirm_discard():
                return
            self._load_story_paths(
                candidates, folder=target if kind == "folder" else None
            )
            return
        if not target.is_file():
            self._forget_recent(path)
            return
        if not self._confirm_discard():
            return
        if kind == "lommod":
            self._import_lommod_path(target)
        else:
            self._load_story_path(target)

    def _forget_recent(self, path: str) -> None:
        """把失效项从最近列表移除并提示，避免下次又点到同一个坏路径。"""
        recents = [item for item in self._load_recents() if item.get("path") != path]
        self.game_manager.save_pref(
            "recent_projects", json.dumps(recents, ensure_ascii=False)
        )
        self._rebuild_recent_menu()
        QMessageBox.warning(
            self, t("app.title"), t("recent.missing", path=path)
        )

    def restore_last_project(self) -> bool:
        kind = self.game_manager.load_pref("last_open_kind")
        path = self.game_manager.load_pref("last_open_path")
        if not path:
            return False
        target = Path(path)
        if kind == "folder":
            if not target.is_dir():
                return False
            paths = story_json_candidates(target)
            if not paths:
                return False
            self._load_story_paths(paths, folder=target, quiet=True)
        elif kind == "files":
            try:
                items = json.loads(self.game_manager.load_pref("last_open_files") or "[]")
            except (ValueError, TypeError):
                items = []
            paths = [
                Path(item)
                for item in items
                if isinstance(item, str) and Path(item).is_file()
            ]
            if not paths:
                return False
            self._load_story_paths(paths, folder=None, quiet=True)
        elif kind == "lommod":
            if not target.is_file():
                return False
            if not self._import_lommod_path(target):
                return False
        elif kind == "story":
            if not target.is_file():
                return False
            self._load_story_path(target)
            if not self._story_paths:
                return False
        else:
            return False
        if not self._stories:
            return False
        story_id = self.game_manager.load_pref("last_open_story_id")
        if story_id and story_id in self._stories and story_id != self._current_id:
            self._current_id = story_id
            self._refresh_all()
        return True

    def _install_project(
        self,
        stories: dict[str, dict],
        story_paths: dict[str, Path],
        current_id: str,
        source_kind: str,
        source_path: Path | None,
    ) -> None:
        """把一批已读好的 story 装进编辑器状态（单文件 / 多文件 / 文件夹共用）。"""
        self._stories = stories
        self._current_id = current_id
        self.manifest = {}
        self.manifest_base = {}
        self._story_paths = story_paths
        self._set_project_source(source_kind, source_path)
        self._saved_snapshot = self._snapshot()
        self._undo_stack.clear()
        self._redo_stack.clear()
        self._pending_before = None
        self._commit_timer.stop()
        self._refresh_all()

    def _load_story_path(self, path: Path) -> None:
        try:
            story = models.load_story(path)
        except Exception as exc:
            QMessageBox.critical(self, t("app.title"), t("error.open", error=exc))
            return
        repaired = models.normalize_character_ids([story], self.editor_data)
        self._install_project(
            {story["id"]: story}, {story["id"]: path}, story["id"], "story", path
        )
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
