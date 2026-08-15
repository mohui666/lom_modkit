# -*- coding: utf-8 -*-
"""Atomic, out-of-project recovery snapshots for the desktop editor."""

from __future__ import annotations

from datetime import datetime, timezone
import json
import os
from pathlib import Path
import tempfile
import uuid


RECOVERY_SCHEMA = 1
MAX_SNAPSHOT_BYTES = 64 * 1024 * 1024


class RecoveryError(RuntimeError):
    pass


def recovery_root() -> Path:
    override = os.environ.get("LOM_MODKIT_RECOVERY_DIR")
    if override:
        return Path(override)
    appdata = os.environ.get("APPDATA")
    base = Path(appdata) if appdata else Path.home() / "AppData" / "Roaming"
    return base / "lom_modkit" / "recovery"


def _now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def _atomic_json(path: Path, value: dict, max_bytes: int) -> None:
    try:
        payload = (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    except (TypeError, ValueError) as exc:
        raise RecoveryError(f"恢复副本不是有效 JSON：{exc}") from exc
    if len(payload) > max_bytes:
        raise RecoveryError(
            "恢复副本超过 %d MiB 上限" % (max_bytes // (1024 * 1024))
        )
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        fd, raw = tempfile.mkstemp(prefix=path.name + ".", suffix=".tmp", dir=path.parent)
        temporary = Path(raw)
        with os.fdopen(fd, "wb") as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        temporary = None
    except OSError as exc:
        raise RecoveryError(f"无法写入恢复副本：{exc}") from exc
    finally:
        if temporary is not None:
            try:
                temporary.unlink()
            except OSError:
                pass


class RecoverySession:
    """One editor-process recovery slot, intentionally separate from project files."""

    def __init__(self, root: Path | None = None, editor_version: str = ""):
        self.root = Path(root) if root is not None else recovery_root()
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        self.session_id = "%s-%d-%s" % (stamp, os.getpid(), uuid.uuid4().hex[:12])
        self.directory = self.root / self.session_id
        self.marker_path = self.directory / "session.json"
        self.snapshot_path = self.directory / "snapshot.json"
        self._marker = {
            "recovery_schema": RECOVERY_SCHEMA,
            "session_id": self.session_id,
            "editor_version": str(editor_version or ""),
            "pid": os.getpid(),
            "status": "active",
            "started_at": _now(),
            "updated_at": _now(),
        }
        _atomic_json(self.marker_path, self._marker, 64 * 1024)

    def write_snapshot(
        self,
        *,
        stories: dict,
        current_story_id: str,
        manifest: dict,
        story_paths: dict,
        source_kind: str,
        source_path: str | None,
    ) -> Path:
        if not isinstance(stories, dict) or not stories:
            raise RecoveryError("恢复副本至少需要一个剧情章节")
        if current_story_id not in stories:
            raise RecoveryError("恢复副本的当前章节不存在")
        if source_kind not in ("untitled", "story", "lommod"):
            raise RecoveryError("恢复副本来源类型无效")
        document = {
            "recovery_schema": RECOVERY_SCHEMA,
            "session_id": self.session_id,
            "saved_at": _now(),
            "source": {
                "kind": source_kind,
                "path": str(source_path) if source_path else None,
            },
            "current_story_id": current_story_id,
            "manifest": manifest if isinstance(manifest, dict) else {},
            "story_paths": {
                str(key): (str(value) if value else None)
                for key, value in story_paths.items()
            },
            "stories": stories,
        }
        _atomic_json(self.snapshot_path, document, MAX_SNAPSHOT_BYTES)
        self._marker["updated_at"] = document["saved_at"]
        self._marker["has_snapshot"] = True
        _atomic_json(self.marker_path, self._marker, 64 * 1024)
        return self.snapshot_path

    def clear_snapshot(self) -> None:
        try:
            self.snapshot_path.unlink(missing_ok=True)
        except OSError as exc:
            raise RecoveryError(f"无法清除恢复副本：{exc}") from exc
        self._marker["updated_at"] = _now()
        self._marker["has_snapshot"] = False
        _atomic_json(self.marker_path, self._marker, 64 * 1024)

    def mark_closed(self) -> None:
        self.clear_snapshot()
        self._marker["status"] = "closed"
        self._marker["closed_at"] = _now()
        self._marker["updated_at"] = self._marker["closed_at"]
        _atomic_json(self.marker_path, self._marker, 64 * 1024)
