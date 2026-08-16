# -*- coding: utf-8 -*-
"""Read-only inspection for untrusted ``.lommod`` packages.

The inspector deliberately does not call the normal import path: inspecting a
package must not register bundled user content or mutate the current project.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import hashlib
import json
from pathlib import Path
import re
import zipfile

from app_version import RUNTIME_VERSION
from package_io import (
    MAX_JSON_BYTES,
    PackError,
    _verify_story_lua_pairs,
    _validated_entries,
)
from lomc.package_validation import MAX_PACKAGE_FILE_BYTES
from schema_versions import CONTENT_SCHEMA, PACKAGE_FORMAT, STORY_SCHEMA


MAX_PACKAGE_BYTES = MAX_PACKAGE_FILE_BYTES
MAX_TEXT_PREVIEW_BYTES = MAX_JSON_BYTES
CONTENT_HASH_ENTRY = "package-content.sha256"
CONTENT_HASH_ALGORITHM = "lom-entry-sha256-v1"
_SEMVER_RE = re.compile(
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)


@dataclass(frozen=True)
class InspectedEntry:
    name: str
    category: str
    size: int
    compressed_size: int
    sha256: str
    preview: str = ""
    preview_truncated: bool = False


@dataclass
class PackageInspection:
    path: Path
    package_size: int
    package_sha256: str
    manifest: dict = field(default_factory=dict)
    entries: list[InspectedEntry] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    referenced_assets: list[str] = field(default_factory=list)
    bundled_assets: list[str] = field(default_factory=list)
    missing_assets: list[str] = field(default_factory=list)
    unreferenced_assets: list[str] = field(default_factory=list)
    logical_content_hash: str = ""
    declared_content_hash: str = ""
    content_hash_valid: bool | None = None

    @property
    def stories(self) -> list[InspectedEntry]:
        return [entry for entry in self.entries if entry.category == "Story"]

    @property
    def lua_files(self) -> list[InspectedEntry]:
        return [entry for entry in self.entries if entry.category == "Lua"]

    @property
    def user_contents(self) -> list[InspectedEntry]:
        return [entry for entry in self.entries if entry.category == "User content"]


def _category(name: str) -> str:
    if name == "manifest.json":
        return "Manifest"
    if name == "texts.json" or name.startswith("texts/"):
        return "Texts"
    if name.startswith("story/") and name.endswith(".json"):
        return "Story"
    if name.startswith("lua/") and name.endswith(".lua"):
        return "Lua"
    if name.startswith("assets/user/"):
        return "User content"
    if name.startswith("assets/"):
        return "Asset"
    if name in (CONTENT_HASH_ENTRY, "story-lua.sha256"):
        return "Hash"
    return "Other"


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


def _hash_header(digest, name: str, size: int) -> None:
    encoded = name.encode("utf-8")
    digest.update(len(encoded).to_bytes(4, "big"))
    digest.update(encoded)
    digest.update(size.to_bytes(8, "big"))


def _parse_json(data: bytes, name: str, inspection: PackageInspection):
    try:
        return json.loads(data.decode("utf-8-sig"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        inspection.errors.append(f"{name} 不是合法 JSON：{exc}")
        return None


def _parse_semver(value: object):
    if not isinstance(value, str):
        return None
    match = _SEMVER_RE.fullmatch(value)
    if match is None:
        return None
    return tuple(int(match.group(i)) for i in (1, 2, 3)), match.group(4)


def _semver_compare(left, right) -> int:
    if left[0] != right[0]:
        return 1 if left[0] > right[0] else -1
    lpre, rpre = left[1], right[1]
    if lpre is None or rpre is None:
        return 0 if lpre == rpre else (1 if lpre is None else -1)
    lparts, rparts = lpre.split("."), rpre.split(".")
    for lpart, rpart in zip(lparts, rparts):
        if lpart == rpart:
            continue
        ln, rn = lpart.isdigit(), rpart.isdigit()
        if ln and rn:
            return 1 if int(lpart) > int(rpart) else -1
        if ln != rn:
            return -1 if ln else 1
        return 1 if lpart > rpart else -1
    return (len(lparts) > len(rparts)) - (len(lparts) < len(rparts))


def _compatibility_checks(
    manifest: dict,
    inspection: PackageInspection,
    current_host_version: str,
    current_game_version: str | None,
) -> None:
    declared = manifest.get("package_format", manifest.get("format"))
    for field_name, value, current in (
        ("package_format", declared, PACKAGE_FORMAT),
        ("story_schema", manifest.get("story_schema", STORY_SCHEMA), STORY_SCHEMA),
        ("content_schema", manifest.get("content_schema", CONTENT_SCHEMA), CONTENT_SCHEMA),
    ):
        accepted = (1, current) if field_name == "package_format" else (current,)
        if isinstance(value, bool) or value not in accepted:
            inspection.errors.append(
                f"不支持的 {field_name}：{value!r}（检查器支持 {current}）"
            )
    current_host = _parse_semver(current_host_version)
    minimum = _parse_semver(manifest.get("min_host_version"))
    tested = _parse_semver(manifest.get("tested_host_version"))
    if "min_host_version" in manifest and minimum is None:
        inspection.errors.append("min_host_version 不是合法 SemVer")
    elif minimum is not None and current_host is not None and _semver_compare(current_host, minimum) < 0:
        inspection.errors.append(
            f"需要 MortalModHost {manifest['min_host_version']} 或更高；当前随附版本为 {current_host_version}"
        )
    if "tested_host_version" in manifest and tested is None:
        inspection.errors.append("tested_host_version 不是合法 SemVer")
    elif tested is not None and current_host is not None and _semver_compare(current_host, tested) > 0:
        inspection.warnings.append(
            f"作者只测试到 Host {manifest['tested_host_version']}；当前随附版本为 {current_host_version}"
        )
    required_game = manifest.get("game_version")
    tested_game = manifest.get("tested_game_version")
    if current_game_version:
        if isinstance(required_game, str) and required_game.casefold() != current_game_version.casefold():
            inspection.errors.append(
                f"要求游戏版本 {required_game}；当前检测到 {current_game_version}"
            )
        elif isinstance(tested_game, str) and tested_game.casefold() != current_game_version.casefold():
            inspection.warnings.append(
                f"作者测试游戏版本为 {tested_game}；当前检测到 {current_game_version}"
            )
    elif required_game or tested_game:
        inspection.warnings.append("包声明了游戏版本兼容范围；未提供本机游戏版本，无法比对")


def _story_references(story: dict, direct: set[str], user_ids: set[str]) -> None:
    """Collect only fields that the Story contract defines as resource refs."""
    for node in story.get("nodes") or []:
        if not isinstance(node, dict):
            continue
        value = node.get("image")
        if isinstance(value, str):
            normalized = value.replace("\\", "/")
            if normalized.startswith("assets/"):
                direct.add(normalized)
    try:
        from lomc.content import collect_story_content_refs
        for item in collect_story_content_refs(story):
            user_ids.add(item["ref"].content_id)
    except Exception:
        # Contract validation already reports malformed refs; retain a conservative
        # fallback so the package view still opens and shows useful inventory.
        for node in story.get("nodes") or []:
            if not isinstance(node, dict):
                continue
            for field_name in ("name", "image", "voice", "character"):
                value = node.get(field_name)
                if isinstance(value, str) and value.startswith("user:") and len(value) > 5:
                    user_ids.add(value[5:])


def inspect_lommod(
    path: str | Path,
    *,
    current_host_version: str = RUNTIME_VERSION,
    current_game_version: str | None = None,
) -> PackageInspection:
    """Inspect a package without extracting files or changing editor state."""
    package = Path(path)
    try:
        package_size = package.stat().st_size
    except OSError as exc:
        raise PackError(f"无法读取 {package.name}：{exc}") from exc
    if package_size > MAX_PACKAGE_BYTES:
        raise PackError("Mod 包文件超过 160 MiB 上限")
    inspection = PackageInspection(package, package_size, _sha256_file(package))
    try:
        archive = zipfile.ZipFile(package)
    except (OSError, zipfile.BadZipFile) as exc:
        raise PackError(f"{package.name} 不是合法的 .lommod（zip）：{exc}") from exc
    raw_documents: dict[str, object] = {}
    logical_digest = hashlib.sha256()
    with archive:
        try:
            entries = _validated_entries(archive)
        except (ValueError, OSError, zipfile.BadZipFile, PackError) as exc:
            raise PackError(f"无法检查包内容：{exc}") from exc
        for name in sorted(entries):
            info = entries[name]
            if info.is_dir():
                continue
            digest = hashlib.sha256()
            preview_bytes = bytearray()
            try:
                with archive.open(info) as stream:
                    while True:
                        chunk = stream.read(1024 * 1024)
                        if not chunk:
                            break
                        digest.update(chunk)
                        if len(preview_bytes) < MAX_TEXT_PREVIEW_BYTES:
                            preview_bytes.extend(chunk[: MAX_TEXT_PREVIEW_BYTES - len(preview_bytes)])
            except (OSError, RuntimeError, zipfile.BadZipFile) as exc:
                inspection.errors.append(f"无法读取 {name}：{exc}")
                continue
            preview = ""
            if _category(name) in ("Manifest", "Story", "Lua", "Texts", "Hash") or name.endswith("/content.json"):
                try:
                    preview = bytes(preview_bytes).decode("utf-8-sig")
                except UnicodeDecodeError:
                    inspection.errors.append(f"{name} 不是 UTF-8 文本")
            inspection.entries.append(
                InspectedEntry(
                    name, _category(name), info.file_size, info.compress_size,
                    digest.hexdigest().upper(), preview,
                    info.file_size > MAX_TEXT_PREVIEW_BYTES,
                )
            )
            if name.endswith(".json") and info.file_size <= MAX_JSON_BYTES:
                raw_documents[name] = _parse_json(bytes(preview_bytes), name, inspection)

        # Calculate the framed logical hash in one clear, independent pass.
        logical_digest = hashlib.sha256()
        for name in sorted(entries):
            info = entries[name]
            if info.is_dir() or name == CONTENT_HASH_ENTRY:
                continue
            _hash_header(logical_digest, name, info.file_size)
            try:
                with archive.open(info) as stream:
                    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                        logical_digest.update(chunk)
            except (OSError, RuntimeError, NotImplementedError, zipfile.BadZipFile) as exc:
                raise PackError(f"无法计算 {name} 的逻辑内容哈希：{exc}") from exc
        inspection.logical_content_hash = logical_digest.hexdigest().upper()

    manifest = raw_documents.get("manifest.json")
    if not isinstance(manifest, dict):
        inspection.errors.append("包内缺少合法的 manifest.json")
    else:
        inspection.manifest = manifest
        _compatibility_checks(
            manifest, inspection, current_host_version, current_game_version
        )
        try:
            from lomc.validate import validate_manifest
            validate_manifest(manifest)
        except Exception as exc:
            inspection.errors.append(f"manifest.json 格式错误：{exc}")

    stories = {
        name: value for name, value in raw_documents.items()
        if name.startswith("story/") and name.endswith(".json") and isinstance(value, dict)
    }
    if not stories:
        inspection.errors.append("包内没有合法的 story/*.json")
    elif isinstance(manifest, dict):
        story_by_id = {Path(name).stem: value for name, value in stories.items()}
        try:
            with zipfile.ZipFile(package) as verification_archive:
                verification_entries = _validated_entries(verification_archive)
                _verify_story_lua_pairs(
                    verification_archive,
                    verification_entries,
                    manifest,
                    story_by_id,
                    require_integrity=(
                        manifest.get("package_format", manifest.get("format"))
                        == PACKAGE_FORMAT
                    ),
                )
                if "story-lua.sha256" not in verification_entries:
                    inspection.warnings.append(
                        "包未包含 story-lua.sha256（旧包允许，已通过现场复编译核对）"
                    )
        except (OSError, zipfile.BadZipFile, PackError) as exc:
            inspection.errors.append("Story/Lua 一致性校验失败：%s" % exc)
    try:
        from lomc.validate import validate_story
        for name, story in stories.items():
            try:
                validate_story(story, source=name)
            except Exception as exc:
                inspection.errors.append(f"{name} 格式错误：{exc}")
    except ImportError:
        inspection.warnings.append("编译器不可用，未执行 Story 契约校验")

    entry_names = {entry.name for entry in inspection.entries}
    if isinstance(manifest, dict):
        entry = manifest.get("entry")
        if isinstance(entry, str):
            for expected in (f"story/{entry}.json", f"lua/{entry}.lua"):
                if expected not in entry_names:
                    inspection.errors.append(f"入口缺少 {expected}")
    for story_name in stories:
        script_id = Path(story_name).stem
        if f"lua/{script_id}.lua" not in entry_names:
            inspection.errors.append(f"{story_name} 缺少对应 lua/{script_id}.lua")

    texts = raw_documents.get("texts.json")
    if texts is not None and (
        not isinstance(texts, dict)
        or any(not isinstance(key, str) or not isinstance(value, str) for key, value in texts.items())
    ):
        inspection.errors.append("texts.json 必须是字符串到字符串的对象")

    try:
        from lomc.content import listed_content_files, normalize_content_metadata
        for name, document in raw_documents.items():
            parts = name.split("/")
            if len(parts) != 5 or parts[:2] != ["assets", "user"] or parts[4] != "content.json":
                continue
            try:
                metadata = normalize_content_metadata(document, source=name)
            except Exception as exc:
                inspection.errors.append(f"{name} 格式错误：{exc}")
                continue
            root = "/".join(parts[:4])
            if metadata.get("type") != parts[2] or metadata.get("id") != parts[3]:
                inspection.errors.append(f"{name} 的 type/id 与包内路径不一致")
            for filename in listed_content_files(metadata):
                expected = root + "/" + filename
                if expected not in entry_names:
                    inspection.errors.append(f"{name} 引用但缺少 {expected}")
    except ImportError:
        inspection.warnings.append("编译器不可用，未执行用户内容 metadata 校验")

    declared_entry = next((entry for entry in inspection.entries if entry.name == CONTENT_HASH_ENTRY), None)
    if declared_entry is None:
        inspection.warnings.append("包未包含 package-content.sha256（旧包允许，但无法核对逻辑内容）")
    else:
        record = {}
        for line in declared_entry.preview.splitlines():
            if "=" in line:
                key, value = line.split("=", 1)
                record[key.strip()] = value.strip()
        inspection.declared_content_hash = record.get("sha256", "").upper()
        if record.get("algorithm") != CONTENT_HASH_ALGORITHM or not re.fullmatch(
            r"[0-9A-F]{64}", inspection.declared_content_hash
        ):
            inspection.errors.append("package-content.sha256 记录格式无效")
            inspection.content_hash_valid = False
        else:
            inspection.content_hash_valid = (
                inspection.declared_content_hash == inspection.logical_content_hash
            )
            if not inspection.content_hash_valid:
                inspection.errors.append("package-content.sha256 与包内逻辑内容不一致")

    bundled = sorted(
        entry.name for entry in inspection.entries
        if entry.name.startswith("assets/")
    )
    direct_refs: set[str] = set()
    user_ids: set[str] = set()
    for story in stories.values():
        _story_references(story, direct_refs, user_ids)
    referenced = set(direct_refs)
    missing = {ref for ref in direct_refs if ref not in bundled}
    for content_id in user_ids:
        matches = [
            name for name in bundled
            if name.startswith("assets/user/") and f"/{content_id}/" in name
        ]
        if matches:
            referenced.update(matches)
        else:
            missing.add("user:" + content_id)
    inspection.bundled_assets = bundled
    inspection.referenced_assets = sorted(referenced)
    inspection.missing_assets = sorted(missing)
    inspection.unreferenced_assets = sorted(set(bundled) - referenced)
    if inspection.missing_assets:
        inspection.errors.append(
            "引用但未打包的资源：" + "、".join(inspection.missing_assets)
        )
    if inspection.unreferenced_assets:
        inspection.warnings.append(
            f"发现 {len(inspection.unreferenced_assets)} 个已打包但未被 Story 引用的资源"
        )
    return inspection
