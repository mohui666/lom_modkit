# -*- coding: utf-8 -*-
"""lomc — 活侠传 mod 剧情编译器（story.json -> 游戏原生 Lua）。

格式契约：docs/zh_CN/mod_format.md（v1）。纯 Python 3 标准库，零第三方依赖。
"""

from .compiler import compile_story, compile_story_file, load_json_file
from .content import (
    collect_stories_content_refs,
    collect_story_content_refs,
    is_user_ref,
    parse_content_ref,
    validate_story_content_refs,
)
from .errors import LomcError
from .deterministic_zip import PACKAGE_CONTENT_HASH_ENTRY, package_content_hash
from .watermark_protocol import (
    PROTOCOL_VERSION as WATERMARK_PROTOCOL_VERSION,
    decode_packet as decode_watermark_packet,
    encode_packet as encode_watermark_packet,
)
from .localization import (
    SUPPORTED_LOCALES, apply_story_locale, available_locales,
    iter_localizable_texts, resolved_catalog, validate_story_localization,
)
from .pack import pack_mod
from .schema_versions import CONTENT_SCHEMA, PACKAGE_FORMAT, STORY_SCHEMA
from .validate import validate_manifest, validate_story

__version__ = "1.0.0"

__all__ = [
    "LomcError",
    "compile_story",
    "compile_story_file",
    "load_json_file",
    "pack_mod",
    "validate_manifest",
    "validate_story",
    "is_user_ref",
    "parse_content_ref",
    "collect_story_content_refs",
    "collect_stories_content_refs",
    "validate_story_content_refs",
    "SUPPORTED_LOCALES",
    "apply_story_locale",
    "available_locales",
    "iter_localizable_texts",
    "resolved_catalog",
    "validate_story_localization",
    "PACKAGE_FORMAT",
    "STORY_SCHEMA",
    "CONTENT_SCHEMA",
    "PACKAGE_CONTENT_HASH_ENTRY",
    "package_content_hash",
    "__version__",
]
