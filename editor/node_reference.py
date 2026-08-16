# -*- coding: utf-8 -*-
"""由实际节点 schema 生成的离线节点/API 参考界面。"""

from __future__ import annotations

import html
import json
import re

from PySide6.QtCore import Qt, QUrl
from PySide6.QtGui import QKeySequence, QShortcut
from PySide6.QtWidgets import (
    QDialog,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QSplitter,
    QStyle,
    QTabWidget,
    QTextBrowser,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
    QWidget,
)

from i18n import t
from help_content import current_help_html
import models


# 对作者公开的是稳定 Story JSON 契约；这里同时标出编译后调用的原版/Host 接口，
# 便于高级作者确认哪些能力来自原版，哪些由 MortalModHost 提供。
RUNTIME_API: dict[str, str] = {
    "music": "AudioManager.PlayMusic / StopMusic / FadeoutMusic",
    "sound": "AudioManager.PlaySound / PlayEnvironmentSound / FadeoutSound",
    "scene": "MainUI.ChangeScene",
    "background": "MortalModHost.mod_background_*",
    "custom_cg": "MortalModHost.mod_custom_cg_*",
    "overlay": "MortalModHost.mod_overlay_*",
    "show": "CharacterPlaceholder.LoadCharacterAsset + Stage.show",
    "move": "Stage.show (fromPosition / toPosition)",
    "face": "Stage.show (facing)",
    "hide": "Stage.hide",
    "focus": "CharacterPlaceholder.Focus",
    "offset": "CharacterPlaceholder.MoveOffsetCoroutine",
    "say": "SayDialog + Stage.showPortrait + say",
    "choice": "MenuDialog.Options + choose",
    "shock": "common Flowchart / shock",
    "mask": "OS_Mask.Show",
    "intro": "CharacterIntroPanel.Show / MortalModHost.mod_prepare_character_intro",
    "effect": "EffectManager.CreateEffect",
    "transition": "TransitionPanel",
    "camera": "Original camera filter / stage camera",
    "block": "Fungus Flowchart.RunBlock",
    "cg": "MainUI Show/Hide Picture / Item / Map / FamilyTree",
    "dim": "Stage.SetDimmed",
    "message": "MainUI.DisplayMessageText",
    "rotate": "CharacterPlaceholder.Rotate",
    "dayenv": "game day-environment API",
    "stat": "StatModifyManager.Player",
    "stat_set": "MortalModHost.mod_stat_set",
    "affinity": "AffinityManager",
    "talent": "TalentManager",
    "item": "ItemDatabase",
    "flag": "modflags",
    "game_flag": "LuaManager FlagData",
    "enemy": "EnemyManager setup",
    "battle_skill": "game battle-skill slots",
    "combat": "SceneController → Combat + verified Host result",
    "battle": "SceneController → Battle + verified Host result",
    "battle_result": "MortalModHost.mod_last_battle_result",
    "reward": "stat / affinity / talent / item / flag composition",
    "result_screen": "MainUI.DisplayMessageText + reward",
    "custom_shop": "ShopPanel + Host temporary inventory",
    "stat_check": "LuaManager.GetStatData",
    "affinity_check": "MortalModHost.mod_affinity_value",
    "item_check": "MortalModHost.mod_has_item + ItemDatabase",
    "talent_check": "MortalModHost.mod_talent_level",
    "flag_check": "modflags / CheckPointManager.Condition / LuaManager.GetFlagData",
    "activity": "checks + time + reward composition",
    "mod_quest": "isolated MOD quest state",
    "quest_check": "isolated MOD quest state query",
    "persistent_var": "isolated MOD save variables",
    "persistent_check": "isolated MOD save-variable query",
    "mission": "MissionManager",
    "time": "LuaManager.NextRound / NextMonth",
    "autosave": "SaveSystem + isolated MOD slot",
    "branch": "modflags / CheckPointManager / StatData",
    "dice": "MortalModHost custom result bridge + original DiceMenuDialog",
    "goto_scene": "SceneController (Free / Combat / Battle / GameOver / End)",
    "panel": "MainUI system panels",
    "wait": "Fungus wait",
    "end": "Free scene or same-package next_script",
    "death": "GameOverPanel + MOD 9xxxxx ID",
    "raw": "MoonSharp/Fungus raw Lua",
}


