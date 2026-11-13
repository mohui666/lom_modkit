#!/usr/bin/env python3
"""Read the complete registered flag catalog; write only editor metadata.

FlagData's live fields are m_Name, _devNote, _default, State. No flag state,
game files or save files are changed. Run with the existing UnityPy environment.
"""
import argparse
import json
import re
import struct
from pathlib import Path

from extract_editor_data import CSV_STAT, RAW_FLAG, _load_prefixed_kv
from vanilla_story_reader import _string, resources, script_text

ROOT = Path(__file__).resolve().parents[1]
VALUE_LINE = re.compile(r"^\s*[=＝]?\s*(-?\d+)(?:\s*([:：=＝、.,，．])\s*|\s+)(\S.*?)\s*$")
FLAG_CALL = re.compile(
    r"(?:statmodifymanager\s*\.\s*(?:SetFlag|AddFlag)|luamanager\s*\.\s*GetFlagData)"
    r"\s*\(\s*['\"]([^'\"]+)['\"]"
)
FRIENDLY_TITLES = {"M20008": "留學去向", "M20011": "崆峒所選門派"}


def value_options(note):
    """Only explicit numbered meanings become suggestions, never constraints.

    Decimal-looking lines and conflicting descriptions are ambiguous. Keep the
    complete note available instead of turning them into invented choices.
    """
    values = {}
    for line in note.splitlines():
        match = VALUE_LINE.fullmatch(line)
        if not match:
            continue
        number, separator, label = match.groups()
        if separator and separator in ".．" and label[0].isdigit():
            continue
        number = int(number)
        if not -(2**31) <= number < 2**31:
            continue
        label = label.strip()
        if number in values and values[number] != label:
            return []
        values[number] = label
    return [{"value": value, "name": name} for value, name in sorted(values.items())]


def direct_references(entries):
    refs = {}
    prefix = "story/chinesetraditional/"  # LuaManager.ExecuteLuaScript.
    for key, pointer in entries.items():
        if not key.startswith(prefix):
            continue
        source = script_text(pointer)
        source = re.sub(r"--\[(=*)\[.*?\]\1\]", "", source, flags=re.S)
        source = re.sub(r"--[^\r\n]*", "", source)
        script = pointer.read().m_Name
        for flag in set(FLAG_CALL.findall(source)):
            refs.setdefault(flag, set()).add(script)
    return {flag: sorted(scripts) for flag, scripts in refs.items()}


def enrich(row, note, default, official_name, scripts):
    row = {"id": row["id"]}
    note = note.replace("\r\n", "\n").replace("\r", "\n").strip()
    title = note.splitlines()[0].strip() if note else row["id"]
    row["name"] = official_name or FRIENDLY_TITLES.get(row["id"], title)
    row["default"] = default
    if note:
        row["description"] = note
        choices = value_options(note)
        if choices:
            row["values"] = choices
        if any(word in note for word in ("暫存", "暂存", "重複使用", "重复使用")):
            row["temporary"] = True
    if scripts:
        row["scripts"] = scripts
    return row


def registered_flags(game):
    """Follow MissionManagerData.FlagsCollection, exactly as GetFlag does."""
    import UnityPy
    from UnityPy.classes import PPtr

    assets = UnityPy.load(str(game / "Mortal_Data/globalgamemanagers.assets"))
    managers = []
    for obj in assets.objects:
        if obj.type.name != "MonoBehaviour":
            continue
        header = obj.read(check_read=False)
        script = header.m_Script.read()
        if script.m_ClassName == "MissionManagerData" and script.m_Namespace == "Mortal.Core":
            managers.append(obj)
    if len(managers) != 1:
        raise ValueError("无法确认唯一的原版 MissionManagerData")

    def pointer_list(owner, at):
        raw = owner.get_raw_data()
        count = struct.unpack_from("<i", raw, at)[0]
        at += 4
        end = at + count * 12
        if count < 0 or end > len(raw):
            raise ValueError("原版旗标集合字段布局已改变")
        pointers = []
        for offset in range(at, end, 12):
            file_id, path_id = struct.unpack_from("<iq", raw, offset)
            if not path_id:
                raise ValueError("原版旗标集合含空引用")
            pointers.append(PPtr(m_FileID=file_id, m_PathID=path_id,
                                 assetsfile=owner.assets_file).deref())
        return pointers, end

    manager = managers[0]
    _, at = _string(manager.get_raw_data(), 28)
    # Live MissionManagerData: player stat, main mission, sub-missions, then flags.
    collections, _ = pointer_list(manager, at + 3 * 12)
    flags = {}
    for collection in collections:
        _, at = _string(collection.get_raw_data(), 28)
        entries, end = pointer_list(collection, at)
        if end != len(collection.get_raw_data()):
            raise ValueError("FlagCollectionData.List 字段布局已改变")
        for obj in entries:
            header = obj.read(check_read=False)
            script = header.m_Script.read()
            if script.m_ClassName != "FlagData" or script.m_Namespace != "Mortal.Core":
                raise ValueError("原版旗标集合引用了非 FlagData 对象")
            raw = obj.get_raw_data()
            flag, at = _string(raw, 28)
            note, at = _string(raw, at)
            if at + 8 != len(raw):
                raise ValueError(f"FlagData 字段布局已改变：{flag}")
            default, _ = struct.unpack_from("<ii", raw, at)
            if flag in flags and flags[flag] != (note, default):
                raise ValueError(f"原版旗标编号重复且内容冲突：{flag}")
            flags[flag] = (note, default)
    return flags


def refresh(game, output=ROOT / "data/editor_data.json"):
    document = json.loads(output.read_text(encoding="utf-8"))
    rows = {row["id"]: row for row in document["game_flags"]}
    flags = registered_flags(game)
    missing = sorted(rows.keys() - flags.keys())
    if missing:
        raise ValueError("当前游戏缺少目录中的旗标：" + ", ".join(missing))
    _, entries = resources(game)
    references = direct_references(entries)
    official = _load_prefixed_kv(CSV_STAT, RAW_FLAG, "Flag/Name")
    document["game_flags"] = [
        enrich({"id": flag}, *flags[flag], official.get(flag), references.get(flag, []))
        for flag in sorted(flags)
    ]
    output.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return {
        "flags": len(flags),
        "added": len(flags.keys() - rows.keys()),
        "described": sum("description" in row for row in document["game_flags"]),
        "value_choices": sum("values" in row for row in document["game_flags"]),
        "temporary": sum(row.get("temporary", False) for row in document["game_flags"]),
        "with_references": sum("scripts" in row for row in document["game_flags"]),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--game-dir", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(refresh(args.game_dir), ensure_ascii=False))
