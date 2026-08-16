# -*- coding: utf-8 -*-
"""Authoritative versions for the public lom_modkit file contracts."""

PACKAGE_FORMAT = 3
STORY_SCHEMA = 2
CONTENT_SCHEMA = 1


def version_declarations() -> dict[str, int]:
    """Return a fresh manifest declaration for newly written packages."""
    return {
        "package_format": PACKAGE_FORMAT,
        "story_schema": STORY_SCHEMA,
        "content_schema": CONTENT_SCHEMA,
    }
