#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""从 lom_unpack 解包产物提取编辑器数据，生成 data/editor_data.json。

格式契约见 docs/chs/mod_format.md §5。仅用 Python 标准库。
"""

import csv
import json
import os
import re
import sys

# 当前仓库根目录。输出和随仓数据不能硬编码到某个开发者工作区，否则在
# 独立 worktree/CI 中运行提取器会误写另一份仓库。
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# 解包产物目录：优先环境变量 LOM_UNPACK_DIR，缺省为本机开发路径
UNPACK_DIR = os.environ.get("LOM_UNPACK_DIR", r"C:/Users/mohui666/lom_unpack")
SCRIPTS_DIR = os.path.join(UNPACK_DIR, "raw_scripts")
CSV_CHAR = os.path.join(UNPACK_DIR, "output", "csv", "11_人物.csv")
RAW_CHAR = os.path.join(UNPACK_DIR, "raw", "Character_zh-cn.txt")
RAW_CHAR_TITLE = os.path.join(UNPACK_DIR, "raw", "CharacterTitle_zh-cn.txt")
RAW_CHAR_INTRO = os.path.join(UNPACK_DIR, "raw", "CharacterIntro0_zh-cn.txt")
CSV_STAT = os.path.join(UNPACK_DIR, "output", "csv", "14_属性与Flag.csv")
RAW_STAT = os.path.join(UNPACK_DIR, "raw", "Stat_zh-cn.txt")
CSV_TALENT = os.path.join(UNPACK_DIR, "output", "csv", "08_天赋技能.csv")
RAW_TALENT = os.path.join(UNPACK_DIR, "raw", "Talent_zh-cn.txt")
CSV_BOOK = os.path.join(UNPACK_DIR, "output", "csv", "05_武学秘籍书籍.csv")
RAW_BOOK = os.path.join(UNPACK_DIR, "raw", "ItemBook_zh-cn.txt")
CSV_ITEM = os.path.join(UNPACK_DIR, "output", "csv", "10_物品.csv")
RAW_MISC = os.path.join(UNPACK_DIR, "raw", "ItemMisc_zh-cn.txt")
RAW_SPECIAL = os.path.join(UNPACK_DIR, "raw", "ItemSpecial_zh-cn.txt")
RAW_ENEMY_TEAM = os.path.join(UNPACK_DIR, "raw", "EnemyTeam_zh-cn.txt")
RAW_BATTLE_SKILL = os.path.join(UNPACK_DIR, "raw", "BattleSkill_zh-cn.txt")
CSV_MESSAGE = os.path.join(UNPACK_DIR, "output", "csv", "03_剧情系统提示.csv")
RAW_MESSAGE = os.path.join(UNPACK_DIR, "raw", "Story_Message_zh-cn.txt")
RAW_FLAG = os.path.join(UNPACK_DIR, "raw", "Flag_zh-cn.txt")
RAW_POSITION = os.path.join(UNPACK_DIR, "raw", "Position_zh-cn.txt")
VIEW_MAP_JSON = os.environ.get(
    "LOM_VIEW_MAP_JSON",
    os.path.join(REPO_ROOT, "data", "assets", "_probe", "view_map.json"),
)
# 死亡/结局 id 权威参考（由 lom-save-analyzer 仓库 mappings.js 提取）
REF_IDS_JSON = os.path.join(REPO_ROOT, "data", "ref", "death_ending_ids.json")
OUT_PATH = os.path.join(REPO_ROOT, "data", "editor_data.json")

# 契约 §5 固定值
MODES = ["character", "think", "narrative", "center"]

# 自由模式地图地点（Mortal.Core PositionType 枚举，顺序按 Position 文本表）
FREE_POSITION_TYPES = [
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
]

# 舞台站位字母代码 -> 中文（无官方中文名，规则化标注；数字=纵深层级）
_POSITION_LETTERS = {"S": "屏外", "L": "左", "M": "中", "R": "右", "B": "后", "C": "央"}
_POSITION_SPECIAL = {"Talk": "对话位"}


def position_label(pid):
    """站位代码 -> 规则化中文标注，如 SL->屏外左、RM2->右中2；无法解析的原样返回。"""
    if pid in _POSITION_SPECIAL:
        return _POSITION_SPECIAL[pid]
    out = []
    for ch in pid:
        if ch in _POSITION_LETTERS:
            out.append(_POSITION_LETTERS[ch])
        elif ch.isdigit():
            out.append(ch)
        else:
            return pid
    return "".join(out)


RE_CHARACTER = re.compile(r'characters\.Get\("([^"]+)"\)')
RE_PORTRAIT = re.compile(r'GetPortrait\("([^"]+)",\s*"([^"]+)"\)')
RE_VIEW = re.compile(r'getvar\(flowcharts\.view,\s*"ViewName"\)\.value\s*=\s*"([^"]+)"')
RE_MUSIC = re.compile(r'luamanager\.PlayMusic\("([^"]+)"\)')
RE_SOUND = re.compile(r'luamanager\.PlaySound\("([^"]+)"\)')
RE_ENV_SOUND = re.compile(r'luamanager\.PlayEnvSound\("([^"]+)"\)')
RE_STAT = re.compile(r'statmodifymanager\.Player\("([^"]+)"')
RE_AFFINITY = re.compile(r'statmodifymanager\.Character\("([^"]+)"')
RE_STAGE_SHOW = re.compile(r"stage\.show\{(.*?)\}", re.S)
RE_FROM_POS = re.compile(r'fromPosition\s*=\s*"([^"]+)"')
RE_TO_POS = re.compile(r'toPosition\s*=\s*"([^"]+)"')
RE_MENU_DIALOG = re.compile(r"setmenudialog\(menudialogs\.([A-Za-z0-9_]+)")
RE_EFFECT = re.compile(r'effects\.SetupEffect\("([^"]+)"')
RE_DICE = re.compile(r'checkpointmanager\.(?:Dice|Switch)\("([^"]+)"')
RE_COMBAT = re.compile(r'ChangeScene\("Combat",\s*"([^"]+)"')
RE_BATTLE = re.compile(r'ChangeScene\("Battle",\s*"([^"]+)"')
# 死亡画面 id（GameOver）与结局 id（End/endgamepanel）分开提取（契约 §5）
RE_DEATH = re.compile(r'ChangeScene\("GameOver",\s*"([^"]+)"')
RE_ENDING = re.compile(r'ChangeScene\("End",\s*"([^"]+)"|endgamepanel\.Open\("([^"]+)"')
RE_FLAG = re.compile(r'statmodifymanager\.(?:SetFlag|AddFlag)\("([^"]+)"')
RE_TALENT = re.compile(r'AddTalent\("([^"]+)"')
RE_BOOK = re.compile(r'AddBook\("([^"]+)"')
RE_MISC = re.compile(r'AddMisc\("([^"]+)"')
RE_SPECIAL = re.compile(r'AddSpecial\("([^"]+)"')
RE_MESSAGE = re.compile(r'mainui\.DisplayMessage\("([^"]+)"')


RE_DICE_CALL = re.compile(r'checkpointmanager\.Dice\("([A-Za-z0-9_]+)"')
RE_DICE_MAX = re.compile(r"math\.random\((\d+)\)|SetRandom\((\d+),")
RE_DICE_BAND = re.compile(r'(\w+)\[(\d+)\]\s*=\s*"([^"]*)\|([^"]*)"')


def _to_int(text):
    """regex \d+ 捕获组转 int（防御性：非法时返回 0，不抛）。"""
    try:
        return int(text)
    except (TypeError, ValueError):
        return 0


def collect_travel_scripts(files):
    """扫出旅行脚本集合（骰子元数据提取必须排除），返回 {脚本名（不含 .lua.txt）}。

    旅行系统的检查点（Travel_*）只存在于旅行系统配置里，故事场景的
    CheckPointManager 查不到（用户实测骰子菜单崩溃）。旅行脚本由两条规则识别：
    1. 文件名：travel_ 前缀或 _travel 子串（travel_result_* 等旅行系统配置脚本）；
    2. 引用：被任意脚本 SetCurrentTravelScript("X") / SetTempScript(..., "X")
       引用的脚本（如 S0208_04_01_02、S2501_01_travel、section_01_free_start_01），
       这些脚本由旅行系统注册运行，检查点同样进旅行配置而非故事场景。
    """
    re_cur = re.compile(r"SetCurrentTravelScript\(")
    re_ref = re.compile(
        r'SetCurrentTravelScript\("([^"]+)"\)|SetTempScript\([^,]+,\s*"([^"]+)"\)'
    )
    referred = set()
    for fn in files:
        try:
            with open(
                os.path.join(SCRIPTS_DIR, fn), "r", encoding="utf-8", errors="replace"
            ) as f:
                text = f.read()
        except OSError:
            continue
        if not re_cur.search(text) and "SetTempScript" not in text:
            continue  # 无旅行引用，跳过正则扫描
        for a, b in re_ref.findall(text):
            referred.add(a or b)
    return {
        fn[: -len(".lua.txt")]
        for fn in files
        if fn[: -len(".lua.txt")].lower().startswith("travel_")
        or "_travel" in fn[: -len(".lua.txt")].lower()
        or fn[: -len(".lua.txt")] in referred
    }


def extract_dice_meta(text, travel=False):
    """从一个官方脚本里提取骰子检查点元数据：{check: {max, bands}}。

    每个 checkpointmanager.Dice("X", ...) 调用点：向前 12 行找骰子范围
    （math.random(N) / SetRandom(N, ...)），向后到 ExecuteRoll 收集结果带
    （dice_xxx[i] = "文本|条件"）。同检查点多次出现时保留首次提取的结果。

    travel=True 时整个脚本的 dice 调用点都不提取：旅行检查点只存在于旅行系统
    配置，故事场景查不到（骰子菜单会崩），见 collect_travel_scripts。
    """
    if travel:
        return {}
    meta = {}
    lines = text.splitlines()
    for i, line in enumerate(lines):
        m = RE_DICE_CALL.search(line)
        if not m:
            continue
        check = m.group(1)
        if check in meta:
            continue
        # 骰子范围：向前找最近的 math.random/SetRandom
        dice_max = None
        for back in lines[max(0, i - 12) : i + 1][::-1]:
            mm = RE_DICE_MAX.search(back)
            if mm:
                dice_max = _to_int(mm.group(1) or mm.group(2))
                break
        if dice_max is None:
            continue
        # 结果带：向后到 ExecuteRoll 之前的 dice_xxx[n] = "文本|条件"
        bands = []
        for fwd in lines[i + 1 : i + 20]:
            if "ExecuteRoll" in fwd:
                break
            bm = RE_DICE_BAND.search(fwd)
            if bm:
                bands.append((_to_int(bm.group(2)), bm.group(3), bm.group(4)))
        bands.sort()
        # 索引必须从 1 连续递增（官方均如此），否则视为解析失败
        if bands and [b[0] for b in bands] == list(range(1, len(bands) + 1)):
            meta[check] = {
                "max": dice_max,
                "bands": [{"text": t, "cond": c} for _i, t, c in bands],
            }
    return meta


def _load_prefixed_kv(csv_path, raw_path, prefix):
    """加载 <prefix>/<id> 形式的中文名表：优先 CSV（utf-8-sig，第 3 列简体），退回 raw key=value。"""
    names = {}
    if os.path.isfile(csv_path):
        try:
            with open(csv_path, "r", encoding="utf-8-sig", newline="") as f:
                for row in csv.reader(f):
                    if len(row) < 3 or not row[0].startswith(prefix + "/"):
                        continue
                    cid, name = row[0][len(prefix) + 1 :], row[2].strip()
                    if cid and name:
                        names.setdefault(cid, name)
        except OSError:
            pass
        if names:
            return names
    if os.path.isfile(raw_path):
        try:
            with open(raw_path, "r", encoding="utf-8-sig") as f:
                for line in f:
                    if not line.startswith(prefix + "/") or "=" not in line:
                        continue
                    key, _, val = line.partition("=")
                    cid = key.strip()[len(prefix) + 1 :]
                    name = val.strip()
                    if cid and name:
                        names.setdefault(cid, name)
        except OSError:
            pass
    return names


def load_names():
    """人物显示名：key 格式 Character/<id>。"""
    return _load_prefixed_kv(CSV_CHAR, RAW_CHAR, "Character")


def load_character_intro_data():
    """原版人物介绍卡文本：CharacterTitle/<id> + CharacterIntro0/<id>。

    CharacterIntroPanel.TransitionIntro 已反编译验证只读取 Intro0；Intro1~3 是
    其它关系资料页内容，不能拿来冒充首次人物介绍卡。
    """
    titles = _load_prefixed_kv(CSV_CHAR, RAW_CHAR_TITLE, "CharacterTitle")
    intros = _load_prefixed_kv(CSV_CHAR, RAW_CHAR_INTRO, "CharacterIntro0")
    return titles, {
        key: value.replace("\\n", "\n") for key, value in intros.items()
    }


def load_stat_names():
    """属性显示名：key 格式 PlayerStat/<id>。"""
    return _load_prefixed_kv(CSV_STAT, RAW_STAT, "PlayerStat")


def load_free_position_names():
    """自由模式地点中文名：raw/Position_zh-cn.txt，key 格式 Position/Name/<Type>。"""
    names = {}
    if os.path.isfile(RAW_POSITION):
        try:
            with open(RAW_POSITION, "r", encoding="utf-8-sig") as f:
                for line in f:
                    if not line.startswith("Position/Name/") or "=" not in line:
                        continue
                    key, _, val = line.partition("=")
                    pid = key.strip()[len("Position/Name/") :]
                    name = val.strip()
                    if pid and name:
                        names.setdefault(pid, name)
        except OSError:
            pass
    return names


def load_view_names():
    """场景显示名：探测产物 view_map.json 的 name 字段（游戏内 m_Name）。"""
    if os.path.isfile(VIEW_MAP_JSON):
        try:
            with open(VIEW_MAP_JSON, encoding="utf-8") as f:
                vm = json.load(f)
            return {k: v["name"] for k, v in vm.items() if v.get("name")}
        except (OSError, ValueError):
            pass
    # 发布源码通常不带体积较大的探测目录。重新提取其他 catalog 时，保留
    # 已提交 editor_data 里的原版场景名称，不能把可读中文静默退化为内部 id。
    if os.path.isfile(OUT_PATH):
        try:
            with open(OUT_PATH, encoding="utf-8") as f:
                current = json.load(f)
            return {
                str(item.get("id")): str(item.get("name"))
                for item in current.get("views", [])
                if isinstance(item, dict) and item.get("id") and item.get("name")
            }
        except (OSError, ValueError, TypeError):
            pass
    return {}


def load_ref_ids():
    """读死亡/结局 id 权威参考（data/ref/death_ending_ids.json）：

    {death: {id: {name, desc}}, ending: {...}, epilogue: {...}}，由
    lom-save-analyzer 仓库 mappings.js（LIBRARY_NAMES/DESCS）提取。
    """
    if not os.path.isfile(REF_IDS_JSON):
        return {}
    try:
        with open(REF_IDS_JSON, "r", encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def enrich_id_list(ids, ref_table):
    """裸 id 列表 → [{id, name}] 对象数组（name 取参考文件标题，无则回退 id）。"""
    return [{"id": i, "name": ref_table.get(i, {}).get("name", i)} for i in sorted(ids)]


def main():
    names = load_names()
    character_titles, character_intros = load_character_intro_data()
    stat_names = load_stat_names()
    view_names = load_view_names()
    free_pos_names = load_free_position_names()
    ref_ids = load_ref_ids()  # 死亡/结局 id 标题参考（death/ending/epilogue）
    flag_names = _load_prefixed_kv(CSV_STAT, RAW_FLAG, "Flag/Name")
    talent_names = _load_prefixed_kv(CSV_TALENT, RAW_TALENT, "PlayerTalent/Name")
    book_names = _load_prefixed_kv(CSV_BOOK, RAW_BOOK, "Book/Name")
    misc_names = _load_prefixed_kv(CSV_ITEM, RAW_MISC, "Misc/Name")
    special_names = _load_prefixed_kv(CSV_ITEM, RAW_SPECIAL, "Special/Name")
    enemy_team_names = _load_prefixed_kv("", RAW_ENEMY_TEAM, "EnemyTeam")
    battle_skill_names = _load_prefixed_kv("", RAW_BATTLE_SKILL, "BattleSkill/Name")
    message_names = _load_prefixed_kv(CSV_MESSAGE, RAW_MESSAGE, "Story")

    char_ids = set()
    portraits = {}  # id -> set
    views = set()
    music = set()
    sounds = set()
    env_sounds = set()
    positions = set()
    stats = set()
    affinity_chars = set()
    menu_dialogs = set()
    effects_found = set()
    dice_checks = set()
    dice_meta = {}
    combat_ids = set()
    battle_ids = set()
    death_ids = set()
    ending_ids = set()
    game_flags = set()
    talents = set()
    items_book = set()
    items_misc = set()
    items_special = set()
    messages = set()

    try:
        files = sorted(fn for fn in os.listdir(SCRIPTS_DIR) if fn.endswith(".lua.txt"))
    except OSError as e:
        print("无法读取脚本目录 %s: %s" % (SCRIPTS_DIR, e), file=sys.stderr)
        return 1
    # 旅行脚本集合：骰子检查点元数据只提取故事场景可用的（旅行检查点剔除）
    travel_scripts = collect_travel_scripts(files)
    for fn in files:
        path = os.path.join(SCRIPTS_DIR, fn)
        base = fn[: -len(".lua.txt")]
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            continue

        char_ids.update(RE_CHARACTER.findall(text))
        for cid, port in RE_PORTRAIT.findall(text):
            portraits.setdefault(cid, set()).add(port)
        views.update(RE_VIEW.findall(text))
        music.update(RE_MUSIC.findall(text))
        sounds.update(RE_SOUND.findall(text))
        env_sounds.update(RE_ENV_SOUND.findall(text))
        stats.update(RE_STAT.findall(text))
        affinity_chars.update(RE_AFFINITY.findall(text))
        for block in RE_STAGE_SHOW.findall(text):
            positions.update(RE_FROM_POS.findall(block))
            positions.update(RE_TO_POS.findall(block))
        menu_dialogs.update(RE_MENU_DIALOG.findall(text))
        effects_found.update(RE_EFFECT.findall(text))
        dice_checks.update(RE_DICE.findall(text))  # 全名清单保留（含旅行检查点）
        for check, meta in extract_dice_meta(
            text, travel=base in travel_scripts
        ).items():
            dice_meta.setdefault(check, meta)
        combat_ids.update(RE_COMBAT.findall(text))
        battle_ids.update(RE_BATTLE.findall(text))
        death_ids.update(RE_DEATH.findall(text))
        for a, b in RE_ENDING.findall(text):
            ending_ids.add(a or b)
        game_flags.update(RE_FLAG.findall(text))
        talents.update(RE_TALENT.findall(text))
        items_book.update(RE_BOOK.findall(text))
        items_misc.update(RE_MISC.findall(text))
        items_special.update(RE_SPECIAL.findall(text))
        messages.update(RE_MESSAGE.findall(text))

    # 表情只在 stage.show / GetPortrait 出现的人物也收进 characters
    char_ids.update(portraits.keys())

    characters = [
        {
            "id": cid,
            "name": names.get(cid, cid),
            "portraits": sorted(portraits.get(cid, ())),
            **(
                {"title": character_titles.get(cid, ""), "intro": character_intros[cid]}
                if cid in character_intros
                else {}
            ),
        }
        for cid in sorted(char_ids)
    ]

    data = {
        "schema": 3,
        "characters": characters,
        "views": [{"id": v, "name": view_names.get(v, v)} for v in sorted(views)],
        "music": [
            {"id": m, "name": m} for m in sorted(music)
        ],  # 音乐 id 本身已是中文名
        "sounds": [
            {"id": sound, "name": sound} for sound in sorted(sounds)
        ],  # 普通音效 id 来自原版 PlaySound 调用
        "env_sounds": [
            {"id": sound, "name": sound} for sound in sorted(env_sounds)
        ],  # 环境音 id 来自原版 PlayEnvSound 调用
        "positions": [{"id": p, "name": position_label(p)} for p in sorted(positions)],
        "stats": [{"id": s, "name": stat_names.get(s, s)} for s in sorted(stats)],
        "modes": MODES,
        "menu_dialogs": sorted(menu_dialogs),
        "effects": [
            {"id": e, "name": e} for e in sorted(effects_found)
        ],  # 特效 id 即资源名
        "dice_checks": sorted(dice_checks),
        "dice_meta": dice_meta,
        "combat_ids": sorted(combat_ids),
        "battle_ids": sorted(battle_ids),
        # StatModifyManager.ModifyEnemy* 的 id 来自原版 BattleTeamStat，不能让
        # 作者猜 001/201/500 这类裸编号。全量取官方 EnemyTeam 本地化表。
        "enemy_teams": [
            {"id": i, "name": enemy_team_names[i]} for i in sorted(enemy_team_names)
        ],
        # SetPlayerBattleSkill / SetBattleSkillActive 使用的原版技能 key。
        "battle_skills": [
            {"id": i, "name": battle_skill_names[i]} for i in sorted(battle_skill_names)
        ],
        "death_ids": enrich_id_list(death_ids, ref_ids.get("death", {})),
        "ending_ids": enrich_id_list(ending_ids, ref_ids.get("ending", {})),
        "game_flags": [
            {"id": g, "name": flag_names.get(g, g)} for g in sorted(game_flags)
        ],
        "talents": [{"id": t, "name": talent_names.get(t, t)} for t in sorted(talents)],
        "items_book": [
            {"id": i, "name": book_names.get(i, i)} for i in sorted(items_book)
        ],
        "items_misc": [
            {"id": i, "name": misc_names.get(i, i)} for i in sorted(items_misc)
        ],
        "items_special": [
            {"id": i, "name": special_names.get(i, i)} for i in sorted(items_special)
        ],
        "messages": [
            {"id": m, "name": message_names.get(m, m)} for m in sorted(messages)
        ],
        "affinity_characters": sorted(affinity_chars),
        "free_positions": [
            {"id": p, "name": free_pos_names.get(p, p)} for p in FREE_POSITION_TYPES
        ],
    }

    try:
        os.makedirs(os.path.dirname(OUT_PATH), exist_ok=True)
        with open(OUT_PATH, "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False, indent=2)
    except OSError as e:
        print("无法写入输出文件 %s: %s" % (OUT_PATH, e), file=sys.stderr)
        return 1

    hit = lambda ids, table: "%d（名称命中 %d）" % (
        len(ids),
        sum(1 for i in ids if i in table),
    )
    print("脚本数: %d" % len(files))
    print("旅行脚本数（骰子元数据剔除）: %d" % len(travel_scripts))
    print("人物数: %d" % len(characters))
    print("人物介绍卡文本数: %d" % len(character_intros))
    print("场景数: %s" % hit(views, view_names))
    print("音乐数: %d" % len(music))
    print("普通音效数: %d" % len(sounds))
    print("环境音效数: %d" % len(env_sounds))
    print("站位数: %d" % len(positions))
    print("属性数: %s" % hit(stats, stat_names))
    print("菜单对话框数: %d" % len(menu_dialogs))
    print("特效数: %d" % len(effects_found))
    print("骰子检查点数: %d（含元数据 %d）" % (len(dice_checks), len(dice_meta)))
    print(
        "Combat ids: %d, Battle ids: %d, Death ids: %d, Ending ids: %d"
        % (len(combat_ids), len(battle_ids), len(death_ids), len(ending_ids))
    )
    death_names = sum(
        1
        for i in enrich_id_list(death_ids, ref_ids.get("death", {}))
        if i["name"] != i["id"]
    )
    ending_names = sum(
        1
        for i in enrich_id_list(ending_ids, ref_ids.get("ending", {}))
        if i["name"] != i["id"]
    )
    print(
        "死亡/结局 id 标题命中: %d/%d, %d/%d"
        % (death_names, len(death_ids), ending_names, len(ending_ids))
    )
    print("游戏 Flag 数: %s" % hit(game_flags, flag_names))
    print("天赋数: %s" % hit(talents, talent_names))
    print(
        "书籍: %s, 杂物: %s, 特殊物品: %s"
        % (
            hit(items_book, book_names),
            hit(items_misc, misc_names),
            hit(items_special, special_names),
        )
    )
    print("系统消息数: %s" % hit(messages, message_names))
    print("好感度目标人物数: %d" % len(affinity_chars))
    print("自由模式地点数: %s" % hit(FREE_POSITION_TYPES, free_pos_names))
    print(
        "人物名称命中: %d / %d"
        % (sum(1 for c in characters if c["name"] != c["id"]), len(characters))
    )
    print("输出: %s" % OUT_PATH)


if __name__ == "__main__":
    sys.exit(main())
