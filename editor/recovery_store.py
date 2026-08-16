# -*- coding: utf-8 -*-
"""Atomic, out-of-project recovery snapshots for the desktop editor."""

from __future__ import annotations

from datetime import datetime, timezone
from dataclasses import dataclass
import ctypes
import json
import os
from pathlib import Path
import tempfile
import uuid


RECOVERY_SCHEMA = 1
MAX_SNAPSHOT_BYTES = 64 * 1024 * 1024


class RecoveryError(RuntimeError):
    pass


@dataclass(frozen=True)
class RecoveryCandidate:
    session_id: str
    snapshot_path: Path
    marker_path: Path
    saved_at: str
    source_kind: str
    source_path: str | None
    current_story_id: str
    story_ids: tuple[str, ...]
    project_name: str
    document: dict


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


def _read_json(path: Path, max_bytes: int) -> dict:
    try:
        size = path.stat().st_size
        if size <= 0 or size > max_bytes:
            raise RecoveryError("恢复文件大小无效")
        value = json.loads(path.read_text(encoding="utf-8"))
    except RecoveryError:
        raise
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise RecoveryError(f"无法读取恢复文件 {path.name}：{exc}") from exc
    if not isinstance(value, dict):
        raise RecoveryError("恢复文件顶层必须是对象")
    return value


def _process_is_running(pid: int) -> bool:
    if type(pid) is not int or pid <= 0:
        return False
    if pid == os.getpid():
        return True
    if os.name == "nt":
        process_query = 0x1000
        still_active = 259
        handle = ctypes.windll.kernel32.OpenProcess(process_query, False, pid)
        if not handle:
            return False
        try:
            code = ctypes.c_ulong()
            if not ctypes.windll.kernel32.GetExitCodeProcess(handle, ctypes.byref(code)):
                return False
            return code.value == still_active
        finally:
            ctypes.windll.kernel32.CloseHandle(handle)
    try:
        os.kill(pid, 0)
        return True
    except (OSError, ValueError):
        return False


def _candidate_from(directory: Path, include_live: bool) -> RecoveryCandidate | None:
    marker_path = directory / "session.json"
    snapshot_path = directory / "snapshot.json"
    if not marker_path.is_file() or not snapshot_path.is_file():
        return None
    marker = _read_json(marker_path, 64 * 1024)
    if marker.get("recovery_schema") != RECOVERY_SCHEMA:
        return None
    if marker.get("status") != "active" or not marker.get("has_snapshot"):
        return None
    if not include_live and _process_is_running(marker.get("pid")):
        return None
    document = _read_json(snapshot_path, MAX_SNAPSHOT_BYTES)
    if document.get("recovery_schema") != RECOVERY_SCHEMA:
        raise RecoveryError("不支持的恢复快照版本")
    session_id = str(marker.get("session_id") or "")
    if not session_id or document.get("session_id") != session_id:
        raise RecoveryError("恢复会话 ID 不一致")
    stories = document.get("stories")
    if not isinstance(stories, dict) or not stories or len(stories) > 256:
        raise RecoveryError("恢复快照的剧情章节集合无效")
    story_ids = []
    for key, story in stories.items():
        if not isinstance(key, str) or not key or not isinstance(story, dict):
            raise RecoveryError("恢复快照含无效剧情章节")
        if not isinstance(story.get("nodes"), list):
            raise RecoveryError("恢复剧情缺少 nodes 数组")
        story_ids.append(key)
    current = document.get("current_story_id")
    if current not in stories:
        raise RecoveryError("恢复快照当前章节不存在")
    source = document.get("source")
    if not isinstance(source, dict):
        source = {}
    kind = source.get("kind")
    if kind not in ("untitled", "story", "lommod"):
        kind = "untitled"
    raw_path = source.get("path")
    source_path = raw_path if isinstance(raw_path, str) and len(raw_path) <= 4096 else None
    manifest = document.get("manifest")
    if not isinstance(manifest, dict):
        manifest = {}
    project_name = str(manifest.get("name") or manifest.get("id") or current)[:160]
    return RecoveryCandidate(
        session_id=session_id,
        snapshot_path=snapshot_path,
        marker_path=marker_path,
        saved_at=str(document.get("saved_at") or marker.get("updated_at") or "")[:64],
        source_kind=kind,
        source_path=source_path,
        current_story_id=str(current),
        story_ids=tuple(sorted(story_ids)),
        project_name=project_name,
        document=document,
    )


def list_recovery_candidates(root: Path | None = None, *,
                             exclude_session_id: str | None = None,
                             include_live: bool = False) -> list[RecoveryCandidate]:
    base = Path(root) if root is not None else recovery_root()
    if not base.is_dir():
        return []
    candidates = []
    for directory in sorted(base.iterdir(), reverse=True):
        if not directory.is_dir() or directory.name == exclude_session_id:
            continue
        try:
            candidate = _candidate_from(directory, include_live)
        except RecoveryError:
            continue
        if candidate is not None:
            candidates.append(candidate)
        if len(candidates) >= 50:
            break
    candidates.sort(key=lambda item: item.saved_at, reverse=True)
    return candidates


def finish_candidate(candidate: RecoveryCandidate, status: str) -> None:
    if status not in ("recovered", "discarded"):
        raise RecoveryError("恢复候选结束状态无效")
    marker = _read_json(candidate.marker_path, 64 * 1024)
    if marker.get("session_id") != candidate.session_id:
        raise RecoveryError("恢复候选会话已发生变化")
    try:
        candidate.snapshot_path.unlink(missing_ok=True)
    except OSError as exc:
        raise RecoveryError(f"无法清除已处理的恢复快照：{exc}") from exc
    marker["status"] = status
    marker["has_snapshot"] = False
    marker["updated_at"] = _now()
    marker[status + "_at"] = marker["updated_at"]
    _atomic_json(candidate.marker_path, marker, 64 * 1024)


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
