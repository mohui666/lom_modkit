# -*- coding: utf-8 -*-
"""Release-only checks layered over the normal editing preflight."""

from __future__ import annotations

from dataclasses import replace
import re

from lomc import validate_manifest
from lomc.localization import SUPPORTED_LOCALES, available_locales

from preflight import PreflightIssue
from project_statistics import unused_asset_paths
from schema_versions import manifest_versions


_SEMVER = re.compile(
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)
_RELEASE_FIELDS = ("id", "name", "version", "author", "description", "entry")
_CRITICAL_EXTENSIONS = {
    ".png", ".jpg", ".jpeg", ".wav", ".ogg", ".mp3", ".flac",
}


def _version(value: object):
    if not isinstance(value, str):
        return None
    match = _SEMVER.fullmatch(value)
    if match is None:
        return None
    return tuple(int(match.group(index)) for index in (1, 2, 3)), match.group(4)


def _greater(left, right) -> bool:
    if left[0] != right[0]:
        return left[0] > right[0]
    if left[1] is None:
        return right[1] is not None
    if right[1] is None:
        return False
    lp = left[1].split(".")
    rp = right[1].split(".")
    for a, b in zip(lp, rp):
        if a == b:
            continue
        an, bn = a.isdigit(), b.isdigit()
        if an and bn:
            return int(a) > int(b)
        if an != bn:
            return not an
        return a > b
    return len(lp) > len(rp)


def validate_release_version(value: object) -> str | None:
    """Return a human-readable error when the public Mod version is not SemVer."""
    parsed = _version(value)
    if parsed is None:
        return "manifest.version 必须是 SemVer（例如 1.2.3 或 2.0.0-beta.1）"
    core, prerelease = parsed
    if any(part > 2147483647 for part in core):
        return "manifest.version 的版本数字不能超过 2147483647"
    for identifier in (prerelease or "").split("."):
        if identifier.isdigit() and (
            (len(identifier) > 1 and identifier.startswith("0"))
            or int(identifier) > 2147483647
        ):
            return "manifest.version 的预发布数字必须无前导零且不超过 2147483647"
    return None


def apply_release_profile(
    editing_issues: list[PreflightIssue],
    stories: dict[str, dict],
    manifest: dict,
    runtime_version: str,
    bundled_assets=None,
) -> list[PreflightIssue]:
    """Add strict checks and upgrade only explicitly release-blocking warnings."""
    issues = [
        replace(issue, severity="error")
        if issue.code == "placeholder_text" and issue.severity == "warning"
        else issue
        for issue in editing_issues
    ]

    missing = [
        field for field in _RELEASE_FIELDS
        if not isinstance(manifest.get(field), str) or not manifest.get(field).strip()
    ]
    for field in missing:
        issues.append(PreflightIssue(
            "error", "missing_release_metadata", "", "",
            "发布元数据缺失：manifest.%s 必须在发布前填写" % field,
        ))
    if not missing:
        release_manifest = {**manifest_versions(), **manifest}
        try:
            validate_manifest(release_manifest, "Release manifest")
        except Exception as exc:
            issues.append(PreflightIssue(
                "error", "invalid_release_manifest", "", "", str(exc)
            ))

    version_error = validate_release_version(manifest.get("version"))
    if version_error is not None:
        issues.append(PreflightIssue(
            "error", "invalid_release_version", "", "", version_error
        ))

    required = _version(manifest.get("min_host_version"))
    current = _version(runtime_version)
    if required is not None and current is not None and _greater(required, current):
        issues.append(PreflightIssue(
            "error", "incompatible_runtime_requirement", "", "",
            "项目要求 MortalModHost %s，但当前随附 Runtime 是 %s"
            % (manifest.get("min_host_version"), runtime_version),
        ))

    for story_id, story in sorted(stories.items()):
        if not isinstance(story, dict) or not isinstance(story.get("localization"), dict):
            continue
        available = set(available_locales(story))
        for locale in SUPPORTED_LOCALES:
            if locale not in available:
                issues.append(PreflightIssue(
                    "warning", "missing_locale", str(story_id), "",
                    "发布本地化缺少 %s；运行时将使用 fallback/default 文本" % locale,
                ))

    if bundled_assets is not None:
        for asset in unused_asset_paths(stories, bundled_assets):
            suffix = "." + asset.rsplit(".", 1)[-1].lower() if "." in asset else ""
            if suffix in _CRITICAL_EXTENSIONS:
                issues.append(PreflightIssue(
                    "warning", "unused_critical_asset", "", "",
                    "发布包含未使用的图片/音频资产：%s" % asset,
                ))

    unique = {
        (item.severity, item.code, item.story_id, item.node_id, item.message): item
        for item in issues
    }
    order = {"error": 0, "warning": 1}
    return sorted(unique.values(), key=lambda item: (
        order.get(item.severity, 9), item.story_id, item.node_id, item.code, item.message
    ))