# 字段 kind 到可本地化说明类别的映射。保留实际 kind 名，便于作者按契约搜索。
KIND_DOCS: dict[str, str] = {
    "line": "reference.kind.line",
    "multiline": "reference.kind.multiline",
    "code": "reference.kind.code",
    "int": "reference.kind.int",
    "float": "reference.kind.float",
    "bool": "reference.kind.bool",
    "character": "reference.kind.game_or_user_id",
    "affinity_character": "reference.kind.game_id",
    "portrait": "reference.kind.game_id",
    "position": "reference.kind.game_id",
    "view": "reference.kind.game_id",
    "music": "reference.kind.game_or_user_id",
    "sound_name": "reference.kind.game_or_user_id",
    "voice": "reference.kind.user_id",
    "stat": "reference.kind.game_id",
    "talent": "reference.kind.game_id",
    "item": "reference.kind.game_id",
    "game_flag": "reference.kind.game_id",
    "mode": "reference.kind.enum_like",
    "facing": "reference.kind.enum_like",
    "branch_source": "reference.kind.enum_like",
    "node_ref": "reference.kind.node_ref",
    "story_ref": "reference.kind.story_ref",
    "options": "reference.kind.target_table",
    "cases": "reference.kind.target_table",
    "dice_bands": "reference.kind.target_table",
    "vars": "reference.kind.structured",
    "effect": "reference.kind.game_id",
    "camera": "reference.kind.game_id",
    "menu_dialog": "reference.kind.game_id",
    "user_image": "reference.kind.user_id",
    "intro_image": "reference.kind.user_id",
    "ending_image": "reference.kind.game_or_user_id",
    "battle_faction": "reference.kind.game_id",
    "enemy_team": "reference.kind.game_id",
    "enemy_team_optional": "reference.kind.game_id",
    "battle_skill": "reference.kind.game_id",
    "goto_scene_key": "reference.kind.game_id",
    "death_id": "reference.kind.mod_id",
    "official_characters": "reference.kind.structured",
    "combat_talents": "reference.kind.structured",
    "reward_entries": "reference.kind.nonempty_table",
    "reward_entries_optional": "reference.kind.structured",
    "custom_shop_items": "reference.kind.nonempty_table",
    "discount_toggle": "reference.kind.number_or_bool",
    "bool_int": "reference.kind.number_or_bool",
    "percent_scale": "reference.kind.percent",
    "percent_cg_scale": "reference.kind.percent",
    "percent_position": "reference.kind.percent",
    "percent_offset": "reference.kind.percent",
    "percent_opacity": "reference.kind.percent",
}


def _group_for(node_type: str) -> str:
    for group, node_types in models.NODE_GROUPS:
        if node_type in node_types:
            return group
    return t("reference.group.other", default="Other")


def _kind_doc(kind: str) -> str:
    if kind.startswith("enum:"):
        enum_name = kind.split(":", 1)[1]
        values = models.ENUM_SETS.get(enum_name, [])
        rendered = " · ".join(f"{value} = {label}" for value, label in values)
        base = t("reference.kind.enum", default="Fixed enum")
        return base + (": " + rendered if rendered else "")
    key = KIND_DOCS.get(kind, "reference.kind.editor")
    description = t(key, default="Editor contract field")
    return f"{description} [{kind}]"


_FLOW_TARGET_FIELDS = {
    "goto", "win", "lose", "success", "failure", "next_script", "next",
}
_COMBAT_VITAL_FIELDS = {"max_health", "health", "max_stamina", "stamina"}
_COMBAT_STAT_FIELDS = {
    "strength", "internal", "dexterity", "talking", "defence", "sword",
    "fist", "martial_weapon", "mental",
}
_COMBAT_RATE_FIELDS = {
    "talk_rate", "attack_rate", "weapon_rate", "ultimate_rate", "block_rate",
}
_BATTLE_PEOPLE_FIELDS = {"friend_people", "enemy_people"}
_BATTLE_FACTION_FIELDS = {"friend_faction", "enemy_faction"}
_BATTLE_CHARACTER_FIELDS = {"friend_characters", "enemy_characters"}


