# -*- coding: utf-8 -*-
"""Story content localization (independent from the editor UI locale)."""

from __future__ import annotations

import copy

from .errors import LomcError


SUPPORTED_LOCALES = ("chs", "cht", "ja", "ko")
LOCALE_ALIASES = {
    "zh_CN": "chs", "zh-CN": "chs", "zh_Hans": "chs", "zh-Hans": "chs",
    "zh_TW": "cht", "zh-TW": "cht", "zh_Hant": "cht", "zh-Hant": "cht",
}


def normalize_locale(locale):
    """Canonicalize old locale spellings without emitting them in new packages."""
    return LOCALE_ALIASES.get(locale, locale)

# Only author-facing prose is localizable. IDs, asset refs and raw Lua never are.
_NODE_TEXT_FIELDS = {
    "say": ("text",),
    "message": ("text",),
    "intro": ("title", "name", "text"),
    "goto_scene": ("title", "desc"),
    "death": ("title", "text"),
    "dice": ("header", "bonus_name", "bonus_status"),
}


def localization_config(story):
    raw = story.get("localization") if isinstance(story, dict) else None
    if not isinstance(raw, dict):
        return None
    normalized = dict(raw)
    normalized["default_locale"] = normalize_locale(raw.get("default_locale"))
    if "fallback_locale" in raw:
        normalized["fallback_locale"] = normalize_locale(raw.get("fallback_locale"))
    translations = raw.get("translations")
    if isinstance(translations, dict):
        canonical = {}
        for old_locale, catalog in translations.items():
            locale = normalize_locale(old_locale)
            if locale in canonical and isinstance(canonical[locale], dict) and isinstance(catalog, dict):
                merged = dict(canonical[locale])
                merged.update(catalog)
                canonical[locale] = merged
            else:
                canonical[locale] = catalog
        normalized["translations"] = canonical
    return normalized


def iter_localizable_texts(story):
    """Yield ``(stable_path, text)`` for every source-locale author string."""
    title = story.get("title")
    if isinstance(title, str) and title:
        yield "story.title", title
    for node in story.get("nodes") or []:
        if not isinstance(node, dict):
            continue
        node_id = str(node.get("id") or "")
        node_type = str(node.get("type") or "")
        for field in _NODE_TEXT_FIELDS.get(node_type, ()):
            value = node.get(field)
            if isinstance(value, str) and value:
                yield "%s.%s" % (node_id, field), value
        if node_type == "choice":
            for index, option in enumerate(node.get("options") or []):
                if isinstance(option, dict) and isinstance(option.get("text"), str) and option["text"]:
                    yield "%s.options.%d.text" % (node_id, index), option["text"]
        if node_type == "dice":
            if "bands" in node:
                for band_index, band in enumerate(node.get("bands") or []):
                    if isinstance(band, dict) and isinstance(band.get("text"), str) and band["text"]:
                        yield "%s.bands.%d.text" % (node_id, band_index), band["text"]
            else:
                # 已打包的旧 check/options 剧情仍可由编译器读取；编辑器打开时会迁移。
                for option_index, option in enumerate(node.get("options") or []):
                    if not isinstance(option, dict):
                        continue
                    for band_index, value in enumerate(option.get("band_texts") or []):
                        if isinstance(value, str) and value:
                            yield "%s.options.%d.band_texts.%d" % (
                                node_id, option_index, band_index,
                            ), value


def validate_story_localization(story):
    config = localization_config(story)
    if config is None:
        if isinstance(story, dict) and "localization" in story:
            raise LomcError('字段 "localization" 必须是对象')
        return
    allowed = {"default_locale", "fallback_locale", "translations"}
    unknown = set(config) - allowed
    if unknown:
        raise LomcError('localization 含未知字段 "%s"' % sorted(unknown)[0])
    default = config.get("default_locale")
    fallback = config.get("fallback_locale", default)
    if default not in SUPPORTED_LOCALES:
        raise LomcError("localization.default_locale 必须是 %s 之一" % "/".join(SUPPORTED_LOCALES))
    if fallback not in SUPPORTED_LOCALES:
        raise LomcError("localization.fallback_locale 必须是 %s 之一" % "/".join(SUPPORTED_LOCALES))
    translations = config.get("translations", {})
    if not isinstance(translations, dict):
        raise LomcError("localization.translations 必须是对象")
    known_paths = dict(iter_localizable_texts(story))
    for locale, catalog in translations.items():
        if locale not in SUPPORTED_LOCALES:
            raise LomcError("localization.translations 含不支持的 locale %r" % locale)
        if locale == default:
            raise LomcError("默认语言 %s 的文本直接保存在节点字段中，不应重复放入 translations" % default)
        if not isinstance(catalog, dict):
            raise LomcError("localization.translations.%s 必须是对象" % locale)
        for path, value in catalog.items():
            if path not in known_paths:
                raise LomcError("localization.translations.%s 含不存在或不可翻译的路径 %r" % (locale, path))
            if not isinstance(value, str) or not value:
                raise LomcError("localization.translations.%s.%s 必须是非空字符串" % (locale, path))


def available_locales(story):
    config = localization_config(story)
    if config is None:
        return ()
    translations = config.get("translations") or {}
    return tuple(locale for locale in SUPPORTED_LOCALES if locale == config["default_locale"] or locale in translations)


def resolved_catalog(story, locale):
    """Resolve every path using requested → fallback → source/default."""
    validate_story_localization(story)
    source = dict(iter_localizable_texts(story))
    config = localization_config(story)
    if config is None:
        return source
    locale = normalize_locale(locale)
    if locale not in SUPPORTED_LOCALES:
        raise LomcError("不支持的 Story locale: %s" % locale)
    translations = config.get("translations") or {}
    requested = translations.get(locale, {}) if locale != config["default_locale"] else {}
    fallback_locale = config.get("fallback_locale", config["default_locale"])
    fallback = translations.get(fallback_locale, {}) if fallback_locale != config["default_locale"] else {}
    return {path: requested.get(path, fallback.get(path, value)) for path, value in source.items()}


def _assign_path(story, path, value):
    if path == "story.title":
        story["title"] = value
        return
    parts = path.split(".")
    node_id = parts.pop(0)
    node = next(item for item in story.get("nodes") or [] if isinstance(item, dict) and item.get("id") == node_id)
    target = node
    for part in parts[:-1]:
        target = target[int(part)] if isinstance(target, list) else target[part]
    if isinstance(target, list):
        target[int(parts[-1])] = value
    else:
        target[parts[-1]] = value


def apply_story_locale(story, locale):
    localized = copy.deepcopy(story)
    for path, value in resolved_catalog(story, locale).items():
        _assign_path(localized, path, value)
    localized.pop("localization", None)
    return localized
