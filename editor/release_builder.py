# -*- coding: utf-8 -*-
"""Local, offline release build transaction for ``.lommod`` packages."""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import os
from pathlib import Path
import tempfile

from lomc import validate_manifest

import package_io
from preflight import PreflightIssue, run_preflight
from release_preflight import apply_release_profile, validate_release_version
from schema_versions import manifest_versions


@dataclass(frozen=True)
class ReleaseBuildResult:
    package_path: Path
    checksum_path: Path
    package_sha256: str
    package_size: int
    story_count: int
    node_count: int
    warnings: tuple[PreflightIssue, ...]
    compile_report: tuple[str, ...]


class ReleaseBuildBlocked(Exception):
    """The build did not create output because release validation failed."""

    def __init__(self, issues: list[PreflightIssue]):
        self.issues = tuple(issues)
        errors = sum(issue.severity == "error" for issue in issues)
        super().__init__("发布构建已停止：发现 %d 个错误" % errors)


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


def _atomic_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=path.name + ".", suffix=".tmp", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as stream:
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    except Exception:
        try:
            os.unlink(temporary)
        except OSError:
            pass
        raise


def build_release(
    output: str | Path,
    manifest: dict,
    stories: dict[str, dict],
    editor_data: dict,
    runtime_version: str,
    *,
    content_root: str | Path | None = None,
    bundled_assets=None,
) -> ReleaseBuildResult:
    """Validate, preflight, package and checksum a release without publishing it."""
    destination = Path(output)
    if destination.suffix.lower() != ".lommod":
        destination = destination.with_suffix(".lommod")
    destination.parent.mkdir(parents=True, exist_ok=True)

    release_manifest = {**manifest_versions(), **dict(manifest)}
    manifest_issues: list[PreflightIssue] = []
    version_error = validate_release_version(release_manifest.get("version"))
    if version_error is not None:
        manifest_issues.append(PreflightIssue(
            "error", "invalid_release_version", "", "", version_error
        ))
    try:
        validate_manifest(release_manifest, "Release manifest")
    except Exception as exc:
        manifest_issues.append(PreflightIssue(
            "error", "invalid_release_manifest", "", "", str(exc)
        ))
    if manifest_issues:
        raise ReleaseBuildBlocked(manifest_issues)

    entry = release_manifest.get("entry")
    editing_issues = run_preflight(
        stories,
        editor_data,
        entry if isinstance(entry, str) else None,
        manifest=release_manifest,
        content_root=content_root,
    )
    issues = apply_release_profile(
        editing_issues,
        stories,
        release_manifest,
        runtime_version,
        bundled_assets=bundled_assets,
    )
    if any(issue.severity == "error" for issue in issues):
        raise ReleaseBuildBlocked(issues)

    checksum_path = destination.with_name(destination.name + ".sha256")
    with tempfile.TemporaryDirectory(
        prefix="lom_release_", dir=str(destination.parent)
    ) as temporary_dir:
        staged_package = Path(temporary_dir) / destination.name
        compile_report = package_io.export_lommod(
            staged_package, release_manifest, stories
        )
        package_sha256 = _sha256_file(staged_package)
        package_size = staged_package.stat().st_size
        os.replace(staged_package, destination)

    _atomic_text(
        checksum_path,
        "%s  %s\n" % (package_sha256, destination.name),
    )
    return ReleaseBuildResult(
        package_path=destination.resolve(),
        checksum_path=checksum_path.resolve(),
        package_sha256=package_sha256,
        package_size=package_size,
        story_count=len(stories),
        node_count=sum(
            len(story.get("nodes") or [])
            for story in stories.values() if isinstance(story, dict)
        ),
        warnings=tuple(issue for issue in issues if issue.severity == "warning"),
        compile_report=tuple(compile_report),
    )