def _field_label(node_type: str, key: str, fallback: str) -> str:
    return t(
        f"field.{node_type}.{key}",
        default=t(f"field.{key}", default=fallback),
    )


def _field_effect(node_type: str, key: str, field_label: str, optional: bool) -> str:
    """Return an author-facing explanation of what the parameter changes."""
    if key == "id":
        return t("reference.effect.id")
    if key == "type":
        return t("reference.effect.type")
    if key in _FLOW_TARGET_FIELDS:
        return t("reference.effect.flow_target", field=field_label)
    if node_type == "combat":
        if key == "character":
            return t("reference.effect.combat_character")
        if key in _COMBAT_VITAL_FIELDS:
            return t("reference.effect.combat_vital", field=field_label)
        if key in _COMBAT_STAT_FIELDS:
            return t("reference.effect.combat_stat", field=field_label)
        if key == "talents":
            return t("reference.effect.combat_talents")
        if key.startswith("ultimate_"):
            return t("reference.effect.combat_ultimate", field=field_label)
        if key in _COMBAT_RATE_FIELDS:
            return t("reference.effect.combat_rate", field=field_label)
    if node_type == "battle":
        if key in _BATTLE_FACTION_FIELDS:
            return t("reference.effect.battle_faction", field=field_label)
        if key in _BATTLE_PEOPLE_FIELDS:
            return t("reference.effect.battle_people", field=field_label)
        if key in _BATTLE_CHARACTER_FIELDS:
            return t("reference.effect.battle_characters", field=field_label)
    if node_type == "dice":
        special = {
            "max": "reference.effect.dice_max",
            "header": "reference.effect.dice_header",
            "bonus": "reference.effect.dice_bonus",
            "bonus_name": "reference.effect.dice_bonus_name",
            "bonus_status": "reference.effect.dice_bonus_status",
            "bands": "reference.effect.dice_bands",
        }
        return t(special[key])
    return t(
        "reference.effect.optional" if optional else "reference.effect.required",
        field=field_label,
    )


