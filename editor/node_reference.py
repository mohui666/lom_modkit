# -*- coding: utf-8 -*-
"""由实际节点 schema 生成的离线节点/API 参考界面。"""

from __future__ import annotations

import html
import json

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QHBoxLayout,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QSplitter,
    QTextBrowser,
    QVBoxLayout,
    QWidget,
)

from i18n import t
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
    "camera": "Camera preset / stage camera",
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
    "battle_setup": "MortalModHost battle setup",
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
    "dice": "Dice / CheckPoint metadata + MenuDialog",
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
    "dice_options": "reference.kind.target_table",
    "dice_check": "reference.kind.game_id",
    "vars": "reference.kind.structured",
    "effect": "reference.kind.game_id",
    "camera": "reference.kind.game_id",
    "menu_dialog": "reference.kind.game_id",
    "user_image": "reference.kind.user_id",
    "intro_image": "reference.kind.user_id",
    "ending_image": "reference.kind.game_or_user_id",
    "combat_id": "reference.kind.game_id",
    "battle_id": "reference.kind.game_id",
    "battle_preset": "reference.kind.preset_id",
    "goto_scene_key": "reference.kind.game_id",
    "death_id": "reference.kind.mod_id",
    "battle_setup_skills": "reference.kind.structured",
    "reward_entries": "reference.kind.nonempty_table",
    "reward_entries_optional": "reference.kind.structured",
    "custom_shop_items": "reference.kind.nonempty_table",
    "discount_toggle": "reference.kind.number_or_bool",
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
        (key, t(f"field.{key}", default=field_label), _kind_doc(kind), optional, example.get(key))
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
        table_rows.append(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td></tr>".format(
                html.escape(str(key)), html.escape(str(field_label)),
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
      body {{ color: #e8e8ed; font-family: sans-serif; font-size: 13px; }}
      h1 {{ font-size: 22px; margin-bottom: 2px; }}
      h2 {{ font-size: 16px; margin-top: 20px; }}
      .muted {{ color: #a8a8b2; }}
      .api {{ border-left: 4px solid #e9912c; padding: 8px 12px; background: #24242a; }}
      table {{ border-collapse: collapse; width: 100%; }}
      th, td {{ border-bottom: 1px solid #44444c; padding: 7px; text-align: left; vertical-align: top; }}
      th {{ color: #ffd7a0; }}
      code, pre {{ font-family: Consolas, monospace; }}
      pre {{ background: #1d1d22; padding: 10px; white-space: pre-wrap; }}
    </style>
    <h1>{html.escape(str(label))}</h1>
    <p class="muted">{html.escape(t('reference.story_type', default='Story type'))}: <code>{html.escape(node_type)}</code> · {html.escape(t('reference.category', default='Category'))}: {html.escape(_group_for(node_type))}</p>
    <p>{html.escape(help_text or str(schema.get('label', label)))}</p>
    <h2>{html.escape(t('reference.runtime_api', default='Runtime API'))}</h2>
    <div class="api"><code>{html.escape(api)}</code><br>{html.escape(t('reference.runtime_note', default='The editor saves Story JSON and lomc generates Lua. Authors do not call this API directly.'))}</div>
    <h2>{html.escape(t('reference.field_api', default='Field contract'))}</h2>
    <table><tr><th>{html.escape(t('reference.col.key', default='JSON key'))}</th><th>{html.escape(t('reference.col.meaning', default='UI meaning'))}</th><th>{html.escape(t('reference.col.requirement', default='Requirement'))}</th><th>{html.escape(t('reference.col.type', default='Type / values'))}</th><th>{html.escape(t('reference.col.default', default='New default'))}</th></tr>{''.join(table_rows)}</table>
    <h2>{html.escape(t('reference.example', default='Minimal example'))}</h2>
    <pre>{sample}</pre>
    <h2>{html.escape(t('reference.compatibility', default='Compatibility and safety'))}</h2>
    <p>{html.escape(t('reference.compatibility_note', default='Use only listed fields; the compiler rejects unknown fields. Game IDs come from the current extracted data and must be regenerated and retested after a game update.'))}</p>
    """


def reference_node_types() -> tuple[str, ...]:
    return tuple(models.NODE_TYPES)


class NodeReferenceWidget(QWidget):
    """帮助窗口中的可搜索节点/API 文档。"""

    def __init__(self, parent=None):
        super().__init__(parent)
        layout = QVBoxLayout(self)
        self.search = QLineEdit()
        self.search.setPlaceholderText(t("reference.search", default="Search nodes, fields, or APIs…"))
        self.search.setClearButtonEnabled(True)
        layout.addWidget(self.search)

        splitter = QSplitter(Qt.Orientation.Horizontal)
        self.node_list = QListWidget()
        self.node_list.setMinimumWidth(220)
        self.browser = QTextBrowser()
        self.browser.setOpenExternalLinks(False)
        splitter.addWidget(self.node_list)
        splitter.addWidget(self.browser)
        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)
        layout.addWidget(splitter, 1)

        self.search.textChanged.connect(self._filter)
        self.node_list.currentItemChanged.connect(self._show_current)
        self._filter("")

    def _filter(self, text: str) -> None:
        query = (text or "").strip().casefold()
        current = self.current_node_type()
        self.node_list.clear()
        for node_type in models.NODE_TYPES:
            schema = models.NODE_SCHEMAS[node_type]
            label = models.NODE_TYPE_CN.get(node_type, schema.get("label", node_type))
            field_terms = []
            for key, field_label, kind, _optional in schema["fields"]:
                field_terms.extend((key, t(f"field.{key}", default=field_label), _kind_doc(kind)))
            fields = " ".join(field_terms)
            haystack = " ".join(
                (node_type, str(label), fields, models.NODE_HELP.get(node_type, ""), RUNTIME_API.get(node_type, ""))
            ).casefold()
            if query and query not in haystack:
                continue
            item = QListWidgetItem(f"{_group_for(node_type)} · {label}")
            item.setData(Qt.ItemDataRole.UserRole, node_type)
            item.setToolTip(f"{label} ({node_type})")
            self.node_list.addItem(item)
            if node_type == current:
                self.node_list.setCurrentItem(item)
        if self.node_list.currentRow() < 0 and self.node_list.count():
            self.node_list.setCurrentRow(0)
        if self.node_list.count() == 0:
            self.browser.setHtml(
                f"<p>{html.escape(t('reference.no_matches', default='No matching nodes or fields.'))}</p>"
            )

    def _show_current(self, current: QListWidgetItem | None, _previous=None) -> None:
        if current is None:
            return
        node_type = str(current.data(Qt.ItemDataRole.UserRole) or "")
        self.browser.setHtml(node_reference_html(node_type))

    def current_node_type(self) -> str:
        item = self.node_list.currentItem()
        return str(item.data(Qt.ItemDataRole.UserRole) or "") if item else ""
