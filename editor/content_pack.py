# -*- coding: utf-8 -*-
"""Offline, deterministic sharing packages for one user-content record."""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import tempfile
import unicodedata
import zipfile

import content_registry
from content_registry import ContentRegistryError
from lomc.content import (
    AUDIO_EXTENSIONS,
    CONTENT_SCHEMA,
    IMAGE_EXTENSIONS,
    MAX_AUDIO_BYTES,
    MAX_IMAGE_BYTES,
    content_metadata_payload,
    listed_content_files,
    load_content_metadata,
    normalize_content_metadata,
    package_content_dir,
    validate_content_id,
)
from lomc.deterministic_zip import (
    CONTENT_HASH_ALGORITHM,
    PACKAGE_CONTENT_HASH_ENTRY,
    DeterministicPackageBuilder,
    package_content_hash,
    stable_json_bytes,
)
from lomc.errors import LomcError
from package_io import (
    MAX_ARCHIVE_ENTRIES,
    MAX_ARCHIVE_UNCOMPRESSED,
    PackError,
    _safe_archive_name,
)


CONTENT_PACK_FORMAT = 1
CONTENT_PACK_MANIFEST = "content-pack.json"
CONTENT_PACK_META_FILE = ".lomcontent.json"
MAX_CONTENT_PACK_BYTES = 128 * 1024 * 1024
MAX_MANIFEST_BYTES = 4 * 1024 * 1024
_SEMVER_RE = re.compile(
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)


@dataclass(frozen=True)
class ContentPackInfo:
    path: Path
    content_id: str
    content_type: str
    version: str
    author: str
    license: str
    name: str
    files: tuple[dict, ...]
    package_sha256: str
    logical_content_hash: str
    dependencies: tuple[str, ...] = ()
    missing_dependencies: tuple[str, ...] = ()
    collision_type: str | None = None


def _visible_text(field: str, value: object, limit: int) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ContentRegistryError(f"{field} 必须是非空文本")
    text = value.strip()
    if len(text) > limit:
        raise ContentRegistryError(f"{field} 不能超过 {limit} 个字符")
    if any(
        char in "\r\n" or unicodedata.category(char) in ("Cc", "Cf", "Zl", "Zp")
        for char in text
    ):
        raise ContentRegistryError(f"{field} 不能含换行、控制、零宽或双向格式字符")
    return text


def _validate_version(value: object) -> str:
    if (
        not isinstance(value, str)
        or len(value) > 128
        or _SEMVER_RE.fullmatch(value) is None
    ):
        raise ContentRegistryError("内容版本必须是 SemVer，例如 1.0.0 或 1.1.0-beta.1")
    prerelease = value.split("+", 1)[0].partition("-")[2]
    if any(part.isdigit() and len(part) > 1 and part.startswith("0") for part in prerelease.split(".") if part):
        raise ContentRegistryError("SemVer 的数字预发布标识不能有前导零")
    return value


def _file_sha256(path: Path) -> tuple[int, str]:
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            size += len(chunk)
            digest.update(chunk)
    return size, digest.hexdigest().upper()


def _raw_package_sha256(path: Path) -> str:
    return _file_sha256(path)[1]


def _collision_type(content_id: str) -> str | None:
    root = content_registry.repository_root()
    for content_type in ("audio", "character", "image"):
        folder = root / Path(package_content_dir(content_type, content_id))
        if folder.exists():
            return content_type
    return None


def _dependency_available(content_id: str) -> bool:
    try:
        content_registry.get(content_id)
        return True
    except ContentRegistryError:
        return False


def _sharing_defaults(record: content_registry.ContentRecord) -> dict:
    path = record.folder / CONTENT_PACK_META_FILE
    if path.is_file():
        try:
            value = json.loads(path.read_text(encoding="utf-8-sig"))
            if isinstance(value, dict):
                return value
        except (OSError, UnicodeError, json.JSONDecodeError):
            pass
    return {}


def content_pack_defaults(content_id: str) -> dict:
    record = content_registry.get(content_id)
    stored = _sharing_defaults(record)
    try:
        dependencies = list(
            _normalize_dependencies(stored.get("dependencies", []), record.content_id)
        )
    except ContentRegistryError:
        dependencies = []
    return {
        "version": str(stored.get("version") or "1.0.0"),
        "author": str(stored.get("author") or content_registry.default_namespace()),
        "license": str(stored.get("license") or "All Rights Reserved"),
        "dependencies": dependencies,
    }