def node_reference_html(node_type: str) -> str:
    """生成单个节点的完整离线参考；字段直接来自实际 schema。"""
    if node_type not in models.NODE_SCHEMAS:
        return f"<p>{html.escape(t('reference.unknown', default='Unknown node.'))}</p>"
    schema = models.NODE_SCHEMAS[node_type]
    label = models.NODE_TYPE_CN.get(node_type, schema.get("label", node_type))
    help_text = models.NODE_HELP.get(node_type, "")
    example = models.new_node(node_type, f"{node_type}1", models.FALLBACK_EDITOR_DATA)

    rows = [
        ("id", t("reference.node_id", default="Node ID"), t("reference.node_id_contract", default="Unique string within the chapter"), False, example.get("id")),
        ("type", t("reference.node_type", default="Node type"), t("reference.fixed_type", default="Fixed value: {value}", value=node_type), False, node_type),
    ]
    rows.extend(
        (key, _field_label(node_type, key, field_label), _kind_doc(kind), optional, example.get(key))
        for key, field_label, kind, optional in schema["fields"]
    )
    if node_type not in {
        "choice", "branch", "dice", "end", "death", "goto_scene", "combat",
        "battle", "battle_result",
    }:
        rows.append(("goto", t("field.goto", default="Advanced jump"), t("reference.goto_contract", default="Optional node reference; overrides sequential flow"), True, None))

    table_rows = []
    for key, field_label, contract, optional, default in rows:
        default_text = "—" if default is None else json.dumps(default, ensure_ascii=False)
        effect = _field_effect(node_type, key, str(field_label), optional)
        table_rows.append(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td></tr>".format(
                html.escape(str(key)), html.escape(str(field_label)),
                html.escape(str(effect)),
                t("reference.optional", default="Optional") if optional else t("reference.required", default="Required"), html.escape(str(contract)),
                html.escape(default_text),
            )
        )

    api = RUNTIME_API.get(
        node_type, f"lomc.codegen._emit_{node_type} → MortalModHost / game API"
    )
    sample = html.escape(json.dumps(example, ensure_ascii=False, indent=2))
    return f"""
    <style>
      body {{ color: #e8e8ed; font-family: sans-serif; font-size: 13px; line-height: 1.45; margin: 24px 30px 40px 30px; }}
      h1 {{ font-size: 25px; margin-bottom: 2px; }}
      h2 {{ font-size: 17px; margin-top: 24px; border-bottom: 1px solid #44444c; padding-bottom: 6px; }}
      .muted {{ color: #a8a8b2; }}
      .api {{ border-left: 4px solid #0a84ff; padding: 10px 13px; background: #24242a; }}
      table {{ border-collapse: collapse; width: 100%; }}
      th, td {{ border-bottom: 1px solid #44444c; padding: 7px; text-align: left; vertical-align: top; }}
      th {{ color: #b7d7ff; }}
      code, pre {{ font-family: Consolas, monospace; }}
      code {{ color: #ffd6a0; }}
      pre {{ background: #1d1d22; border: 1px solid #393943; padding: 12px; white-space: pre-wrap; }}
    </style>
    <h1>{html.escape(str(label))}</h1>
    <p class="muted">{html.escape(t('reference.story_type', default='Story type'))}: <code>{html.escape(node_type)}</code> · {html.escape(t('reference.category', default='Category'))}: {html.escape(_group_for(node_type))}</p>
    <p>{html.escape(help_text or str(schema.get('label', label)))}</p>
    <h2>{html.escape(t('reference.runtime_api', default='Runtime API'))}</h2>
    <div class="api"><code>{html.escape(api)}</code><br>{html.escape(t('reference.runtime_note', default='The editor saves Story JSON and lomc generates Lua. Authors do not call this API directly.'))}</div>
    <h2>{html.escape(t('reference.field_api', default='Field contract'))}</h2>
    <table><tr><th>{html.escape(t('reference.col.key', default='JSON key'))}</th><th>{html.escape(t('reference.col.meaning', default='UI meaning'))}</th><th>{html.escape(t('reference.col.effect', default='What it does'))}</th><th>{html.escape(t('reference.col.requirement', default='Requirement'))}</th><th>{html.escape(t('reference.col.type', default='Type / values'))}</th><th>{html.escape(t('reference.col.default', default='New default'))}</th></tr>{''.join(table_rows)}</table>
    <h2>{html.escape(t('reference.example', default='Minimal example'))}</h2>
    <pre>{sample}</pre>
    <h2>{html.escape(t('reference.compatibility', default='Compatibility and safety'))}</h2>
    <p>{html.escape(t('reference.compatibility_note', default='Use only listed fields; the compiler rejects unknown fields. Game IDs come from the current extracted data and must be regenerated and retested after a game update.'))}</p>
    """


def reference_node_types() -> tuple[str, ...]:
    return tuple(models.NODE_TYPES)


def documentation_home_html() -> str:
    """生成类似 Unity Manual 首页的离线文档入口。"""
    group_rows = []
    for group_name, node_types in models.NODE_GROUPS:
        if not node_types:
            continue
        first = node_types[0]
        group_rows.append(
            '<tr><td><a href="lomdoc://node/{node}">{group}</a></td>'
            '<td>{count}</td><td>{examples}</td></tr>'.format(
                node=html.escape(first, quote=True),
                group=html.escape(group_name),
                count=len(node_types),
                examples=html.escape(" · ".join(node_types[:4])),
            )
        )
    return f"""
    <style>
      body {{ color: #e8e8ed; font-family: sans-serif; font-size: 13px; line-height: 1.5; margin: 24px 30px 40px 30px; }}
      h1 {{ font-size: 27px; margin: 0 0 4px 0; }}
      h2 {{ font-size: 18px; margin-top: 26px; border-bottom: 1px solid #44444c; padding-bottom: 6px; }}
      .eyebrow {{ color: #78b7ff; font-size: 11px; font-weight: 600; letter-spacing: 1px; }}
      .lead {{ color: #c6c6cf; font-size: 15px; max-width: 760px; }}
      .note {{ border-left: 4px solid #0a84ff; padding: 10px 13px; background: #24242a; }}
      table {{ border-collapse: collapse; width: 100%; }}
      th, td {{ border-bottom: 1px solid #44444c; padding: 9px 7px; text-align: left; vertical-align: top; }}
      th {{ color: #b7d7ff; }}
      a {{ color: #66adff; text-decoration: none; }}
      code {{ color: #ffd6a0; font-family: Consolas, monospace; }}
    </style>
    <div class="eyebrow">LOM MODKIT DOCUMENTATION</div>
    <h1>{html.escape(t('docs.home_title', default='Author documentation'))}</h1>
    <p class="lead">{html.escape(t('docs.home_intro', default='Offline reference generated from the editor schema. Search every node, field, and runtime bridge from one place.'))}</p>
    <div class="note"><b>{html.escape(t('docs.contract_title', default='Authoring contract'))}</b><br>
    {html.escape(t('docs.contract_note', default='The editor writes Story JSON, lomc validates and compiles it, and MortalModHost bridges verified functions to the original game.'))}</div>
    <h2>{html.escape(t('docs.catalog_title', default='Node catalog'))}</h2>
    <table><tr><th>{html.escape(t('docs.col.category', default='Category'))}</th><th>{html.escape(t('docs.col.nodes', default='Nodes'))}</th><th>{html.escape(t('docs.col.examples', default='Examples'))}</th></tr>{''.join(group_rows)}</table>
    <h2>{html.escape(t('docs.how_to_title', default='How to use this reference'))}</h2>
    <p>{html.escape(t('docs.how_to_note', default='Choose a category in the sidebar or search by visible field name, JSON key, node type, or runtime API. Each page shows requirements, defaults, a minimal example, and compatibility notes.'))}</p>
    """


