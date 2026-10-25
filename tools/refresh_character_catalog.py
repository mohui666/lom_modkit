#!/usr/bin/env python3
"""Refresh portrait choices from a user's installed StoryCharacterConfig.

Only our metadata is written. No images, game code or assemblies are copied.
Unlike scanning story scripts, this includes expressions not used by a script.
"""
import argparse
import json
from pathlib import Path

import UnityPy
import _probe_assets as probe

ROOT = Path(__file__).resolve().parents[1]
IMPORTANT = ("player", "brother1", "brother2", "brother3", "brother4", "sister1",
             "master", "girl1", "girl2", "girl4", "girl5", "girl6", "girl7",
             "girl8", "girl9", "girl2_4", "special102", "special835", "special999")
SUFFIXES = {"隱": "剪影", "早": "早期", "幼年2": "幼年", "幼年": "幼年",
            "魂": "魂魄", "公子": "公子装", "夜行": "夜行装", "飛賊": "飞贼（化名）",
            "入魔": "入魔", "大師兄": "大师兄装", "半": "半身", "全武裝": "全副武装",
            "無鎚": "无锤", "盛裝": "盛装", "眼鏡": "眼镜", "散髮": "散发",
            "問號": "身份不明", "宮裝": "宫装", "蒙面": "蒙面", "年輕": "年轻",
            "少年": "少年", "鳳凰": "凤凰", "綁": "被绑", "上": "上半身", "下": "下半身"}
APPEARANCE_NAMES = {"playerRight": "右侧", "playerLeft": "左侧", "player_unknown": "身份不明",
                    "player_old": "老年", "player_section": "门派像", "player_soul": "魂魄",
                    "player_imagine": "入魔", "player_ass": "背身", "player_hand": "手部",
                    "sister1_look": "瞪视", "brother1_2": "另一造型", "girl2_2": "特殊造型",
                    "girl5_hand": "幼年手部"}


def read_names(data):
    tables = {}
    for obj in UnityPy.load(str(data / "sharedassets0.assets")).objects:
        if obj.type.name != "TextAsset":
            continue
        asset = obj.read()
        if asset.m_Name not in ("Character_zh-cn", "Character_zh-tw", "Character_kr"):
            continue
        table = {}
        for line in asset.m_Script.splitlines():
            key, sep, value = line.partition(" = ")
            if sep and key.startswith("Character/"):
                table[key.removeprefix("Character/")] = value.strip()
        tables[asset.m_Name] = table
    return tables


def refresh(game, output=ROOT / "data/editor_data.json"):
    data = game / "Mortal_Data"
    probe.SA2 = str(data / "sharedassets2.assets")
    configs, _ = probe.load_sa2()
    if "girl5_9" not in configs:
        raise ValueError("未找到当前游戏的小梅入魔配置，请核对安装目录和游戏版本")
    tables = read_names(data)
    document = json.loads(output.read_text(encoding="utf-8"))
    rows = {row["id"]: row for row in document["characters"]}
    added = 0
    for cid, config in sorted(configs.items()):
        if cid == "player_beautified":  # preview key, never an official character ID
            continue
        if cid not in rows:
            rows[cid] = {"id": cid, "name": tables["Character_zh-cn"].get(cid, config["name"])}
            added += 1
        row = rows[cid]
        row["portraits"] = sorted(config["portraits"])
        row["default_portrait"] = next((key for key, address in config["portraits"].items()
                                         if address == config["first"]), "normal")
        for key in ("appearance_group", "appearance_name"):
            row.pop(key, None)
        group = next((root for root in IMPORTANT if cid == root or cid.startswith(root + "_")), None)
        if cid in ("girl2_4", "girl2_3"):
            group = "girl2_4"  # 叶云啾 is a separate person from her sister 叶云裳.
        if cid in ("playerLeft", "playerRight"):
            group = "player"
        if group:
            row["appearance_group"] = group
            suffix = config["name"].removeprefix(configs[group]["name"]).lstrip("_")
            row["appearance_name"] = "普通" if cid == group else APPEARANCE_NAMES.get(cid, SUFFIXES.get(suffix, suffix or "普通（备用）"))
            if group == "girl5" and cid == "girl5_4":
                row["appearance_name"] = "普通（备用）"  # live config points to adult normal
    document["characters"] = sorted(rows.values(), key=lambda row: row["id"])
    document["player_appearance_portraits"] = {
        "original": sorted(configs["player"]["portraits"]),
        "beautified": sorted(configs["player_beautified"]["portraits"]),
    }
    output.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    # Existing wiki-localized names retain priority; fill only missing official names.
    for locale, source in [("chs", "Character_zh-cn"), ("cht", "Character_zh-tw"),
                           ("ja", "Character_zh-tw"), ("ko", "Character_kr")]:
        path = ROOT / f"editor/i18n/terms/{locale}.json"
        terms = json.loads(path.read_text(encoding="utf-8"))
        for cid, name in tables[source].items():
            if cid in configs:
                terms["characters"].setdefault(cid, name)
        path.write_text(json.dumps(terms, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return {"added_characters": added, "characters": len(rows),
            "portrait_choices": sum(len(row["portraits"]) for row in rows.values())}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--game-dir", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(refresh(args.game_dir), ensure_ascii=False))
