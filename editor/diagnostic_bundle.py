# -*- coding: utf-8 -*-
"""Privacy-bounded diagnostic bundle export.

The archive is a fixed allowlist. It never walks project/game directories and never
copies stories, user content, mods, saves or game binaries.
"""

from __future__ import annotations

from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import tempfile
from typing import Iterable
import zipfile

from app_version import EDITOR_VERSION, RUNTIME_VERSION
from game_install import GameInstallManager, RUNTIME_DLL_NAME
import models


DIAGNOSTIC_FORMAT = 1
MAX_LOG_READ_BYTES = 1024 * 1024
MAX_LOG_OUTPUT_CHARS = 256 * 1024
MAX_RELEVANT_RUNTIME_LINES = 600
MAX_VALUE_DEPTH = 8
MAX_COLLECTION_ITEMS = 1000
MAX_STRING_CHARS = 8192
STEAM_APP_ID = "1859910"

_WINDOWS_PATH_RE = re.compile(r"(?i)(?<![A-Za-z0-9_])(?:[A-Z]:[\\/][^\r\n\"<>|]*)")
_UNC_PATH_RE = re.compile(r"\\\\[^\\/\s]+[\\/][^\r\n\"<>|]*")
_POSIX_PRIVATE_RE = re.compile(r"(?<![A-Za-z0-9_:])/(?:Users|home|tmp|var/tmp)/[^\r\n\"<>|]*")
_RUNTIME_VERSION_RE = re.compile(r"MortalModHost\s+([0-9]+(?:\.[0-9A-Za-z_-]+)+)\s+启动")
_BUILD_ID_RE = re.compile(r'"buildid"\s+"([0-9]+)"')
_RUNTIME_LOG_MARKERS = (
    "MortalModHost",
    "mod-runtime-error",
    "MortalModHost.dll",
    "玩家内容披露",
    "MOD 演出",
    ".lommod",
)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _read_tail(path: Path | None, limit: int = MAX_LOG_READ_BYTES) -> str:
    if path is None or not path.is_file():
        return ""
    try:
        with path.open("rb") as stream:
            stream.seek(0, os.SEEK_END)
            size = stream.tell()
            stream.seek(max(0, size - limit), os.SEEK_SET)
            data = stream.read(limit)
        return data.decode("utf-8", errors="replace")
    except OSError:
        return ""


def _known_private_roots(game_root: Path | None) -> list[tuple[str, str]]:
    roots: list[tuple[str, str]] = []
    candidates = (
        (Path.home(), "<user-home>"),
        (Path(os.environ.get("APPDATA", "")) if os.environ.get("APPDATA") else None, "<appdata>"),
        (Path(tempfile.gettempdir()), "<temp>"),
        (game_root, "<game-dir>"),
        (models.project_root(), "<project-dir>"),
    )
    for path, label in candidates:
        if path is None:
            continue
        variants = {str(path)}
        try:
            variants.add(str(path.resolve()))
        except OSError:
            pass
        # Windows runners may expose the same temp directory through both an
        # 8.3 alias (RUNNER~1) and its expanded spelling (runneradmin).  Keep
        # both forms so a diagnostic never falls through to the generic local
        # path redaction merely because Path.resolve() changed the spelling.
        for text in variants:
            if text:
                roots.append((text, label))
    roots.sort(key=lambda item: len(item[0]), reverse=True)
    return roots


def sanitize_text(value: object, game_root: Path | None = None) -> str:
    """Remove private directory prefixes and bound an arbitrary diagnostic string."""
    text = str(value or "")
    for root, label in _known_private_roots(game_root):
        text = re.sub(re.escape(root), label, text, flags=re.IGNORECASE)
        text = re.sub(re.escape(root.replace("\\", "/")), label, text, flags=re.IGNORECASE)
    username = os.environ.get("USERNAME") or os.environ.get("USER")
    if username:
        text = re.sub(re.escape(username), "<user>", text, flags=re.IGNORECASE)
    text = _UNC_PATH_RE.sub("<network-path>", text)
    text = _WINDOWS_PATH_RE.sub("<local-path>", text)
    text = _POSIX_PRIVATE_RE.sub("<local-path>", text)
    if len(text) > MAX_STRING_CHARS:
        text = text[:MAX_STRING_CHARS] + "…"
    return text


