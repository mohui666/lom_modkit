# -*- coding: utf-8 -*-
"""Versions written by the editor; kept dependency-free for frozen builds."""

PACKAGE_FORMAT = 2
STORY_SCHEMA = 1
CONTENT_SCHEMA = 1


def manifest_versions() -> dict[str, int]:
    return {
        "format": PACKAGE_FORMAT,  # compatibility spelling
        "package_format": PACKAGE_FORMAT,
        "story_schema": STORY_SCHEMA,
        "content_schema": CONTENT_SCHEMA,
    }


def version_value(document: dict, explicit: str, legacy: str | None = None):
    """Read an explicit version, falling back only to its legacy spelling."""
    if explicit in document:
        return document.get(explicit)
    if legacy is not None:
        return document.get(legacy)
    return None


def assert_supported_version(
    document: dict,
    explicit: str,
    current: int,
    *,
    legacy: str | None = None,
    allow_missing: bool = False,
    supported: tuple[int, ...] | None = None,
) -> None:
    value = version_value(document, explicit, legacy)
    has_version = explicit in document or (
        legacy is not None and legacy in document
    )
    if not has_version and allow_missing:
        return
    accepted = supported or (current,)
    if value not in accepted or isinstance(value, bool):
        raise ValueError(
            "不支持的 %s：%r（支持 %s）"
            % (explicit, value, "/".join(str(item) for item in accepted))
        )
    if explicit in document and legacy is not None and legacy in document:
        legacy_value = document.get(legacy)
        if legacy_value != value or isinstance(legacy_value, bool):
            raise ValueError(
                "%s 与旧字段 %s 的版本声明不一致" % (explicit, legacy)
            )