def _normalize_dependencies(raw, self_id: str) -> tuple[str, ...]:
    if raw is None:
        return ()
    if not isinstance(raw, (list, tuple)):
        raise ContentRegistryError("dependencies 必须是内容 ID 列表")
    found = set()
    for value in raw:
        if not isinstance(value, str):
            raise ContentRegistryError("dependencies 的每一项必须是内容 ID 文本")
        content_id = value.strip()
        if content_id.startswith("user:"):
            content_id = content_id[5:]
        try:
            validate_content_id(content_id)
        except LomcError as exc:
            raise ContentRegistryError(f"依赖 {value!r} 无效：{exc}") from exc
        if content_id == self_id:
            raise ContentRegistryError("内容包不能依赖自己")
        found.add(content_id)
        if len(found) > 128:
            raise ContentRegistryError("直接依赖不能超过 128 项")
    return tuple(sorted(found))


def export_content_pack(
    path: str | Path,
    content_id: str,
    *,
    version: str,
    author: str,
    license_name: str,
    dependencies=(),
) -> ContentPackInfo:
    """Export exactly one registry record and only its declared files."""
    version = _validate_version(version)
    author = _visible_text("作者", author, 96)
    license_name = _visible_text("许可证", license_name, 128)
    record = content_registry.get(content_id)
    display_name = _visible_text("显示名称", record.name, 128)
    dependencies = _normalize_dependencies(dependencies, record.content_id)
    try:
        metadata = load_content_metadata(str(record.folder / "content.json"))
    except LomcError as exc:
        raise ContentRegistryError(str(exc)) from exc
    metadata_payload = content_metadata_payload(metadata)
    file_rows = []
    sources: list[tuple[str, Path]] = []
    for filename in listed_content_files(metadata):
        source = record.folder / filename
        if not source.is_file():
            raise ContentRegistryError(f"内容文件不存在：{filename}")
        size, digest = _file_sha256(source)
        file_rows.append(
            {"path": "files/" + filename, "size": size, "sha256": digest}
        )
        sources.append((filename, source))
    manifest = {
        "content_pack_format": CONTENT_PACK_FORMAT,
        "content_schema": CONTENT_SCHEMA,
        "id": record.content_id,
        "type": record.type,
        "name": display_name,
        "version": version,
        "author": author,
        "license": license_name,
        "dependencies": list(dependencies),
        "metadata": metadata_payload,
        "files": file_rows,
    }
    builder = DeterministicPackageBuilder()
    builder.add_json(CONTENT_PACK_MANIFEST, manifest)
    for filename, source in sources:
        builder.add_file("files/" + filename, source)
    destination, logical_hash = builder.write(path)
    output = Path(destination)
    return ContentPackInfo(
        path=output,
        content_id=record.content_id,
        content_type=record.type,
        version=version,
        author=author,
        license=license_name,
        name=display_name,
        files=tuple(file_rows),
        package_sha256=_raw_package_sha256(output),
        logical_content_hash=logical_hash,
        dependencies=dependencies,
    )


def _load_manifest(archive: zipfile.ZipFile, entries: dict[str, zipfile.ZipInfo]) -> dict:
    info = entries.get(CONTENT_PACK_MANIFEST)
    if info is None:
        raise ContentRegistryError(f"内容包缺少 {CONTENT_PACK_MANIFEST}")
    if info.file_size < 0 or info.file_size > MAX_MANIFEST_BYTES:
        raise ContentRegistryError("content-pack.json 超过 4 MiB 上限")
    try:
        raw = archive.read(info)
        value = json.loads(raw.decode("utf-8-sig"))
    except (OSError, RuntimeError, UnicodeError, json.JSONDecodeError, zipfile.BadZipFile) as exc:
        raise ContentRegistryError(f"content-pack.json 无法读取：{exc}") from exc
    if not isinstance(value, dict):
        raise ContentRegistryError("content-pack.json 顶层必须是对象")
    return value


