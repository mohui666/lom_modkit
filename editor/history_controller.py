# -*- coding: utf-8 -*-
"""Undo/redo, dirty state, autosave and window-close lifecycle."""

from __future__ import annotations

import copy
import traceback
from pathlib import Path

from PySide6.QtWidgets import QMessageBox

from i18n import t
from preview import log_crash
from recovery_store import RecoveryError


UNDO_LIMIT = 100


class HistoryControllerMixin:
    """State-history controller mixed into the editor window."""

    def _snapshot(self) -> dict:
        return copy.deepcopy(self._stories)

    def _trim_undo(self) -> None:
        while len(self._undo_stack) > UNDO_LIMIT:
            self._undo_stack.pop(0)

    def _flush_pending(self) -> None:
        if self._pending_before is not None:
            self._undo_stack.append((self._pending_before, self._current_id))
            self._trim_undo()
            self._pending_before = None
            self._prev_snapshot = self._snapshot()
        self._commit_timer.stop()

    def _record_continuous(self) -> None:
        if self._loading:
            return
        if self._pending_before is None:
            self._pending_before = self._prev_snapshot
            self._redo_stack.clear()
        self._commit_timer.start()
        self._set_dirty(True)

    def _record_discrete(self) -> None:
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
        if not self._dirty:
            self._clear_recovery_snapshot()

    def _autosave_recovery(self) -> None:
        session = self._recovery_session
        if session is None or not self._dirty:
            return
        try:
            session.write_snapshot(
                stories=self._snapshot(),
                current_story_id=self._current_id,
                manifest=copy.deepcopy(self.manifest),
                story_paths=dict(self._story_paths),
                source_kind=self._source_kind,
                source_path=str(self._source_path) if self._source_path else None,
            )
            self._recovery_error_logged = False
        except Exception:
            if not self._recovery_error_logged:
                log_crash("写入自动恢复副本失败：\n" + traceback.format_exc())
                self._recovery_error_logged = True

    def _clear_recovery_snapshot(self) -> None:
        session = getattr(self, "_recovery_session", None)
        if session is None:
            return
        try:
            session.clear_snapshot()
        except RecoveryError:
            if not self._recovery_error_logged:
                log_crash("清除自动恢复副本失败：\n" + traceback.format_exc())
                self._recovery_error_logged = True

    def _set_project_source(self, kind: str, path: Path | None) -> None:
        self._source_kind = kind
        self._source_path = Path(path).resolve() if path is not None else None

    def _update_title(self) -> None:
        name = (
            self.manifest.get("name")
            or self.manifest.get("id")
            or self._current_id
            or "未命名"
        )
        star = " *" if self._dirty else ""
        self.setWindowTitle(f"{t('app.title')} — {name}{star}")

    def _mark_saved(self) -> None:
        self._saved_snapshot[self._current_id] = copy.deepcopy(self.story)
        for key in list(self._saved_snapshot):
            if key not in self._stories:
                del self._saved_snapshot[key]
        self._set_dirty(self._stories != self._saved_snapshot)

    def _confirm_discard(self) -> bool:
        if not self._dirty or not self._prompt_on_discard:
            return True
        box = QMessageBox(self)
        box.setWindowTitle(t("app.title"))
        box.setText(t("discard.title"))
        box.setInformativeText("导出 Mod 会保存全部章节；也可以放弃这次修改。")
        save_btn = box.addButton(t("discard.save"), QMessageBox.ButtonRole.AcceptRole)
        discard_btn = box.addButton(
            t("discard.discard"), QMessageBox.ButtonRole.DestructiveRole
        )
        box.addButton(t("discard.cancel"), QMessageBox.ButtonRole.RejectRole)
        box.setDefaultButton(save_btn)
        box.exec()
        clicked = box.clickedButton()
        box.deleteLater()
        if clicked is save_btn:
            return self.export_lommod()
        return clicked is discard_btn

    def closeEvent(self, event) -> None:
        if self._confirm_discard():
            self._commit_timer.stop()
            self._recovery_timer.stop()
            if self._recovery_session is not None:
                try:
                    self._recovery_session.mark_closed()
                except RecoveryError:
                    log_crash("关闭自动恢复会话失败：\n" + traceback.format_exc())
            event.accept()
        else:
            event.ignore()
