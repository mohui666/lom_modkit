#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""从游戏解包表 + LoM-wiki 生成编辑器游戏名词对照。

优先级（用户约定）：
1. wiki https://github.com/mohui666/LoM-wiki-CNS 有的名词用 wiki
2. wiki 没有的（韩语全文、大量物品/天赋/场景）用 lom_unpack 官方语言表

官方解包只有 zh-cn / zh-tw / kr。日语人物名按 wiki 日文页（与繁中汉字同形），
属性名按 wiki 日文设施页的译法。
"""

from __future__ import annotations

import json
import os
import re
from pathlib import Path

UNPACK = Path(os.environ.get("LOM_UNPACK_DIR", r"C:/Users/mohui666/lom_unpack"))
RAW = UNPACK / "raw"
WIKI_RAW = Path(os.environ.get("TEMP", r"C:/Users/mohui666/AppData/Local/Temp")) / "lom_wiki_raw"
EDITOR_DATA = Path(__file__).resolve().parent.parent / "data" / "editor_data.json"
OUT_DIR = Path(__file__).resolve().parent.parent / "editor" / "i18n" / "terms"

TABLES = {
    "characters": ("Character_{lang}.txt", "Character/"),
    "stats": ("Stat_{lang}.txt", "PlayerStat/"),
    "talents": ("Talent_{lang}.txt", "PlayerTalent/Name/"),
    "items_book": ("ItemBook_{lang}.txt", "Book/Name/"),
    "items_misc": ("ItemMisc_{lang}.txt", "Misc/Name/"),
    "items_special": ("ItemSpecial_{lang}.txt", "Special/Name/"),
    "enemy_teams": ("EnemyTeam_{lang}.txt", "EnemyTeam/"),
    "battle_skills": ("BattleSkill_{lang}.txt", "BattleSkill/Name/"),
    "free_positions": ("Position_{lang}.txt", "Position/Name/"),
    "system": ("System_{lang}.txt", "System/"),
}

# wiki 日文设施页对属性的译法（解包无日语）
WIKI_JA_STATS = {
    "money": "銀両",
    "life": "体力",
    "stamina": "内力",
    "dexterity": "軽功",
    "literacy": "学問",
    "mental": "心相",
    "fate": "運命",
    "disposition": "性情",
    "behaviour": "処世",
    "training": "品性",
    "karma": "道徳",
    "talking": "弁舌",
    "fame": "名声",
    "people": "門人",
    "team": "団結",
    "weapon": "鍛造",
    "poison": "煉丹",
    "contribution": "貢献度",
    "internal": "陰陽",
    "change-heart": "変心",
    "action": "行動回数",
    "poison-resistance": "抗毒",
    "paralysis-resistance": "抗麻痺",
    "martial-point": "武学",
    "assets": "門派資産",
    "m-sword": "刀剣",
    "m-projectile": "暗器",
    "m-fist": "拳掌",
    "relationship": "好感度",
    "default-battle-hp": "門人の体力",
    "lover": "心上人",
}

# wiki / 官方系统名词
WIKI_COMMON = {
    "chs": {
        "game_title": "活侠传",
        "tang_sect": "唐门",
        "affinity": "好感度",
        "lover": "心上人",
        "endgame_book": "汗青书",
        "death_book": "生死簿",
        "achievement": "风云史",
        "library": "讲经堂",
        "free_mode": "自由模式",
        "junior_sister": "小师妹",
        "eldest_brother": "大师兄",
        "second_brother": "二师兄",
        "third_brother": "三师兄",
        "fourth_brother": "四师兄",
        "sect_leader": "掌门",
    },
    "cht": {
        "game_title": "活俠傳",
        "tang_sect": "唐門",
        "affinity": "好感度",
        "lover": "心上人",
        "endgame_book": "汗青書",
        "death_book": "生死簿",
        "achievement": "風雲史",
        "library": "講經堂",
        "free_mode": "自由模式",
        "junior_sister": "小師妹",
        "eldest_brother": "大師兄",
        "second_brother": "二師兄",
        "third_brother": "三師兄",
        "fourth_brother": "四師兄",
        "sect_leader": "掌門",
    },
    "ja": {
        "game_title": "活俠傳",
        "tang_sect": "唐門",
        "affinity": "好感度",
        "lover": "心上人",
        "endgame_book": "汗青書",
        "death_book": "生死簿",
        "achievement": "風雲史",
        "library": "講経堂",
        "free_mode": "フリーモード",
        "junior_sister": "師妹",
        "eldest_brother": "大師兄",
        "second_brother": "二師兄",
        "third_brother": "三師兄",
        "fourth_brother": "四師兄",
        "sect_leader": "掌門",
    },
    "ko": {
        "game_title": "활협전",
        "tang_sect": "당문",
        "affinity": "호감도",
        "lover": "마음에 둔 사람",
        "endgame_book": "한청서",
        "death_book": "생사부",
        "achievement": "풍운사",
        "library": "강경당",
        "free_mode": "자유 모드",
        "junior_sister": "소사매",
        "eldest_brother": "대사형",
        "second_brother": "이사형",
        "third_brother": "삼사형",
        "fourth_brother": "사사형",
        "sect_leader": "장문인",
    },
}

VIEW_TIME = {
    "chs": {
        "白天": "白天",
        "夜晚": "夜晚",
        "黃昏": "黄昏",
        "黄昏": "黄昏",
        "早上": "早上",
        "夜宴": "夜宴",
        "晴天": "晴天",
        "夜雨": "夜雨",
        "孤雲山": "孤云山",
        "孤云山": "孤云山",
    },
    "cht": {},
    "ja": {
        "白天": "昼",
        "夜晚": "夜",
        "黃昏": "黄昏",
        "黄昏": "黄昏",
        "早上": "朝",
        "夜宴": "夜宴",
        "晴天": "晴天",
        "夜雨": "夜雨",
    },
    "ko": {
        "白天": "낮",
        "夜晚": "밤",
        "黃昏": "황혼",
        "黄昏": "황혼",
        "早上": "아침",
        "夜宴": "밤연회",
        "晴天": "맑음",
        "夜雨": "밤비",
        "孤雲山": "고운산",
        "孤云山": "고운산",
    },
}

# 场景专名：wiki / 解包地点表
VIEW_PLACE = {
    "chs": {
        "煉丹房": "炼丹房",
        "後山": "后山",
        "後山2": "后山2",
        "後山棧道": "后山栈道",
        "後山棧道2": "后山栈道2",
        "後院": "后院",
        "宴會": "宴会",
        "戰場": "战场",
        "杏花林": "杏花林",
        "畫舫": "画舫",
        "小舟": "小舟",
        "馬車內": "马车内",
        "地窖": "地窖",
        "校場": "校场",
        "原野": "原野",
        "城門": "城门",
        "峭壁": "峭壁",
        "武館": "武馆",
        "海岸": "海岸",
        "菜園": "菜园",
        "渡口": "渡口",
        "柴房": "柴房",
        "小吃攤": "小吃摊",
        "樹林": "树林",
        "鍛冶場": "锻冶场",
        "鐵鋪": "铁铺",
        "外堡": "外堡",
        "外堡房間2": "外堡房间2",
        "草原": "草原",
        "招待所": "招待所",
        "巷口人家": "巷口人家",
        "客棧": "客栈",
        "客棧2": "客栈2",
        "客房": "客房",
        "伙房": "伙房",
        "洞庭湖": "洞庭湖",
        "大廳1": "大厅1",
        "全地圖": "全地图",
        "武林大會": "武林大会",
        "山頂": "山顶",
        "孤雲山": "孤云山",
        "山道": "山道",
        "山步道": "山步道",
        "山澗": "山涧",
        "野店": "野店",
        "水田": "水田",
        "藥鋪1": "药铺1",
        "公主房": "公主房",
        "牢房": "牢房",
        "江邊": "江边",
        "大街": "大街",
        "大街2": "大街2",
        "大街3": "大街3",
        "寢室": "寝室",
        "崆峒_玄功洞": "崆峒·玄功洞",
        "崆峒_練武場": "崆峒·练武场",
        "崆峒": "崆峒",
        "崆峒_鐵拳門": "崆峒·铁拳门",
        "崆峒_崆峒山地圖": "崆峒·崆峒山地图",
        "崆峒_市集": "崆峒·市集",
        "崆峒_奪魄峰": "崆峒·夺魄峰",
        "崆峒_道觀": "崆峒·道观",
        "青城派": "青城派",
        "青城道觀": "青城道观",
        "青城派_青城山地圖": "青城派·青城山地图",
        "峨嵋派": "峨眉派",
        "全真派": "全真派",
        "嵩山派": "嵩山派",
        "點蒼派": "点苍派",
        "錦香宮": "锦香宫",
        "錦香宮_宮殿": "锦香宫·宫殿",
        "錦香宮_武英殿": "锦香宫·武英殿",
        "門派戶外": "门派户外",
        "道士房間": "道士房间",
        "大船": "大船",
        "天空": "天空",
        "雪山": "雪山",
        "雪路": "雪路",
        "溫泉": "温泉",
        "溪邊": "溪边",
        "講經堂": "讲经堂",
        "書齋": "书斋",
        "冥河": "冥河",
        "正心堂": "正心堂",
        "唐門全景": "唐门全景",
        "練功塔": "练功塔",
        "茶鋪": "茶铺",
        "破廟": "破庙",
        "樹頂": "树顶",
        "地底湖": "地底湖",
        "陰曹地府": "阴曹地府",
        "山寨": "山寨",
        "白虎堂": "白虎堂",
        "曠野": "旷野",
        "窗下": "窗下",
        "衙門": "衙门",
        "死亡": "死亡",
    },
    "ja": {
        "煉丹房": "煉丹房",
        "後山": "裏山",
        "後山2": "裏山2",
        "後山棧道": "裏山の桟道",
        "後山棧道2": "裏山の桟道2",
        "後院": "裏庭",
        "宴會": "宴会",
        "戰場": "戦場",
        "杏花林": "杏花林",
        "畫舫": "画舫",
        "小舟": "小舟",
        "馬車內": "馬車の中",
        "地窖": "地下倉",
        "校場": "演武場",
        "原野": "原野",
        "城門": "城門",
        "峭壁": "断崖",
        "武館": "武館",
        "海岸": "海岸",
        "菜園": "菜園",
        "渡口": "渡し場",
        "柴房": "薪小屋",
        "小吃攤": "屋台",
        "樹林": "林",
        "鍛冶場": "鍛冶場",
        "鐵鋪": "鉄工房",
        "外堡": "外堡",
        "外堡房間2": "外堡の部屋2",
        "草原": "草原",
        "招待所": "客間",
        "巷口人家": "路地の民家",
        "客棧": "宿屋",
        "客棧2": "宿屋2",
        "客房": "客室",
        "伙房": "厨房",
        "洞庭湖": "洞庭湖",
        "大廳1": "広間",
        "全地圖": "全体地図",
        "武林大會": "武林大会",
        "山頂": "山頂",
        "孤雲山": "孤雲山",
        "山道": "山道",
        "山步道": "山の小径",
        "山澗": "渓谷",
        "野店": "野店",
        "水田": "水田",
        "藥鋪1": "薬舗",
        "公主房": "公主の部屋",
        "牢房": "牢獄",
        "江邊": "川辺",
        "大街": "大通り",
        "大街2": "大通り2",
        "大街3": "大通り3",
        "寢室": "寝室",
        "崆峒_玄功洞": "崆峒・玄功洞",
        "崆峒_練武場": "崆峒・練武場",
        "崆峒": "崆峒",
        "崆峒_鐵拳門": "崆峒・鉄拳門",
        "崆峒_崆峒山地圖": "崆峒山地図",
        "崆峒_市集": "崆峒・市場",
        "崆峒_奪魄峰": "崆峒・奪魄峰",
        "崆峒_道觀": "崆峒・道観",
        "青城派": "青城派",
        "青城道觀": "青城道観",
        "青城派_青城山地圖": "青城山地図",
        "峨嵋派": "峨嵋派",
        "全真派": "全真派",
        "嵩山派": "嵩山派",
        "點蒼派": "点蒼派",
        "錦香宮": "錦香宮",
        "錦香宮_宮殿": "錦香宮・宮殿",
        "錦香宮_武英殿": "錦香宮・武英殿",
        "門派戶外": "門派の屋外",
        "道士房間": "道士の部屋",
        "大船": "大船",
        "天空": "空",
        "雪山": "雪山",
        "雪路": "雪道",
        "溫泉": "温泉",
        "溪邊": "渓流のほとり",
        "講經堂": "講経堂",
        "書齋": "書斎",
        "冥河": "冥河",
        "正心堂": "正心堂",
        "唐門全景": "唐門全景",
        "練功塔": "練功塔",
        "茶鋪": "茶舗",
        "破廟": "廃寺",
        "樹頂": "樹の上",
        "地底湖": "地底湖",
        "陰曹地府": "冥府",
        "山寨": "山寨",
        "白虎堂": "白虎堂",
        "曠野": "曠野",
        "窗下": "窓の下",
        "衙門": "衙門",
        "死亡": "死亡",
    },
    "ko": {
        "煉丹房": "연단방",
        "後山": "뒷산",
        "後山2": "뒷산2",
        "後山棧道": "뒷산 잔도",
        "後山棧道2": "뒷산 잔도2",
        "後院": "뒤뜰",
        "宴會": "연회",
        "戰場": "전장",
        "杏花林": "행화림",
        "畫舫": "화방",
        "小舟": "작은 배",
        "馬車內": "마차 안",
        "地窖": "지하실",
        "校場": "연무장",
        "原野": "들판",
        "城門": "성문",
        "峭壁": "절벽",
        "武館": "무관",
        "海岸": "해안",
        "菜園": "밭",
        "渡口": "나루터",
        "柴房": "땔감 창고",
        "小吃攤": "포장마차",
        "樹林": "숲",
        "鍛冶場": "대장간",
        "鐵鋪": "철물점",
        "外堡": "외성",
        "外堡房間2": "외성 방2",
        "草原": "초원",
        "招待所": "객실",
        "巷口人家": "골목집",
        "客棧": "객잔",
        "客棧2": "객잔2",
        "客房": "객실",
        "伙房": "부엌",
        "洞庭湖": "동정호",
        "大廳1": "대청",
        "全地圖": "전체 지도",
        "武林大會": "무림대회",
        "山頂": "산꼭대기",
        "孤雲山": "고운산",
        "山道": "산길",
        "山步道": "산 산책로",
        "山澗": "산골짜기",
        "野店": "주막",
        "水田": "논",
        "藥鋪1": "약방",
        "公主房": "공주 방",
        "牢房": "감옥",
        "江邊": "강가",
        "大街": "거리",
        "大街2": "거리2",
        "大街3": "거리3",
        "寢室": "침실",
        "崆峒_玄功洞": "공동·현공동",
        "崆峒_練武場": "공동·연무장",
        "崆峒": "공동",
        "崆峒_鐵拳門": "공동·철권문",
        "崆峒_崆峒山地圖": "공동산 지도",
        "崆峒_市集": "공동·시장",
        "崆峒_奪魄峰": "공동·탈백봉",
        "崆峒_道觀": "공동·도관",
        "青城派": "청성파",
        "青城道觀": "청성 도관",
        "青城派_青城山地圖": "청성산 지도",
        "峨嵋派": "아미파",
        "全真派": "전진파",
        "嵩山派": "숭산파",
        "點蒼派": "점창파",
        "錦香宮": "금향궁",
        "錦香宮_宮殿": "금향궁·궁전",
        "錦香宮_武英殿": "금향궁·무영전",
        "門派戶外": "문파 야외",
        "道士房間": "도사 방",
        "大船": "큰 배",
        "天空": "하늘",
        "雪山": "설산",
        "雪路": "눈길",
        "溫泉": "온천",
        "溪邊": "시냇가",
        "講經堂": "강경당",
        "書齋": "서재",
        "冥河": "명하",
        "正心堂": "정심당",
        "唐門全景": "당문 전경",
        "練功塔": "연공탑",
        "茶鋪": "찻집",
        "破廟": "폐사",
        "樹頂": "나무 위",
        "地底湖": "지하호",
        "陰曹地府": "명부",
        "山寨": "산채",
        "白虎堂": "백호당",
        "曠野": "광야",
        "窗下": "창 아래",
        "衙門": "아문",
        "死亡": "사망",
    },
}

LINE_RE = re.compile(r"^([^=]+?)\s*=\s*(.*)$")


def parse_table(path: Path, prefix: str) -> dict[str, str]:
    out: dict[str, str] = {}
    if not path.is_file():
        return out
    for raw in path.read_text(encoding="utf-8-sig").splitlines():
        line = raw.strip()
        if not line or line.startswith("//") or line.startswith("#"):
            continue
        m = LINE_RE.match(line)
        if not m:
            continue
        key, value = m.group(1).strip(), m.group(2).strip()
        if not key.startswith(prefix):
            continue
        item_id = key[len(prefix) :]
        if "/" in item_id:
            continue
        if value:
            out[item_id] = value
    return out


def translate_view_name(name: str, lang: str) -> str:
    if not name or name == name.lower() and name.isascii():
        return name
    places = VIEW_PLACE.get(lang) or {}
    times = VIEW_TIME.get(lang) or {}
    if name in places:
        return places[name]
    parts = name.split("_")
    if len(parts) >= 2 and parts[-1] in times:
        head = "_".join(parts[:-1])
        tail = times[parts[-1]]
        head_tr = places.get(head, head)
        return f"{head_tr}_{tail}"
    return places.get(name, name)


def load_system_terms(lang_file: str) -> dict[str, str]:
    table = parse_table(RAW / f"System_{lang_file}.txt", "System/")
    mapped = {}
    mapping = {
        "Menu/LibraryEnd": "endgame_book",
        "Menu/LibraryDead": "death_book",
        "Menu/LibraryAchievement": "achievement",
        "Menu/LibraryLegend": "game_title",
        "Start/Library": "library",
        "Title/Home": "tang_home",
        "Title/Talent": "talent",
        "Title/Book": "book",
        "Title/Misc": "misc",
        "Title/Special": "treasure",
        "Title/Legend": "legend",
        "Title/Social": "social",
        "Title/Training": "training_action",
        "Title/Alchemy": "alchemy",
        "Title/Forge": "forge",
        "Title/Property": "property",
        "Menu/Title": "title_screen",
        "Menu/Submit": "ok",
        "Menu/Cancel": "cancel",
    }
    for src, dst in mapping.items():
        if src in table:
            mapped[dst] = table[src].strip()
    return mapped


def build_lang(unpack_lang: str, locale: str, ja_from_tw: dict | None = None) -> dict:
    data: dict[str, dict[str, str]] = {}
    for cat, (tmpl, prefix) in TABLES.items():
        if cat == "system":
            continue
        table = parse_table(RAW / tmpl.format(lang=unpack_lang), prefix)
        if cat == "free_positions":
            keep = {
                "Mall",
                "Center",
                "Room1",
                "Room2",
                "Alchemy",
                "Forge",
                "Kitchen",
                "Door",
                "BackMountain",
                "Study",
                "Secret",
                "Downtown",
                "Fortress",
                "Teashop",
                "Pharmacy",
                "Farm",
                "Spa",
                "Tower",
            }
            table = {k: v for k, v in table.items() if k in keep}
        if table:
            data[cat] = table
    if locale == "ja" and ja_from_tw:
        # wiki 日文人物页使用与繁中相同的汉字
        data["characters"] = dict(ja_from_tw.get("characters") or {})
        data["talents"] = dict(ja_from_tw.get("talents") or {})
        data["items_book"] = dict(ja_from_tw.get("items_book") or {})
        data["items_misc"] = dict(ja_from_tw.get("items_misc") or {})
        data["items_special"] = dict(ja_from_tw.get("items_special") or {})
        data["enemy_teams"] = dict(ja_from_tw.get("enemy_teams") or {})
        data["battle_skills"] = dict(ja_from_tw.get("battle_skills") or {})
        data["free_positions"] = dict(ja_from_tw.get("free_positions") or {})
        stats = dict(ja_from_tw.get("stats") or {})
        stats.update(WIKI_JA_STATS)
        data["stats"] = stats
        ja_pos = {
            "Mall": "正心堂",
            "Center": "練功場",
            "Room1": "男弟子の部屋",
            "Room2": "女弟子の部屋",
            "Alchemy": "煉丹房",
            "Forge": "鍛冶場",
            "Kitchen": "厨房",
            "Door": "大門",
            "BackMountain": "裏山",
            "Study": "講経堂",
            "Secret": "謎の小屋",
            "Fortress": "外堡",
            "Teashop": "茶屋",
            "Pharmacy": "薬屋",
            "Farm": "義田",
            "Spa": "温泉",
            "Tower": "練功塔",
        }
        data.setdefault("free_positions", {}).update(ja_pos)
    common = dict(WIKI_COMMON[locale])
    sys_map = {
        "chs": "zh-cn",
        "cht": "zh-tw",
        "ko": "kr",
    }
    if locale in sys_map:
        common.update(load_system_terms(sys_map[locale]))
    data["common"] = common

    if EDITOR_DATA.is_file():
        ed = json.loads(EDITOR_DATA.read_text(encoding="utf-8"))
        views = {}
        for entry in ed.get("views") or []:
            vid = entry.get("id") if isinstance(entry, dict) else str(entry)
            name = entry.get("name") if isinstance(entry, dict) else str(entry)
            if not vid:
                continue
            if locale == "cht":
                views[vid] = name
            else:
                views[vid] = translate_view_name(name, locale)
        data["views"] = views

        # 站位：无官方多语言，按规则翻译
        pos = {}
        letters = {
            "chs": {"S": "屏外", "L": "左", "M": "中", "R": "右", "B": "后", "C": "央"},
            "cht": {"S": "屏外", "L": "左", "M": "中", "R": "右", "B": "後", "C": "央"},
            "ja": {"S": "画面外", "L": "左", "M": "中", "R": "右", "B": "後", "C": "中央"},
            "ko": {"S": "화면 밖", "L": "좌", "M": "중", "R": "우", "B": "후", "C": "중앙"},
        }[locale]
        special = {
            "chs": {"Talk": "对话位"},
            "cht": {"Talk": "對話位"},
            "ja": {"Talk": "会話位置"},
            "ko": {"Talk": "대화 위치"},
        }[locale]
        for entry in ed.get("positions") or []:
            pid = entry.get("id") if isinstance(entry, dict) else str(entry)
            if pid in special:
                pos[pid] = special[pid]
                continue
            out = []
            ok = True
            for ch in pid:
                if ch in letters:
                    out.append(letters[ch])
                elif ch.isdigit():
                    out.append(ch)
                else:
                    ok = False
                    break
            pos[pid] = "".join(out) if ok else pid
        data["positions"] = pos
    return data


def main() -> int:
    if not RAW.is_dir():
        print("missing unpack raw:", RAW)
        return 2
    tw = build_lang("zh-tw", "cht")
    catalogs = {
        "chs": build_lang("zh-cn", "chs"),
        "cht": tw,
        "ja": build_lang("zh-tw", "ja", ja_from_tw=tw),
        "ko": build_lang("kr", "ko"),
    }
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for locale, payload in catalogs.items():
        path = OUT_DIR / f"{locale}.json"
        path.write_text(
            json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        counts = {k: len(v) for k, v in payload.items()}
        print(locale, counts, "->", path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