def _validate_manifest(
    manifest: dict, entry_names: set[str]
) -> tuple[dict, tuple[dict, ...], tuple[str, ...]]:
    if manifest.get("content_pack_format") != CONTENT_PACK_FORMAT or isinstance(
        manifest.get("content_pack_format"), bool
    ):
        raise ContentRegistryError("只支持 content_pack_format=1")
    if manifest.get("content_schema") != CONTENT_SCHEMA or isinstance(
        manifest.get("content_schema"), bool
    ):
        raise ContentRegistryError(f"只支持 content_schema={CONTENT_SCHEMA}")
    content_id = manifest.get("id")
    try:
        validate_content_id(content_id)
    except LomcError as exc:
        raise ContentRegistryError(str(exc)) from exc
    content_type = manifest.get("type")
    if content_type not in ("audio", "character", "image"):
        raise ContentRegistryError("内容包 type 必须是 audio、character 或 image")
    _validate_version(manifest.get("version"))
    _visible_text("作者", manifest.get("author"), 96)
    _visible_text("许可证", manifest.get("license"), 128)
    _visible_text("显示名称", manifest.get("name"), 128)
    try:
        metadata = normalize_content_metadata(
            manifest.get("metadata"), source="content-pack.json.metadata"
        )
    except LomcError as exc:
        raise ContentRegistryError(str(exc)) from exc
    if metadata["id"] != content_id or metadata["type"] != content_type:
        raise ContentRegistryError("内容包 id/type 与 metadata 不一致")
    dependencies = _normalize_dependencies(manifest.get("dependencies", []), content_id)
    rows = manifest.get("files")
    if not isinstance(rows, list) or not rows:
        raise ContentRegistryError("内容包 files 必须是非空列表")
    expected_files = set(listed_content_files(metadata))
    declared_files: set[str] = set()
    normalized_rows = []
    for index, row in enumerate(rows):
        if not isinstance(row, dict):
            raise ContentRegistryError(f"files 第 {index + 1} 项必须是对象")
        path = row.get("path")
        if not isinstance(path, str):
            raise ContentRegistryError(f"files 第 {index + 1} 项缺少 path")
        try:
            canonical = _safe_archive_name(path)
        except PackError as exc:
            raise ContentRegistryError(str(exc)) from exc
        parts = PurePosixPath(canonical).parts
        if len(parts) != 2 or parts[0] != "files":
            raise ContentRegistryError(f"内容文件必须位于 files/ 且不能有子目录：{path}")
        filename = parts[1]
        if filename in declared_files:
            raise ContentRegistryError(f"内容包重复声明文件：{filename}")
        declared_files.add(filename)
        size = row.get("size")
        digest = row.get("sha256")
        if isinstance(size, bool) or not isinstance(size, int) or size < 0:
            raise ContentRegistryError(f"{path} 的 size 必须是非负整数")
        if not isinstance(digest, str) or re.fullmatch(r"[0-9A-Fa-f]{64}", digest) is None:
            raise ContentRegistryError(f"{path} 的 sha256 必须是 64 位十六进制")
        if canonical not in entry_names:
            raise ContentRegistryError(f"内容包缺少已声明文件：{canonical}")
        normalized_rows.append(
            {"path": canonical, "size": size, "sha256": digest.upper()}
        )
    if declared_files != expected_files:
        missing = sorted(expected_files - declared_files)
        extra = sorted(declared_files - expected_files)
        raise ContentRegistryError(
            "files 与 metadata 不一致；缺少=%s，多余=%s" % (missing, extra)
        )
    allowed = {CONTENT_PACK_MANIFEST, PACKAGE_CONTENT_HASH_ENTRY} | {
        row["path"] for row in normalized_rows
    }
    unknown = sorted(entry_names - allowed)
    if unknown:
        raise ContentRegistryError("内容包含未声明条目：" + "、".join(unknown))
    return metadata, tuple(normalized_rows), dependencies


