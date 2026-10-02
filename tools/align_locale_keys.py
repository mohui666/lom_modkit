# -*- coding: utf-8 -*-
"""修正上游遗留的语言包不一致（只跑一次的工具脚本）。

问题：仓库自带的 ``editor/tests/i18n_test.py::test_locale_keys_match`` 断言四套
语言包的键完全一致，但 cht / ja / ko 实际各有 7 个键缺失、5~9 个键是改名前的
残留，所以这个测试在上游一直跑不过。

修法：
- 删掉已无代码引用的残留键（``field.battle.*_faction`` / ``*_people``、
  ``reference.effect.battle_people``，以及 ja/ko 里已被 ``enum.enemy_op.*``
  取代的 ``enum.enemy_*``）；
- 给三套语言包补上缺失的 7 个键（战役标题与血量、附加兵种、战役血量/标题说明）。

按文本增删，不整份重写，避免把用空行分组的大文件抖成 900 行 diff。
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

LOCALES = Path(__file__).resolve().parent.parent / "editor" / "i18n" / "locales"

STALE_KEYS = (
    "field.battle.enemy_faction",
    "field.battle.enemy_people",
    "field.battle.friend_faction",
    "field.battle.friend_people",
    "reference.effect.battle_people",
    # ja / ko 独有的死键：_ENUM_KEY_OVERRIDE 指向它们，但同名的
    # enum.enemy_op.<值> 才是 refresh_labels() 真正取到的那一层。
    "enum.enemy_cohesion",
    "enum.enemy_scale",
    "enum.enemy_people",
    "enum.enemy_current",
)

NEW_TEXTS = {
    "field.battle.friend_characters": {
        "cht": "我方附加具名角色",
        "ja": "味方の追加名付きキャラ",
        "ko": "아군 추가 지명 캐릭터",
    },
    "field.battle.enemy_characters": {
        "cht": "敵方附加具名角色",
        "ja": "敵の追加名付きキャラ",
        "ko": "적군 추가 지명 캐릭터",
    },
    "field.battle.title": {
        "cht": "戰役標題",
        "ja": "戦役タイトル",
        "ko": "전역 제목",
    },
    "field.battle.friend_health": {
        "cht": "我方 NPC 基礎血量",
        "ja": "味方 NPC の基礎体力",
        "ko": "아군 NPC 기본 체력",
    },
    "field.battle.enemy_health": {
        "cht": "敵方 NPC 基礎血量",
        "ja": "敵 NPC の基礎体力",
        "ko": "적군 NPC 기본 체력",
    },
    "field.battle.friend_factions": {
        "cht": "我方附加兵種（每項單獨設人數，總人數自動相加）",
        "ja": "味方の追加兵種（項目ごとに人数を設定、合計は自動計算）",
        "ko": "아군 추가 병종(항목별로 인원을 설정하고 총인원은 자동 합산)",
    },
    "field.battle.enemy_factions": {
        "cht": "敵方附加兵種（每項單獨設人數，總人數自動相加）",
        "ja": "敵の追加兵種（項目ごとに人数を設定、合計は自動計算）",
        "ko": "적군 추가 병종(항목별로 인원을 설정하고 총인원은 자동 합산)",
    },
    "reference.effect.battle_health": {
        "cht": "覆蓋本次生成的「{field}」。原版 CharacterHealth 會複製 HealthData，不會改動官方資產。",
        "ja": "今回生成する「{field}」を上書きします。原版の CharacterHealth は HealthData を複製するため、公式アセットは変更されません。",
        "ko": "이번에 생성되는 '{field}'를 덮어씁니다. 원본 CharacterHealth는 HealthData를 복제하므로 공식 에셋은 변경되지 않습니다.",
    },
    "reference.effect.battle_title": {
        "cht": "寫入原版 ReadyPanel 敵軍名那一行；留空則顯示目前戰役殼的官方陣營名。",
        "ja": "原版 ReadyPanel の敵軍名の行に書き込みます。空欄の場合は現在の戦役シェルの公式陣営名を表示します。",
        "ko": "원본 ReadyPanel의 적군 이름 줄에 기록합니다. 비워 두면 현재 전역 셸의 공식 진영 이름이 표시됩니다.",
    },
}


def flatten(raw: dict, prefix: str = "") -> set[str]:
    out: set[str] = set()
    for key, value in raw.items():
        full = f"{prefix}.{key}" if prefix else key
        if isinstance(value, dict):
            out |= flatten(value, full)
        else:
            out.add(full)
    return out


_VALUE = r'"(?:[^"\\]|\\.)*"'


def remove_entry(text: str, key: str) -> tuple[str, bool]:
    """从 JSON 文本中挖掉一个键值对。

    这些文件并非严格「一行一键」——有 127 行是两项挤在一行的，所以不能按行删。
    这里按语法片段删：优先匹配带尾逗号的写法，其次匹配「最后一项，前面带逗号」。
    """
    name = re.escape(json.dumps(key, ensure_ascii=False))
    with_comma = re.compile(r"\s*" + name + r"\s*:\s*" + _VALUE + r"\s*,")
    if with_comma.search(text):
        return with_comma.sub("", text, count=1), True
    last_entry = re.compile(r",\s*" + name + r"\s*:\s*" + _VALUE)
    if last_entry.search(text):
        return last_entry.sub("", text, count=1), True
    return text, False


def append_keys(text: str, pairs: list[tuple[str, str]]) -> str:
    body = text.rstrip()
    if not body.endswith("}"):
        raise SystemExit("语言包结尾不是 JSON 对象，已中止")
    body = body[:-1].rstrip()
    if not body.endswith(","):
        body += ","
    entries = ",\n".join(
        f"  {json.dumps(k, ensure_ascii=False)}: {json.dumps(v, ensure_ascii=False)}"
        for k, v in pairs
    )
    return f"{body}\n{entries}\n}}\n"


def main() -> int:
    chs = json.loads((LOCALES / "chs.json").read_text(encoding="utf-8"))
    target = flatten(chs)
    stale = set(STALE_KEYS)
    ok = True
    for code in ("cht", "ja", "ko"):
        path = LOCALES / f"{code}.json"
        text = path.read_text(encoding="utf-8")
        current = flatten(json.loads(text))
        removed: list[str] = []
        for key in sorted(stale & current):
            text, did = remove_entry(text, key)
            if did:
                removed.append(key)
        if removed:
            json.loads(text)  # 删完必须仍是合法 JSON，否则宁可不改
        additions = [
            (key, texts[code])
            for key, texts in NEW_TEXTS.items()
            if key not in current
        ]
        if additions:
            text = append_keys(text, additions)
        path.write_text(text, encoding="utf-8")
        after = flatten(json.loads(path.read_text(encoding="utf-8")))
        missing, extra = target - after, after - target
        status = "一致" if not missing and not extra else "仍不一致"
        ok = ok and not missing and not extra
        print(
            f"[{code}] 删除 {len(removed)} 个残留键、补 {len(additions)} 个缺失键 → {status}"
            f"（{len(after)} 键）"
        )
        if missing:
            print("   仍缺：", sorted(missing)[:8])
        if extra:
            print("   仍多：", sorted(extra)[:8])
    print("四套语言包键集合一致：", ok)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