class NodeReferenceWidget(QWidget):
    """Unity Manual 风格的离线节点/API 文档浏览器。"""

    HOME_PAGE = "__home__"

    def __init__(self, parent=None):
        super().__init__(parent)
        self._history = [self.HOME_PAGE]
        self._history_index = 0
        self._current_page = self.HOME_PAGE

        layout = QVBoxLayout(self)
        layout.setContentsMargins(12, 12, 12, 12)
        layout.setSpacing(9)

        toolbar = QHBoxLayout()
        toolbar.setSpacing(6)
        self.back_button = QPushButton()
        self.forward_button = QPushButton()
        self.home_button = QPushButton()
        self.back_button.setIcon(self.style().standardIcon(QStyle.StandardPixmap.SP_ArrowBack))
        self.forward_button.setIcon(self.style().standardIcon(QStyle.StandardPixmap.SP_ArrowForward))
        self.home_button.setIcon(self.style().standardIcon(QStyle.StandardPixmap.SP_DirHomeIcon))
        for button in (self.back_button, self.forward_button, self.home_button):
            button.setFixedSize(34, 30)
        self.back_button.setToolTip(t("docs.back", default="Back"))
        self.forward_button.setToolTip(t("docs.forward", default="Forward"))
        self.home_button.setToolTip(t("docs.home", default="Documentation home"))
        self.back_button.setAccessibleName(t("docs.back", default="Back"))
        self.forward_button.setAccessibleName(t("docs.forward", default="Forward"))
        self.home_button.setAccessibleName(t("docs.home", default="Documentation home"))
        toolbar.addWidget(self.back_button)
        toolbar.addWidget(self.forward_button)
        toolbar.addWidget(self.home_button)

        self.breadcrumb = QLabel()
        self.breadcrumb.setObjectName("documentationBreadcrumb")
        self.breadcrumb.setMinimumWidth(180)
        toolbar.addWidget(self.breadcrumb, 1)

        self.search = QLineEdit()
        self.search.setPlaceholderText(t("reference.search", default="Search nodes, fields, or APIs…"))
        self.search.setClearButtonEnabled(True)
        self.search.setAccessibleName(t("docs.search_name", default="Search documentation"))
        self.search.setMinimumWidth(280)
        toolbar.addWidget(self.search)
        layout.addLayout(toolbar)

        splitter = QSplitter(Qt.Orientation.Horizontal)
        splitter.setChildrenCollapsible(False)
        self.node_tree = QTreeWidget()
        self.node_tree.setObjectName("documentationTree")
        self.node_tree.setHeaderHidden(True)
        self.node_tree.setMinimumWidth(240)
        self.node_tree.setAccessibleName(t("docs.contents", default="Contents"))
        self.browser = QTextBrowser()
        self.browser.setObjectName("documentationBrowser")
        self.browser.setOpenExternalLinks(False)
        self.browser.setOpenLinks(False)
        self.browser.setAccessibleName(t("docs.article", default="Documentation article"))
        splitter.addWidget(self.node_tree)
        splitter.addWidget(self.browser)
        splitter.setSizes([270, 830])
        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)
        layout.addWidget(splitter, 1)

        self.search.textChanged.connect(self._filter)
        self.search.returnPressed.connect(self._activate_first_result)
        self.node_tree.currentItemChanged.connect(self._show_current)
        self.browser.anchorClicked.connect(self._open_link)
        self.back_button.clicked.connect(self.go_back)
        self.forward_button.clicked.connect(self.go_forward)
        self.home_button.clicked.connect(self.go_home)
        QShortcut(QKeySequence.StandardKey.Find, self, activated=self.search.setFocus)
        QShortcut(QKeySequence("Alt+Left"), self, activated=self.go_back)
        QShortcut(QKeySequence("Alt+Right"), self, activated=self.go_forward)
        self._filter("")
        self._update_navigation()

    def _filter(self, text: str) -> None:
        query = (text or "").strip().casefold()
        self.node_tree.blockSignals(True)
        self.node_tree.clear()
        page_items: dict[str, QTreeWidgetItem] = {}
        if not query:
            home = QTreeWidgetItem([t("docs.overview", default="Overview")])
            home.setData(0, Qt.ItemDataRole.UserRole, self.HOME_PAGE)
            self.node_tree.addTopLevelItem(home)
            page_items[self.HOME_PAGE] = home

        shown_count = 0
        for group_name, node_types in models.NODE_GROUPS:
            matches = [node_type for node_type in node_types if self._matches(node_type, query)]
            if not matches:
                continue
            group_item = QTreeWidgetItem([group_name])
            group_item.setFlags(group_item.flags() & ~Qt.ItemFlag.ItemIsSelectable)
            self.node_tree.addTopLevelItem(group_item)
            for node_type in matches:
                schema = models.NODE_SCHEMAS[node_type]
                label = models.NODE_TYPE_CN.get(node_type, schema.get("label", node_type))
                item = QTreeWidgetItem([str(label)])
                item.setData(0, Qt.ItemDataRole.UserRole, node_type)
                item.setToolTip(0, f"{label} ({node_type})")
                group_item.addChild(item)
                page_items[node_type] = item
                shown_count += 1
            group_item.setExpanded(True)

        target = page_items.get(self._current_page)
        if target is None:
            target = page_items.get(self.HOME_PAGE)
        if target is None and page_items:
            target = next(iter(page_items.values()))
        if target is not None:
            self.node_tree.setCurrentItem(target)
        self.node_tree.blockSignals(False)
        if target is not None:
            page = str(target.data(0, Qt.ItemDataRole.UserRole) or "")
            if page:
                self._render_page(page)
        if shown_count == 0:
            self.browser.setHtml(
                f"<p>{html.escape(t('reference.no_matches', default='No matching nodes or fields.'))}</p>"
            )
            self.breadcrumb.setText(t("docs.no_results", default="No results"))

    def _matches(self, node_type: str, query: str) -> bool:
        if not query:
            return True
        schema = models.NODE_SCHEMAS[node_type]
        label = models.NODE_TYPE_CN.get(node_type, schema.get("label", node_type))
        field_terms = []
        for key, field_label, kind, _optional in schema["fields"]:
            field_terms.extend((key, t(f"field.{key}", default=field_label), _kind_doc(kind)))
        haystack = " ".join((
            node_type,
            str(label),
            " ".join(field_terms),
            models.NODE_HELP.get(node_type, ""),
            RUNTIME_API.get(node_type, ""),
        )).casefold()
        return query in haystack

    def _show_current(self, current: QTreeWidgetItem | None, _previous=None) -> None:
        if current is None:
            return
        page = str(current.data(0, Qt.ItemDataRole.UserRole) or "")
        if page:
            self._navigate(page)

    def _render_page(self, page: str) -> None:
        self._current_page = page
        if page == self.HOME_PAGE:
            self.browser.setHtml(documentation_home_html())
            self.breadcrumb.setText(
                f"{t('docs.title', default='Documentation')}  /  {t('docs.overview', default='Overview')}"
            )
        else:
            self.browser.setHtml(node_reference_html(page))
            label = models.NODE_TYPE_CN.get(page, page)
            self.breadcrumb.setText(
                f"{t('docs.title', default='Documentation')}  /  {_group_for(page)}  /  {label}"
            )
        self.browser.verticalScrollBar().setValue(0)

    def _navigate(self, page: str, *, record: bool = True) -> None:
        if page != self.HOME_PAGE and page not in models.NODE_SCHEMAS:
            return
        if record and page != self._current_page:
            del self._history[self._history_index + 1:]
            self._history.append(page)
            self._history_index = len(self._history) - 1
        self._render_page(page)
        item = self._find_item(page)
        if item is not None and self.node_tree.currentItem() is not item:
            self.node_tree.blockSignals(True)
            self.node_tree.setCurrentItem(item)
            self.node_tree.blockSignals(False)
        self._update_navigation()

    def _find_item(self, page: str) -> QTreeWidgetItem | None:
        iterator = self.node_tree.invisibleRootItem()
        stack = [iterator.child(i) for i in range(iterator.childCount())]
        while stack:
            item = stack.pop(0)
            if str(item.data(0, Qt.ItemDataRole.UserRole) or "") == page:
                return item
            stack[0:0] = [item.child(i) for i in range(item.childCount())]
        return None

    def _open_link(self, url: QUrl) -> None:
        if url.scheme() == "lomdoc" and url.host() == "node":
            self._navigate(url.path().lstrip("/"))

    def _activate_first_result(self) -> None:
        item = self.node_tree.currentItem()
        if item is not None:
            page = str(item.data(0, Qt.ItemDataRole.UserRole) or "")
            if page:
                self._navigate(page)
                self.browser.setFocus()

    def go_back(self) -> None:
        if self._history_index <= 0:
            return
        self._history_index -= 1
        page = self._history[self._history_index]
        if page == self.HOME_PAGE and self.search.text():
            self.search.clear()
        self._navigate(page, record=False)

    def go_forward(self) -> None:
        if self._history_index >= len(self._history) - 1:
            return
        self._history_index += 1
        self._navigate(self._history[self._history_index], record=False)

    def go_home(self) -> None:
        if self.search.text():
            self.search.clear()
        self._navigate(self.HOME_PAGE)

    def _update_navigation(self) -> None:
        self.back_button.setEnabled(self._history_index > 0)
        self.forward_button.setEnabled(self._history_index < len(self._history) - 1)

    def current_node_type(self) -> str:
        return "" if self._current_page == self.HOME_PAGE else self._current_page

    def visible_node_types(self) -> tuple[str, ...]:
        """测试与可访问性工具使用的当前搜索结果。"""
        out = []
        root = self.node_tree.invisibleRootItem()
        for i in range(root.childCount()):
            top = root.child(i)
            for j in range(top.childCount()):
                value = str(top.child(j).data(0, Qt.ItemDataRole.UserRole) or "")
                if value:
                    out.append(value)
        return tuple(out)


