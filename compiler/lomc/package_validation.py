# -*- coding: utf-8 -*-
"""Shared ``.lommod`` ZIP validation rules.

This module deliberately depends only on the standard library so the Editor,
Compiler and their tests use the same path and size semantics.  The C# Host
mirrors these constants and rules in ``ModLoader.ValidateArchiveLimits``.
"""

from __future__ import annotations

import os
from pathlib import Path, PurePosixPath, PureWindowsPath


MAX_PACKAGE_FILE_BYTES = 160 * 1024 * 1024
MAX_ARCHIVE_ENTRIES = 2048
MAX_ENTRY_BYTES = 32 * 1024 * 1024
MAX_ARCHIVE_UNCOMPRESSED = 128 * 1024 * 1024
MAX_TEXT_BYTES = 4 * 1024 * 1024


class ArchiveValidationError(ValueError):
    """An archive entry violates the cross-runtime package contract."""


def resolve_confined_file(root, candidate) -> Path:
    """Resolve a source file and reject symlink/junction escapes from ``root``."""
    try:
        real_root = Path(root).resolve(strict=True)
        real_file = Path(candidate).resolve(strict=True)
    except OSError as exc:
        raise ArchiveValidationError("打包文件路径无法解析：%s" % exc) from exc
    try:
        confined = os.path.commonpath((str(real_root), str(real_file))) == str(real_root)
    except ValueError:
        confined = False
    if not confined:
        raise ArchiveValidationError(
            "打包文件通过 symlink/junction 指向项目目录外：%s" % candidate
        )
    if not real_file.is_file():
        raise ArchiveValidationError("打包文件不存在或不是普通文件：%s" % candidate)
    return real_file


def canonical_archive_name(name: str) -> str:
    """Return a canonical POSIX ZIP name or reject an unsafe/ambiguous path."""
    if not isinstance(name, str) or not name or "\x00" in name:
        raise ArchiveValidationError("包内包含空路径或 NUL 字符")
    if "\\" in name:
        raise ArchiveValidationError("包内路径必须使用正斜杠：%r" % name)
    normalized = name
    is_directory = normalized.endswith("/")
    body = normalized[:-1] if is_directory else normalized
    windows = PureWindowsPath(name)
    if (
        not body
        or normalized.startswith("/")
        or PurePosixPath(normalized).is_absolute()
        or windows.is_absolute()
        or bool(windows.drive)
    ):
        raise ArchiveValidationError("包内包含不安全路径：%r" % name)
    parts = body.split("/")
    if any(part in ("", ".", "..") for part in parts):
        raise ArchiveValidationError("包内包含不安全或非规范路径：%r" % name)
    canonical = "/".join(parts)
    return canonical + "/" if is_directory else canonical


def validate_archive_entries(infos):
    """Validate ZipInfo-like objects and return canonical-name -> info."""
    infos = list(infos)
    if len(infos) > MAX_ARCHIVE_ENTRIES:
        raise ArchiveValidationError(
            "包内条目过多（最多 %d 个）" % MAX_ARCHIVE_ENTRIES
        )
    total = 0
    entries = {}
    case_names = {}
    file_names = set()
    directory_names = set()
    for info in infos:
        name = canonical_archive_name(info.filename)
        size = info.file_size
        if size < 0 or size > MAX_ENTRY_BYTES:
            raise ArchiveValidationError("包内条目超过 32 MiB 上限：%s" % name)
        if (
            not name.endswith("/")
            and name.lower().endswith((".json", ".lua", ".sha256", ".txt"))
            and size > MAX_TEXT_BYTES
        ):
            raise ArchiveValidationError("包内文本条目过大（最多 4 MiB）：%s" % name)
        if total > MAX_ARCHIVE_UNCOMPRESSED - size:
            raise ArchiveValidationError("包解压后总大小超过 128 MiB")
        total += size
        if name in entries:
            raise ArchiveValidationError("包内存在重复路径：%s" % name)
        folded = name.casefold()
        previous = case_names.get(folded)
        if previous is not None:
            raise ArchiveValidationError(
                "包内存在大小写冲突路径：%s / %s" % (previous, name)
            )
        case_names[folded] = name
        entries[name] = info
        if name.endswith("/"):
            directory_names.add(name[:-1].casefold())
        else:
            file_names.add(folded)

    # Reject archives where one entry is a file but another treats it as a
    # directory (``assets`` plus ``assets/a.png``).  Extraction behavior for
    # these differs across ZIP libraries and filesystems.
    for name in case_names.values():
        body = name[:-1] if name.endswith("/") else name
        parts = body.split("/")
        for index in range(1, len(parts)):
            prefix = "/".join(parts[:index]).casefold()
            if prefix in file_names:
                raise ArchiveValidationError(
                    "包内路径同时被用作文件和目录：%s" % "/".join(parts[:index])
                )
    if file_names & directory_names:
        conflict = sorted(file_names & directory_names)[0]
        raise ArchiveValidationError("包内路径同时被用作文件和目录：%s" % conflict)
    return entries