def _sanitize_value(value: object, game_root: Path | None, depth: int = 0) -> object:
    if depth >= MAX_VALUE_DEPTH:
        return "<depth-limit>"
    if value is None or isinstance(value, (bool, int, float)):
        return value
    if isinstance(value, str):
        return sanitize_text(value, game_root)
    if isinstance(value, dict):
        result: dict[str, object] = {}
        for index, key in enumerate(sorted(value, key=lambda item: str(item))):
            if index >= MAX_COLLECTION_ITEMS:
                result["<truncated>"] = len(value) - MAX_COLLECTION_ITEMS
                break
            result[sanitize_text(key, game_root)] = _sanitize_value(
                value[key], game_root, depth + 1
            )
        return result
    if isinstance(value, (list, tuple)):
        items = list(value[:MAX_COLLECTION_ITEMS])
        result = [_sanitize_value(item, game_root, depth + 1) for item in items]
        if len(value) > MAX_COLLECTION_ITEMS:
            result.append("<%d more items>" % (len(value) - MAX_COLLECTION_ITEMS))
        return result
    return sanitize_text(value, game_root)


def _runtime_log_path(game_root: Path | None) -> Path | None:
    return game_root / "BepInEx" / "LogOutput.log" if game_root else None


def _installed_runtime_path(game_root: Path | None) -> Path | None:
    if game_root is None:
        return None
    return game_root / "BepInEx" / "plugins" / "MortalModHost" / RUNTIME_DLL_NAME


def detect_runtime_version(
    manager: GameInstallManager, game_root: Path | None, runtime_log: str
) -> str:
    installed = _installed_runtime_path(game_root)
    bundled = manager.runtime_dll
    try:
        if installed is not None and installed.is_file() and bundled.is_file():
            if _sha256(installed) == _sha256(bundled):
                return RUNTIME_VERSION
    except OSError:
        pass
    matches = _RUNTIME_VERSION_RE.findall(runtime_log or "")
    if matches:
        return matches[-1]
    if installed is not None and installed.is_file():
        return "unknown (installed runtime differs from bundled %s)" % RUNTIME_VERSION
    return "not installed"


def detect_game_version(game_root: Path | None) -> str:
    if game_root is None:
        return "not configured"
    try:
        steamapps = game_root.parents[1]
        manifest = steamapps / ("appmanifest_%s.acf" % STEAM_APP_ID)
        text = _read_tail(manifest, 512 * 1024)
        matches = _BUILD_ID_RE.findall(text)
        if matches:
            return "Steam build " + matches[-1]
    except (IndexError, OSError):
        pass
    return "unknown"


def _validation_payload(issues: Iterable[object], game_root: Path | None) -> dict:
    rows = []
    for issue in issues or ():
        rows.append({
            "severity": sanitize_text(getattr(issue, "severity", "unknown"), game_root),
            "code": sanitize_text(getattr(issue, "code", "unknown"), game_root),
            "story": sanitize_text(getattr(issue, "story_id", ""), game_root),
            "node": sanitize_text(getattr(issue, "node_id", ""), game_root),
            "message": sanitize_text(getattr(issue, "message", ""), game_root),
            "fixable": bool(getattr(issue, "fixable", False)),
        })
    return {
        "errors": sum(row["severity"] == "error" for row in rows),
        "warnings": sum(row["severity"] == "warning" for row in rows),
        "issues": rows[:MAX_COLLECTION_ITEMS],
        "truncated": max(0, len(rows) - MAX_COLLECTION_ITEMS),
    }


def _project_metadata(stories: dict[str, dict], editor_data: dict, manifest: dict) -> dict:
    node_types: Counter[str] = Counter()
    node_count = 0
    raw_nodes = 0
    user_refs: set[str] = set()
    for story in stories.values():
        if not isinstance(story, dict):
            continue
        for node in story.get("nodes") or []:
            if not isinstance(node, dict):
                continue
            node_count += 1
            node_type = str(node.get("type") or "unknown")
            node_types[node_type] += 1
            if node_type == "raw":
                raw_nodes += 1
            for value in node.values():
                if isinstance(value, str) and value.startswith("user:"):
                    user_refs.add(value)
    campaign = manifest.get("campaign") if isinstance(manifest, dict) else None
    return {
        "story_count": len(stories),
        "story_ids": sorted(str(story_id) for story_id in stories)[:MAX_COLLECTION_ITEMS],
        "node_count": node_count,
        "node_types": dict(sorted(node_types.items())),
        "raw_node_count": raw_nodes,
        "user_content_reference_count": len(user_refs),
        "editor_data_schema": editor_data.get("schema") if isinstance(editor_data, dict) else None,
        "campaign_enabled": isinstance(campaign, dict),
    }