_SOFTWARE_STYLE = """
<style>
  body { color: #e8e8ed; font-family: sans-serif; font-size: 13px; line-height: 1.55; margin: 24px 30px 40px 30px; }
  h1 { font-size: 26px; margin: 0 0 10px 0; }
  h2 { font-size: 19px; margin: 0 0 12px 0; border-bottom: 1px solid #44444c; padding-bottom: 7px; }
  h3 { font-size: 15px; margin-top: 22px; }
  p, li { max-width: 880px; }
  a { color: #66adff; text-decoration: none; }
  code { color: #ffd6a0; font-family: Consolas, monospace; }
  .eyebrow { color: #78b7ff; font-size: 11px; font-weight: 600; letter-spacing: 1px; }
</style>
"""


def software_documentation_pages() -> tuple[tuple[str, str, str], ...]:
    """把当前语言的使用指南按 h2 拆成 Unity Manual 风格的独立文章。"""
    source = current_help_html()
    matches = list(re.finditer(r"<h2[^>]*>(.*?)</h2>", source, re.I | re.S))
    pages: list[tuple[str, str, str]] = []
    intro_end = matches[0].start() if matches else len(source)
    intro = source[:intro_end].strip()
    pages.append(("overview", t("docs.software_overview", default="Software overview"), intro))
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(source)
        raw_title = re.sub(r"<[^>]+>", "", match.group(1))
        title = html.unescape(raw_title).strip()
        body = source[match.end():end].strip()
        pages.append((f"section-{index + 1}", title, f"<h2>{html.escape(title)}</h2>{body}"))
    return tuple(pages)


