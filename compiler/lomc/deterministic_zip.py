# -*- coding: utf-8 -*-
"""Deterministic package assembly and compression-independent content hashing."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import tempfile
import zipfile

from .errors import LomcError


PACKAGE_CONTENT_HASH_ENTRY = "package-content.sha256"
CONTENT_HASH_ALGORITHM = "lom-entry-sha256-v1"
_FIXED_ZIP_TIME = (1980, 1, 1, 0, 0, 0)
_CHUNK = 1024 * 1024


def stable_json_bytes(value: object) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    ).encode("utf-8")


def _hash_header(digest, name: str, size: int) -> None:
    encoded = name.encode("utf-8")
    digest.update(len(encoded).to_bytes(4, "big"))
    digest.update(encoded)
    digest.update(size.to_bytes(8, "big"))


def _zip_info(name: str) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, _FIXED_ZIP_TIME)
    info.compress_type = zipfile.ZIP_DEFLATED
    info.create_system = 3
    info.external_attr = (0o100644 & 0xFFFF) << 16
    info.flag_bits |= 0x800
    return info


class DeterministicPackageBuilder:
    def __init__(self) -> None:
        self._entries: dict[str, tuple[str, object]] = {}

    def _add(self, name: str, kind: str, value: object) -> None:
        normalized = str(name).replace("\\", "/")
        if not normalized or normalized.startswith("/") or ".." in normalized.split("/"):
            raise LomcError("不安全的包内路径：%r" % name)
        if normalized == PACKAGE_CONTENT_HASH_ENTRY:
            raise LomcError("package-content.sha256 由打包器保留")
        if normalized in self._entries:
            raise LomcError("重复的包内路径：%s" % normalized)
        self._entries[normalized] = (kind, value)

    def add_bytes(self, name: str, data: bytes | str) -> None:
        payload = data.encode("utf-8") if isinstance(data, str) else bytes(data)
        self._add(name, "bytes", payload)

    def add_json(self, name: str, value: object) -> None:
        self.add_bytes(name, stable_json_bytes(value))

    def add_file(self, name: str, source: str | Path) -> None:
        path = Path(source)
        if not path.is_file():
            raise LomcError("打包文件不存在：%s" % path)
        self._add(name, "file", path)

    def write(self, output: str | Path) -> tuple[str, str]:
        destination = Path(output)
        destination.parent.mkdir(parents=True, exist_ok=True)
        temp_path: Path | None = None
        digest = hashlib.sha256()
        try:
            fd, temp_name = tempfile.mkstemp(
                prefix=destination.name + ".", suffix=".package.tmp",
                dir=str(destination.parent),
            )
            os.close(fd)
            temp_path = Path(temp_name)
            with zipfile.ZipFile(
                temp_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
            ) as archive:
                for name in sorted(self._entries):
                    kind, value = self._entries[name]
                    if kind == "bytes":
                        payload = value
                        _hash_header(digest, name, len(payload))
                        digest.update(payload)
                        archive.writestr(_zip_info(name), payload)
                        continue
                    source = value
                    expected = source.stat().st_size
                    _hash_header(digest, name, expected)
                    actual = 0
                    with source.open("rb") as reader, archive.open(
                        _zip_info(name), "w", force_zip64=True
                    ) as writer:
                        for chunk in iter(lambda: reader.read(_CHUNK), b""):
                            actual += len(chunk)
                            digest.update(chunk)
                            writer.write(chunk)
                    if actual != expected:
                        raise LomcError("打包期间文件大小发生变化：%s" % source)
                content_hash = digest.hexdigest().upper()
                record = (
                    "algorithm=" + CONTENT_HASH_ALGORITHM + "\n"
                    "sha256=" + content_hash + "\n"
                ).encode("ascii")
                archive.writestr(_zip_info(PACKAGE_CONTENT_HASH_ENTRY), record)
            os.replace(temp_path, destination)
            temp_path = None
            return str(destination), content_hash
        finally:
            if temp_path is not None:
                try:
                    temp_path.unlink()
                except OSError:
                    pass


def package_content_hash(path: str | Path) -> str:
    """Recompute and verify the logical entry hash from an existing package."""
    digest = hashlib.sha256()
    with zipfile.ZipFile(path) as archive:
        names = sorted(
            info.filename for info in archive.infolist()
            if not info.is_dir() and info.filename != PACKAGE_CONTENT_HASH_ENTRY
        )
        if len(names) != len(set(names)):
            raise LomcError("包内存在重复路径，无法计算内容哈希")
        for name in names:
            info = archive.getinfo(name)
            _hash_header(digest, name, info.file_size)
            with archive.open(info) as stream:
                for chunk in iter(lambda: stream.read(_CHUNK), b""):
                    digest.update(chunk)
    return digest.hexdigest().upper()