def inspect_content_pack(path: str | Path) -> ContentPackInfo:
    """Validate all metadata, hashes and files without installing anything."""
    package = Path(path)
    try:
        package_size = package.stat().st_size
    except OSError as exc:
        raise ContentRegistryError(f"无法读取内容包：{exc}") from exc
    if package_size > MAX_CONTENT_PACK_BYTES:
        raise ContentRegistryError("内容包超过 128 MiB 上限")
    try:
        archive = zipfile.ZipFile(package)
    except (OSError, zipfile.BadZipFile) as exc:
        raise ContentRegistryError(f"不是合法的 .lomcontent（zip）：{exc}") from exc
    with archive:
        infos = archive.infolist()
        if len(infos) > MAX_ARCHIVE_ENTRIES:
            raise ContentRegistryError(f"内容包条目过多（最多 {MAX_ARCHIVE_ENTRIES}）")
        entries: dict[str, zipfile.ZipInfo] = {}
        total = 0
        for info in infos:
            try:
                name = _safe_archive_name(info.filename)
            except PackError as exc:
                raise ContentRegistryError(str(exc)) from exc
            if name in entries:
                raise ContentRegistryError(f"内容包存在重复路径：{name}")
            entries[name] = info
            total += info.file_size
            if info.file_size < 0 or total > MAX_ARCHIVE_UNCOMPRESSED:
                raise ContentRegistryError("内容包解压后总大小超过 128 MiB")
        manifest = _load_manifest(archive, entries)
        metadata, rows, dependencies = _validate_manifest(
            manifest, {name for name, info in entries.items() if not info.is_dir()}
        )
        for row in rows:
            info = entries[row["path"]]
            if info.file_size != row["size"]:
                raise ContentRegistryError(f"{row['path']} 的实际大小与声明不一致")
            limit = MAX_AUDIO_BYTES if metadata["type"] == "audio" else MAX_IMAGE_BYTES
            if info.file_size > limit:
                raise ContentRegistryError(
                    f"{row['path']} 超过 {limit // (1024 * 1024)} MiB 上限"
                )
            digest = hashlib.sha256()
            try:
                with archive.open(info) as stream:
                    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                        digest.update(chunk)
            except (OSError, RuntimeError, NotImplementedError, zipfile.BadZipFile) as exc:
                raise ContentRegistryError(f"无法读取 {row['path']}：{exc}") from exc
            if digest.hexdigest().upper() != row["sha256"]:
                raise ContentRegistryError(f"{row['path']} 的 SHA-256 与声明不一致")
        hash_info = entries.get(PACKAGE_CONTENT_HASH_ENTRY)
        if hash_info is None or hash_info.file_size > 512:
            raise ContentRegistryError("内容包缺少有效的 package-content.sha256")
        try:
            record = archive.read(hash_info).decode("ascii")
        except (OSError, RuntimeError, UnicodeError, zipfile.BadZipFile) as exc:
            raise ContentRegistryError(f"无法读取 package-content.sha256：{exc}") from exc
    fields = {}
    for line in record.splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            fields[key.strip()] = value.strip()
    try:
        computed = package_content_hash(package)
    except (OSError, RuntimeError, zipfile.BadZipFile, LomcError) as exc:
        raise ContentRegistryError(f"无法复算内容包逻辑哈希：{exc}") from exc
    declared = str(fields.get("sha256") or "").upper()
    if fields.get("algorithm") != CONTENT_HASH_ALGORITHM or declared != computed:
        raise ContentRegistryError("内容包逻辑内容哈希无效或不匹配")
    missing_dependencies = tuple(
        content_id for content_id in dependencies if not _dependency_available(content_id)
    )
    return ContentPackInfo(
        path=package,
        content_id=manifest["id"],
        content_type=manifest["type"],
        version=manifest["version"],
        author=manifest["author"].strip(),
        license=manifest["license"].strip(),
        name=manifest["name"].strip(),
        files=rows,
        package_sha256=_raw_package_sha256(package),
        logical_content_hash=computed,
        dependencies=dependencies,
        missing_dependencies=missing_dependencies,
        collision_type=_collision_type(manifest["id"]),
    )