class SoftwareDocumentationWidget(QWidget):
    """软件操作文档：与节点契约分栏，保留目录、搜索和独立文章阅读。"""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.pages = software_documentation_pages()
        layout = QVBoxLayout(self)
        layout.setContentsMargins(12, 12, 12, 12)
        layout.setSpacing(9)

        self.search = QLineEdit()
        self.search.setPlaceholderText(t("docs.software_search", default="Search software usage…"))
        self.search.setClearButtonEnabled(True)
        self.search.setAccessibleName(t("docs.software_search", default="Search software usage"))
        layout.addWidget(self.search)

        splitter = QSplitter(Qt.Orientation.Horizontal)
        splitter.setChildrenCollapsible(False)
        self.contents = QTreeWidget()
        self.contents.setHeaderHidden(True)
        self.contents.setMinimumWidth(240)
        self.contents.setAccessibleName(t("docs.contents", default="Contents"))
        self.browser = QTextBrowser()
        self.browser.setOpenExternalLinks(True)
        self.browser.setAccessibleName(t("docs.article", default="Documentation article"))
        splitter.addWidget(self.contents)
        splitter.addWidget(self.browser)
        splitter.setSizes([270, 830])
        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)
        layout.addWidget(splitter, 1)

        self.search.textChanged.connect(self._filter)
        self.contents.currentItemChanged.connect(self._show_current)
        QShortcut(QKeySequence.StandardKey.Find, self, activated=self.search.setFocus)
        self._filter("")

    def _filter(self, text: str) -> None:
        query = (text or "").strip().casefold()
        self.contents.blockSignals(True)
        self.contents.clear()
        first = None
        for page_id, title, body in self.pages:
            plain = re.sub(r"<[^>]+>", " ", body)
            if query and query not in (title + " " + html.unescape(plain)).casefold():
                continue
            item = QTreeWidgetItem([title])
            item.setData(0, Qt.ItemDataRole.UserRole, page_id)
            self.contents.addTopLevelItem(item)
            if first is None: first = item
        self.contents.blockSignals(False)
        if first is None:
            self.browser.setHtml(
                _SOFTWARE_STYLE + "<p>" + html.escape(
                    t("docs.software_no_results", default="No matching software guide.")) + "</p>"
            )
        else:
            self.contents.setCurrentItem(first)
            self._render(str(first.data(0, Qt.ItemDataRole.UserRole)))

    def _show_current(self, current: QTreeWidgetItem | None, _previous=None) -> None:
        if current is not None:
            self._render(str(current.data(0, Qt.ItemDataRole.UserRole) or ""))

    def _render(self, page_id: str) -> None:
        for candidate, _title, body in self.pages:
            if candidate == page_id:
                self.browser.setHtml(_SOFTWARE_STYLE + body)
                self.browser.verticalScrollBar().setValue(0)
                return

    def visible_page_ids(self) -> tuple[str, ...]:
        return tuple(
            str(self.contents.topLevelItem(i).data(0, Qt.ItemDataRole.UserRole) or "")
            for i in range(self.contents.topLevelItemCount())
        )


class DocumentationDialog(QDialog):
    """可与编辑器并行使用，且明确区分软件操作与脚本契约的独立文档窗口。"""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle(t("docs.window_title", default="Documentation — LOM Modkit"))
        self.setModal(False)
        self.resize(1120, 760)
        self.setMinimumSize(820, 560)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        self.tabs = QTabWidget(self)
        self.software = SoftwareDocumentationWidget(self.tabs)
        self.reference = NodeReferenceWidget(self.tabs)
        self.tabs.addTab(
            self.software,
            t("docs.software_tab", default="Software usage"),
        )
        self.tabs.addTab(
            self.reference,
            t("docs.script_tab", default="Script / API reference"),
        )
        layout.addWidget(self.tabs)