def _filtered_runtime_log(text: str, game_root: Path | None) -> str:
    lines: list[str] = []
    continuation = 0
    for line in (text or "").splitlines():
        marked = any(
            marker.casefold() in line.casefold() for marker in _RUNTIME_LOG_MARKERS
        )
        if marked:
            lines.append(line)
            continuation = 8
            continue
        stripped = line.lstrip()
        if continuation > 0 and (
            line.startswith((" ", "\t"))
            or stripped.startswith(("at ", "--- End of", "Caused by:"))
        ):
            lines.append(line)
            continuation -= 1
        else:
            continuation = 0
    selected = lines[-MAX_RELEVANT_RUNTIME_LINES:]
    sanitized = "\n".join(sanitize_text(line, game_root) for line in selected)
    return sanitized[-MAX_LOG_OUTPUT_CHARS:]


def _sanitized_editor_log(text: str, game_root: Path | None) -> str:
    sanitized = sanitize_text(text, game_root)
    return sanitized[-MAX_LOG_OUTPUT_CHARS:]


def export_diagnostic_bundle(
    output: Path,
    stories: dict[str, dict],
    editor_data: dict,
    manifest: dict,
    validation_issues: Iterable[object],
    *,
    game_manager: GameInstallManager | None = None,
    crash_log: Path | None = None,
    editor_version: str = EDITOR_VERSION,
) -> Path:
    """Write an atomic, fixed-allowlist diagnostic ZIP and return its final path."""
    destination = Path(output)
    if destination.suffix.lower() != ".zip":
        destination = destination.with_suffix(".zip")
    destination.parent.mkdir(parents=True, exist_ok=True)
    manager = game_manager or GameInstallManager()
    game_root = manager.load_game_dir()
    runtime_log_raw = _read_tail(_runtime_log_path(game_root))
    validation = _validation_payload(validation_issues, game_root)
    safe_manifest = _sanitize_value(manifest if isinstance(manifest, dict) else {}, game_root)
    diagnostic = {
        "diagnostic_format": DIAGNOSTIC_FORMAT,
        "created_utc": datetime.now(timezone.utc).isoformat(),
        "editor_version": sanitize_text(editor_version, game_root),
        "runtime_version": detect_runtime_version(manager, game_root, runtime_log_raw),
        "detected_game_version": detect_game_version(game_root),
        "manifest": safe_manifest,
        "validation_summary": {
            "errors": validation["errors"],
            "warnings": validation["warnings"],
            "truncated": validation["truncated"],
        },
        "project_metadata": _sanitize_value(
            _project_metadata(stories, editor_data, manifest), game_root
        ),
        "privacy": {
            "fixed_allowlist": True,
            "project_story_content_included": False,
            "user_content_included": False,
            "game_files_included": False,
            "absolute_paths_redacted": True,
        },
    }
    editor_log = _sanitized_editor_log(
        _read_tail(crash_log or models.crash_log_path()), game_root
    )
    runtime_log = _filtered_runtime_log(runtime_log_raw, game_root)
    readme = (
        "lom_modkit diagnostic bundle\n"
        "Contains only version/manifest/project counts, F6 validation, and bounded relevant logs.\n"
        "Does not contain story text, user content, saves, mods, private directories, or game files.\n"
    )

    fd, temp_name = tempfile.mkstemp(
        prefix="." + destination.name + ".", suffix=".tmp", dir=str(destination.parent)
    )
    os.close(fd)
    temp_path = Path(temp_name)
    try:
        with zipfile.ZipFile(temp_path, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr(
                "diagnostic.json",
                json.dumps(diagnostic, ensure_ascii=False, indent=2) + "\n",
            )
            archive.writestr(
                "validation.json",
                json.dumps(validation, ensure_ascii=False, indent=2) + "\n",
            )
            archive.writestr("logs/editor-crash.log", editor_log)
            archive.writestr("logs/runtime.log", runtime_log)
            archive.writestr("README.txt", readme)
        temp_path.replace(destination)
    finally:
        try:
            if temp_path.exists():
                temp_path.unlink()
        except OSError:
            pass
    return destination
