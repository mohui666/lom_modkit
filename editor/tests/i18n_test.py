# -*- coding: utf-8 -*-
"""多语言：界面键齐全，游戏名词来自 wiki / 官方解包。"""

from __future__ import annotations

import json
import ast
import sys
from pathlib import Path

EDITOR = Path(__file__).resolve().parent.parent
if str(EDITOR) not in sys.path:
    sys.path.insert(0, str(EDITOR))

from i18n import current_language, set_language, t, term  # noqa: E402
import models  # noqa: E402


def _keys(locale: str) -> set[str]:
    raw = json.loads((EDITOR / "i18n" / "locales" / f"{locale}.json").read_text(encoding="utf-8"))

    def walk(data, prefix=""):
        out = set()
        for key, value in data.items():
            full = f"{prefix}.{key}" if prefix else key
            if isinstance(value, dict):
                out |= walk(value, full)
            else:
                out.add(full)
        return out

    return walk(raw)


def test_locale_keys_match():
    source = _keys("chs")
    for locale in ("cht", "ja", "ko"):
        got = _keys(locale)
        missing = source - got
        extra = got - source
        assert not missing, f"{locale} 缺少键：{sorted(missing)[:12]}"
        assert not extra, f"{locale} 多出键：{sorted(extra)[:12]}"
    print("[i18n] 四套界面键一致", len(source))


def test_all_static_t_calls_and_schema_fields_are_translated():
    locale_keys = _keys("chs")
    used = set()
    for path in EDITOR.rglob("*.py"):
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if (
                isinstance(node, ast.Call)
                and isinstance(node.func, ast.Name)
                and node.func.id == "t"
                and node.args
                and isinstance(node.args[0], ast.Constant)
                and isinstance(node.args[0].value, str)
            ):
                used.add(node.args[0].value)
    assert not used - locale_keys, f"未翻译 t() 键：{sorted(used - locale_keys)}"
    missing = {
        f"field.{node_type}.{field}"
        for node_type, schema in models.NODE_SCHEMAS.items()
        for field, _label, _kind, _optional in schema["fields"]
        if f"field.{node_type}.{field}" not in locale_keys and f"field.{field}" not in locale_keys
    }
    assert not missing, f"节点字段缺少翻译：{sorted(missing)}"


def test_default_zh_cn_keeps_existing_labels():
    set_language("chs")
    models.refresh_labels()
    assert current_language() == "chs"
    assert models.NODE_TYPE_CN["say"] == "对白"
    assert models.NODE_TYPE_CN["affinity"] == "好感度"
    assert [g for g, _t in models.NODE_GROUPS] == [
        "画面与声音",
        "数值、物品与任务",
        "战斗与游戏系统",
        "流程与高级功能",
    ]
    s = models.node_summary(
        {"id": "x", "type": "say", "mode": "center", "text": "居中"},
        models.FALLBACK_EDITOR_DATA,
    )
    assert s.startswith("对白·居中旁白"), s
    print("[i18n] 默认简中与旧标签兼容")


def test_wiki_and_official_terms():
    # 简中 / 繁中：wiki 与游戏官方表
    set_language("chs")
    assert term("characters", "sister1") == "唐默铃"
    assert term("characters", "player") == "赵活"
    assert term("stats", "mental") == "心相"
    assert term("common", "endgame_book") == "汗青书"
    assert term("common", "death_book") == "生死簿"
    assert term("common", "affinity") == "好感度"
    assert term("common", "lover") == "心上人"

    set_language("cht")
    assert term("characters", "sister1") == "唐默鈴"
    assert term("characters", "brother3") == "唐陞"
    assert term("stats", "training") == "修養"
    assert term("common", "endgame_book") == "汗青書"
    assert term("common", "tang_sect") == "唐門"

    # 日语：官方游戏没有日语，人物汉字与属性名来自 wiki 日文页
    set_language("ja")
    assert term("characters", "sister1") == "唐默鈴"
    assert term("stats", "talking") == "弁舌"
    assert term("stats", "team") == "団結"
    assert term("stats", "training") == "品性"
    assert term("common", "endgame_book") == "汗青書"

    # 韩语：wiki 无韩语，取自游戏解包官方表
    set_language("ko")
    assert term("characters", "sister1") == "당묵령"
    assert term("characters", "player") == "조활"
    assert term("stats", "mental") == "심상"
    assert term("stats", "relationship") == "호감도"
    assert term("common", "endgame_book") == "한청서"
    assert term("common", "death_book") == "생사부"
    assert term("common", "game_title") == "활협전"
    print("[i18n] wiki / 官方游戏名词抽查 OK")


def test_language_switch_node_labels():
    set_language("ko")
    models.refresh_labels()
    assert models.NODE_TYPE_CN["say"] == "대사"
    assert models.NODE_TYPE_CN["affinity"] == "호감도"
    set_language("ja")
    models.refresh_labels()
    assert models.NODE_TYPE_CN["say"] == "台詞"
    assert t("nav.ending_card") == "汗青書エンディング"
    assert models.NODE_TYPE_CN["enemy"] == "敵陣営"
    enemy_ops = dict(models.ENUM_SETS["enemy_op"])
    assert enemy_ops["team"] == "敵の結束力を変更"
    assert enemy_ops["people"] == "敵陣営の人数を変更"
    assert term("enemy_teams", "201")
    assert term("battle_skills", "special3")
    set_language("chs")
    models.refresh_labels()
    print("[i18n] 切换语言后节点名更新")


def test_legacy_language_codes_are_input_only_aliases():
    assert set_language("zh_CN") == "chs"
    assert current_language() == "chs"
    assert set_language("zh_TW") == "cht"
    assert current_language() == "cht"
    set_language("chs")


def test_gameplay_templates_are_readable_in_every_language():
    data = json.loads((EDITOR.parent / "data" / "editor_data.json").read_text(encoding="utf-8"))
    self_contained = ("chs", "cht", "ja", "ko")
    for locale in self_contained:
        set_language(locale)
        combat_id, combat_label = models.list_items(data, "combat_ids")[0]
        battle_id, battle_label = models.list_items(data, "battle_ids")[0]
        assert combat_id not in combat_label, (locale, combat_label)
        assert battle_id not in battle_label, (locale, battle_label)
        assert "{name}" not in combat_label and "{name}" not in battle_label
    set_language("chs")


if __name__ == "__main__":
    test_locale_keys_match()
    test_all_static_t_calls_and_schema_fields_are_translated()
    test_default_zh_cn_keeps_existing_labels()
    test_wiki_and_official_terms()
    test_language_switch_node_labels()
    test_legacy_language_codes_are_input_only_aliases()
    test_gameplay_templates_are_readable_in_every_language()
    print("i18n tests OK")