def import_content_pack(path: str | Path) -> ContentPackInfo:
    """Install a validated pack atomically; any ID collision is a hard stop."""
    info = inspect_content_pack(path)
    if info.collision_type is not None:
        raise ContentRegistryError(
            f"内容 ID user:{info.content_id} 已存在（类型 {info.collision_type}）；"
            "为避免静默覆盖，请删除旧内容或让作者更换 ID。"
        )
    root = content_registry.repository_root()
    try:
        root.mkdir(parents=True, exist_ok=True)
        stage_root = Path(tempfile.mkdtemp(prefix=".lomcontent-", dir=str(root)))
    except OSError as exc:
        raise ContentRegistryError(f"无法准备内容库导入目录：{exc}") from exc
    stage_content = stage_root / "content"
    stage_content.mkdir()
    target = root / Path(package_content_dir(info.content_type, info.content_id))
    try:
        try:
            if info.path.stat().st_size > MAX_CONTENT_PACK_BYTES:
                raise ContentRegistryError("内容包在校验后超过 128 MiB，未安装内容。")
        except OSError as exc:
            raise ContentRegistryError(f"无法重新确认内容包：{exc}") from exc
        if _raw_package_sha256(info.path) != info.package_sha256:
            raise ContentRegistryError("内容包在校验后发生变化，未安装内容。")
        with zipfile.ZipFile(info.path) as archive:
            manifest_info = archive.getinfo(CONTENT_PACK_MANIFEST)
            if manifest_info.file_size < 0 or manifest_info.file_size > MAX_MANIFEST_BYTES:
                raise ContentRegistryError("content-pack.json 在导入期间发生变化")
            manifest = json.loads(archive.read(manifest_info).decode("utf-8-sig"))
            metadata = normalize_content_metadata(manifest["metadata"])
            (stage_content / "content.json").write_bytes(
                stable_json_bytes(content_metadata_payload(metadata))
            )
            (stage_content / CONTENT_PACK_META_FILE).write_bytes(
                stable_json_bytes(
                    {
                        "content_pack_format": CONTENT_PACK_FORMAT,
                        "version": info.version,
                        "author": info.author,
                        "license": info.license,
                        "dependencies": list(info.dependencies),
                        "package_sha256": info.package_sha256,
                        "logical_content_hash": info.logical_content_hash,
                    }
                )
            )
            for row in info.files:
                filename = PurePosixPath(row["path"]).name
                target_file = stage_content / filename
                digest = hashlib.sha256()
                size = 0
                current_info = archive.getinfo(row["path"])
                if current_info.file_size != row["size"]:
                    raise ContentRegistryError(
                        f"{row['path']} 在导入期间大小发生变化，未安装内容。"
                    )
                with archive.open(current_info) as reader, target_file.open("wb") as writer:
                    for chunk in iter(lambda: reader.read(1024 * 1024), b""):
                        size += len(chunk)
                        digest.update(chunk)
                        writer.write(chunk)
                if size != row["size"] or digest.hexdigest().upper() != row["sha256"]:
                    raise ContentRegistryError(
                        f"{row['path']} 在导入期间发生变化，未安装内容。"
                    )
        if _raw_package_sha256(info.path) != info.package_sha256:
            raise ContentRegistryError("内容包在校验后发生变化，未安装内容。")
        target.parent.mkdir(parents=True, exist_ok=True)
        if _collision_type(info.content_id) is not None or target.exists():
            raise ContentRegistryError(
                f"内容 ID user:{info.content_id} 在导入期间发生冲突，未覆盖任何文件。"
            )
        try:
            os.replace(stage_content, target)
        except OSError as exc:
            raise ContentRegistryError(f"无法安装内容包：{exc}") from exc
    except ContentRegistryError:
        raise
    except (
        OSError,
        RuntimeError,
        KeyError,
        UnicodeError,
        json.JSONDecodeError,
        zipfile.BadZipFile,
        LomcError,
    ) as exc:
        raise ContentRegistryError(f"内容包在导入期间无法读取，未安装内容：{exc}") from exc
    finally:
        shutil.rmtree(stage_root, ignore_errors=True)
    content_registry.rebuild_index()
    return ContentPackInfo(
        path=info.path,
        content_id=info.content_id,
        content_type=info.content_type,
        version=info.version,
        author=info.author,
        license=info.license,
        name=info.name,
        files=info.files,
        package_sha256=info.package_sha256,
        logical_content_hash=info.logical_content_hash,
        dependencies=info.dependencies,
        missing_dependencies=info.missing_dependencies,
    )
