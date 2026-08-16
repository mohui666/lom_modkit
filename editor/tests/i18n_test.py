# -*- coding: utf-8 -*-
"""多语言：界面键齐全，游戏名词来自 wiki / 官方解包。"""

from __future__ import annotations

import json
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
    source = _keys("zh_CN")
    for locale in ("zh_TW", "ja", "ko"):
        got = _keys(locale)
        missing = source - got
        extra = got - source
        assert not missing, f"{locale} 缺少键：{sorted(missing)[:12]}"
        assert not extra, f"{locale} 多出键：{sorted(extra)[:12]}"
    print("[i18n] 四套界面键一致", len(source))


def test_default_zh_cn_keeps_existing_labels():
    set_language("zh_CN")
    models.refresh_labels()
    assert current_language() == "zh_CN"
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
    set_language("zh_CN")
    assert term("characters", "sister1") == "唐默铃"
    assert term("characters", "player") == "赵活"
    assert term("stats", "mental") == "心相"
    assert term("common", "endgame_book") == "汗青书"
    assert term("common", "death_book") == "生死簿"
    assert term("common", "affinity") == "好感度"
    assert term("common", "lover") == "心上人"

    set_language("zh_TW")
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
    set_language("zh_CN")
    models.refresh_labels()
    print("[i18n] 切换语言后节点名更新")


if __name__ == "__main__":
    test_locale_keys_match()
    test_default_zh_cn_keeps_existing_labels()
    test_wiki_and_official_terms()
    test_language_switch_node_labels()
    print("i18n tests OK")
