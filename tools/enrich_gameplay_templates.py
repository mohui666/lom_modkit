#!/usr/bin/env python3
"""Enrich Combat/Battle template IDs from the installed game's serialized assets.

This is a developer extraction tool, not an editor runtime dependency. It only
writes ``data/editor_data.json`` after every referenced template was parsed.
"""

from __future__ import annotations

import argparse
import json
import os
import struct
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EDITOR_DATA = ROOT / "data" / "editor_data.json"


def _settings_game_dir() -> Path | None:
    appdata = os.environ.get("APPDATA")
    if not appdata:
        return None
    path = Path(appdata) / "lom_modkit" / "settings.json"
    try:
        value = json.loads(path.read_text(encoding="utf-8")).get("game_dir")
    except (OSError, ValueError, TypeError):
        return None
    return Path(value) if isinstance(value, str) and value else None


def _align4(offset: int) -> int:
    return (offset + 3) & ~3


def _read_string(raw: bytes, offset: int) -> tuple[str, int]:
    if offset + 4 > len(raw):
        raise ValueError("string length is outside object")
    length = struct.unpack_from("<i", raw, offset)[0]
    if length < 0 or offset + 4 + length > len(raw):
        raise ValueError("invalid serialized string length")
    start = offset + 4
    return raw[start : start + length].decode("utf-8"), _align4(start + length)


def _read_pptr(raw: bytes, offset: int) -> tuple[tuple[int, int], int]:
    if offset + 12 > len(raw):
        raise ValueError("PPtr is outside object")
    return struct.unpack_from("<iq", raw, offset), offset + 12


def _base_name(raw: bytes) -> tuple[str, int]:
    # Unity 2020 MonoBehaviour base: GameObject PPtr + enabled/alignment +
    # MonoScript PPtr, followed by m_Name.
    return _read_string(raw, 28)


def _extract_combat(path: Path) -> dict[str, dict[str, str]]:
    import UnityPy  # Optional developer dependency; intentionally lazy.

    environment = UnityPy.load(str(path))
    result: dict[str, dict[str, str]] = {}
    for obj in environment.objects:
        if obj.type.name != "MonoBehaviour":
            continue
        raw = obj.get_raw_data()
        try:
            name, offset = _base_name(raw)
        except (UnicodeDecodeError, ValueError, struct.error):
            continue
        if not name.startswith("CL_"):
            continue
        description, offset = _read_string(raw, offset)
        _background, offset = _read_pptr(raw, offset)
        offset = _align4(offset + 1)  # _deadEnd bool
        _dead_library, offset = _read_pptr(raw, offset)
        _music, offset = _read_pptr(raw, offset)
        enemy_stat, offset = _read_pptr(raw, offset)
        if enemy_stat[0] != 0:
            raise ValueError("CombatLevel %s EnemyStat is unexpectedly external" % name)
        target = obj.assets_file.objects.get(enemy_stat[1])
        if target is None or target.type.name != "MonoBehaviour":
            raise ValueError("CombatLevel %s has no local EnemyStat" % name)
        stat_raw = target.get_raw_data()
        _asset_name, stat_offset = _base_name(stat_raw)
        character, _stat_offset = _read_string(stat_raw, stat_offset)
        if not character:
            raise ValueError("CombatLevel %s has an empty enemy character" % name)
        result[name[3:]] = {"character": character, "description": description}
    return result


def _extract_battle(path: Path) -> dict[str, dict[str, str]]:
    import UnityPy

    environment = UnityPy.load(str(path))
    result: dict[str, dict[str, str]] = {}
    for obj in environment.objects:
        if obj.type.name != "MonoBehaviour":
            continue
        raw = obj.get_raw_data()
        try:
            name, offset = _base_name(raw)
        except (UnicodeDecodeError, ValueError, struct.error):
            continue
        if not name.startswith("BL_"):
            continue
        name_key, _offset = _read_string(raw, offset)
        if not name_key:
            raise ValueError("BattleLevel %s has an empty NameKey" % name)
        result[name[3:]] = {"name_key": name_key}
    return result


def _ids(items: object) -> list[str]:
    result = []
    for item in items if isinstance(items, list) else []:
        value = item.get("id") if isinstance(item, dict) else item
        if isinstance(value, str) and value:
            result.append(value)
    return result


def enrich(game_dir: Path, output: Path = EDITOR_DATA) -> tuple[int, int]:
    managed_root = game_dir / "Mortal_Data"
    combat = _extract_combat(managed_root / "sharedassets6.assets")
    battle = _extract_battle(managed_root / "sharedassets5.assets")
    data = json.loads(output.read_text(encoding="utf-8"))
    combat_ids = _ids(data.get("combat_ids"))
    battle_ids = _ids(data.get("battle_ids"))
    missing_combat = [key for key in combat_ids if key not in combat]
    missing_battle = [key for key in battle_ids if key not in battle]
    if missing_combat or missing_battle:
        raise ValueError(
            "installed game assets do not contain editor templates: combat=%r battle=%r"
            % (missing_combat[:8], missing_battle[:8])
        )
    data["combat_ids"] = [dict(id=key, **combat[key]) for key in combat_ids]
    data["battle_ids"] = [dict(id=key, **battle[key]) for key in battle_ids]
    output.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=output.name + ".", suffix=".tmp", dir=str(output.parent))
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as stream:
            json.dump(data, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
        os.replace(temporary, output)
    except Exception:
        try:
            os.unlink(temporary)
        except OSError:
            pass
        raise
    return len(combat_ids), len(battle_ids)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--game-dir", type=Path)
    parser.add_argument("--output", type=Path, default=EDITOR_DATA)
    args = parser.parse_args()
    game_dir = args.game_dir or _settings_game_dir()
    if game_dir is None:
        parser.error("game directory not provided and not found in editor settings")
    combat_count, battle_count = enrich(game_dir, args.output)
    print("enriched combat=%d battle=%d -> %s" % (combat_count, battle_count, args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
