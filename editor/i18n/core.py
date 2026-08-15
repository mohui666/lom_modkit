# -*- coding: utf-8 -*-
"""界面翻译与游戏名词查询。

游戏名词来源：
- 简中 / 繁中 / 韩语：游戏解包官方语言表
- 日语：LoM-wiki 日文页（人物汉字与繁中同形，属性名见设施页）
wiki 没有的条目回退到解包表。界面文案缺失时回退简体中文。
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

DEFAULT_LANGUAGE = "zh_CN"
LANGUAGES: tuple[tuple[str, str], ...] = (
    ("zh_CN", "简体中文"),
    ("zh_TW", "繁體中文"),
    ("ja", "日本語"),
    ("ko", "한국어"),
)

_QT_QM = {
    "zh_CN": "qt_zh_CN",
    "zh_TW": "qt_zh_TW",
    "ja": "qt_ja",
    "ko": "qt_ko",
}

_language = DEFAULT_LANGUAGE
_ui: dict[str, str] = {}
_ui_fallback: dict[str, str] = {}
_terms: dict[str, dict[str, str]] = {}
_terms_fallback: dict[str, dict[str, str]] = {}
_help_cache: dict[str, str] = {}
_qt_translator = None


def _bundle_dir() -> Path:
    if getattr(sys, "frozen", False) and hasattr(sys, "_MEIPASS"):
        return Path(getattr(sys, "_MEIPASS")) / "i18n"
    return Path(__file__).resolve().parent


def _load_json(path: Path) -> dict:
    if not path.is_file():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


def _flatten_ui(data: dict, prefix: str = "") -> dict[str, str]:
    out: dict[str, str] = {}
    for key, value in data.items():
        full = f"{prefix}{key}" if not prefix else f"{prefix}.{key}"
        if isinstance(value, dict):
            out.update(_flatten_ui(value, full))
        elif isinstance(value, str):
            out[full] = value
    return out


def language_label(code: str) -> str:
    for item, label in LANGUAGES:
        if item == code:
            return label
    return code


def current_language() -> str:
    return _language


def init_language(code: str | None = None) -> str:
    """启动时加载语言。未知代码回退简体中文。"""
    return set_language(code or DEFAULT_LANGUAGE)


def set_language(code: str) -> str:
    global _language, _ui, _ui_fallback, _terms, _terms_fallback
    valid = {item for item, _label in LANGUAGES}
    _language = code if code in valid else DEFAULT_LANGUAGE
    root = _bundle_dir()
    _ui_fallback = _flatten_ui(_load_json(root / "locales" / f"{DEFAULT_LANGUAGE}.json"))
    _ui = (
        _ui_fallback
        if _language == DEFAULT_LANGUAGE
        else _flatten_ui(_load_json(root / "locales" / f"{_language}.json"))
    )
    _terms_fallback = _load_json(root / "terms" / f"{DEFAULT_LANGUAGE}.json")
    _terms = (
        _terms_fallback
        if _language == DEFAULT_LANGUAGE
        else _load_json(root / "terms" / f"{_language}.json")
    )
    return _language


def t(key: str, default: str | None = None, **kwargs: Any) -> str:
    text = _ui.get(key)
    if text is None:
        text = _ui_fallback.get(key)
    if text is None:
        text = default if default is not None else key
    if kwargs:
        try:
            text = text.format(**kwargs)
        except (KeyError, IndexError, ValueError):
            pass
    return text


def term(category: str, item_id: str, default: str | None = None) -> str:
    """按清单类别 + 内部 id 查游戏名词。"""
    if not item_id:
        return default or ""
    cat = _terms.get(category) or {}
    if item_id in cat and cat[item_id]:
        return cat[item_id]
    fallback = _terms_fallback.get(category) or {}
    if item_id in fallback and fallback[item_id]:
        return fallback[item_id]
    return default if default is not None else item_id


def help_html() -> str:
    lang = _language
    if lang in _help_cache:
        return _help_cache[lang]
    root = _bundle_dir()
    for candidate in (lang, DEFAULT_LANGUAGE):
        path = root / "help" / f"{candidate}.html"
        if path.is_file():
            text = path.read_text(encoding="utf-8")
            _help_cache[lang] = text
            return text
    _help_cache[lang] = ""
    return ""


def install_qt_translator(app) -> None:
    """加载 Qt 内置控件的对应语言包（确定/取消等）。"""
    global _qt_translator
    try:
        from PySide6.QtCore import QLibraryInfo, QTranslator
    except ImportError:
        return
    if _qt_translator is not None:
        app.removeTranslator(_qt_translator)
        _qt_translator = None
    qm = _QT_QM.get(_language)
    if not qm:
        return
    translator = QTranslator(app)
    trans_dir = QLibraryInfo.path(QLibraryInfo.LibraryPath.TranslationsPath)
    if translator.load(qm, trans_dir):
        app.installTranslator(translator)
        _qt_translator = translator


# 导入即准备简体，避免测试在未 init 时拿到空表
set_language(DEFAULT_LANGUAGE)
