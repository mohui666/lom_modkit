# -*- coding: utf-8 -*-
"""Authoritative versions for the public lom_modkit file contracts."""

PACKAGE_FORMAT = 1
STORY_SCHEMA = 1
CONTENT_SCHEMA = 1


def version_declarations() -> dict[str, int]:
    """Return a fresh manifest declaration for newly written packages."""
    return {
        "package_format": PACKAGE_FORMAT,
        "story_schema": STORY_SCHEMA,
        "content_schema": CONTENT_SCHEMA,
    }
