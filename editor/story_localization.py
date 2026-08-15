# -*- coding: utf-8 -*-
"""Story content localization editor.

The source fields in a story are the default locale.  Other locales are kept
in a stable-path catalog so old, single-language stories remain unchanged.
"""

from __future__ import annotations

import copy

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QComboBox, QDialog, QDialogButtonBox, QFormLayout, QHBoxLayout,
    QHeaderView, QLabel, QPushButton, QTableWidget, QTableWidgetItem,
    QVBoxLayout,
)

from i18n import t
from lomc.localization import SUPPORTED_LOCALES, iter_localizable_texts


LOCALE_NAMES = {
    "zh_CN": "简体中文",
    "zh_TW": "繁體中文",
    "ja": "日本語",
    "ko": "한국어",
}


def normalized_localization(story: dict) -> dict:
    """Return a safe editable copy without mutating an old story."""
    raw = story.get("localization")
    if not isinstance(raw, dict):
        return {"default_locale": "zh_CN", "fallback_locale": "zh_CN", "translations": {}}
    default = raw.get("default_locale")
    if default not in SUPPORTED_LOCALES:
        default = "zh_CN"
    fallback = raw.get("fallback_locale")
    if fallback not in SUPPORTED_LOCALES:
        fallback = default
    known = dict(iter_localizable_texts(story))
    translations = {}
    for locale, catalog in (raw.get("translations") or {}).items():
        if locale not in SUPPORTED_LOCALES or locale == default or not isinstance(catalog, dict):
            continue
        cleaned = {path: value for path, value in catalog.items()
                   if path in known and isinstance(value, str) and value.strip()}
        if cleaned:
            translations[locale] = cleaned
    return {"default_locale": default, "fallback_locale": fallback, "translations": translations}


def apply_localization_settings(story: dict, config: dict | None) -> None:
    """Apply dialog output. ``None`` opts the story back into legacy mode."""
    if config is None:
        story.pop("localization", None)
        return
    clean = normalized_localization({**story, "localization": copy.deepcopy(config)})
    story["localization"] = clean


class StoryLocalizationDialog(QDialog):
    """Edit translations for one story; the caller records one undo step."""

    def __init__(self, story: dict, parent=None):
        super().__init__(parent)
        self.setWindowTitle(t("localization.title"))
        self.resize(900, 580)
        self._story = story
        self._config = normalized_localization(story)
        self._removed = False
        self._active_locale = ""

        root = QVBoxLayout(self)
        hint = QLabel(t("localization.hint")); hint.setWordWrap(True); root.addWidget(hint)
        form = QFormLayout()
        self.default_combo = QComboBox()
        self.fallback_combo = QComboBox()
        self.locale_combo = QComboBox()
        for locale in SUPPORTED_LOCALES:
            label = f"{LOCALE_NAMES[locale]} ({locale})"
            self.default_combo.addItem(label, locale)
            self.fallback_combo.addItem(label, locale)
            self.locale_combo.addItem(label, locale)
        form.addRow(t("localization.default"), self.default_combo)
        form.addRow(t("localization.fallback"), self.fallback_combo)
        form.addRow(t("localization.edit_locale"), self.locale_combo)
        root.addLayout(form)

        self.table = QTableWidget(0, 3)
        self.table.setHorizontalHeaderLabels([
            t("localization.path"), t("localization.source"), t("localization.translation")
        ])
        self.table.verticalHeader().setVisible(False)
        self.table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        self.table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        self.table.horizontalHeader().setSectionResizeMode(2, QHeaderView.ResizeMode.Stretch)
        root.addWidget(self.table, 1)

        actions = QHBoxLayout()
        remove = QPushButton(t("localization.remove")); remove.clicked.connect(self._remove)
        actions.addWidget(remove); actions.addStretch(1)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Save | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self._save); buttons.rejected.connect(self.reject)
        actions.addWidget(buttons); root.addLayout(actions)

        default = self._config["default_locale"]
        fallback = self._config["fallback_locale"]
        self.default_combo.setCurrentIndex(self.default_combo.findData(default))
        self.fallback_combo.setCurrentIndex(self.fallback_combo.findData(fallback))
        target = next((locale for locale in SUPPORTED_LOCALES if locale != default), default)
        self.locale_combo.setCurrentIndex(self.locale_combo.findData(target))
        self._active_locale = target
        self._load_table(target)
        self.locale_combo.currentIndexChanged.connect(self._locale_changed)
        self.default_combo.currentIndexChanged.connect(self._default_changed)

    def _capture_table(self) -> None:
        if not self._active_locale:
            return
        catalog = {}
        for row in range(self.table.rowCount()):
            path = self.table.item(row, 0).text()
            value = self.table.item(row, 2).text().strip()
            if value:
                catalog[path] = value
        translations = self._config.setdefault("translations", {})
        if catalog:
            translations[self._active_locale] = catalog
        else:
            translations.pop(self._active_locale, None)

    def _load_table(self, locale: str) -> None:
        catalog = self._config.get("translations", {}).get(locale, {})
        rows = list(iter_localizable_texts(self._story))
        self.table.setRowCount(len(rows))
        for row, (path, source) in enumerate(rows):
            path_item = QTableWidgetItem(path)
            source_item = QTableWidgetItem(source)
            path_item.setFlags(path_item.flags() & ~Qt.ItemFlag.ItemIsEditable)
            source_item.setFlags(source_item.flags() & ~Qt.ItemFlag.ItemIsEditable)
            self.table.setItem(row, 0, path_item)
            self.table.setItem(row, 1, source_item)
            self.table.setItem(row, 2, QTableWidgetItem(catalog.get(path, "")))

    def _locale_changed(self) -> None:
        self._capture_table()
        self._active_locale = str(self.locale_combo.currentData() or "")
        self._load_table(self._active_locale)

    def _default_changed(self) -> None:
        default = str(self.default_combo.currentData())
        self._capture_table()
        self._config["translations"].pop(default, None)
        if self._active_locale == default:
            target = next(locale for locale in SUPPORTED_LOCALES if locale != default)
            self._active_locale = ""
            self.locale_combo.setCurrentIndex(self.locale_combo.findData(target))

    def _save(self) -> None:
        self._capture_table()
        default = str(self.default_combo.currentData())
        fallback = str(self.fallback_combo.currentData())
        self._config["default_locale"] = default
        self._config["fallback_locale"] = fallback
        self._config["translations"].pop(default, None)
        self.accept()

    def _remove(self) -> None:
        self._removed = True
        self.accept()

    def result_config(self) -> dict | None:
        return None if self._removed else copy.deepcopy(self._config)
