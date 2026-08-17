#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Extract the authoritative Combat talent contract from an installed game.

This deliberately reads serialized ``PlayerTalentData`` and
``CombatStateEffectDatabase`` assets instead of inferring Combat talents from
Story ``AddTalent`` calls or localized description text.  The output is a
small, deterministic snapshot consumed by ``extract_editor_data.py``.

Optional extractor dependencies::

    python -m pip install UnityPy TypeTreeGeneratorAPI
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


SCHEMA = 1
UNITY_VERSION = "2020.3.49f1"


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while True:
            chunk = stream.read(1024 * 1024)
            if not chunk:
                return digest.hexdigest()
            digest.update(chunk)


def _game_asset_paths(data_dir: Path) -> list[Path]:
    paths = []
    for path in data_dir.iterdir():
        if not path.is_file() or path.suffix == ".resS":
            continue
        if path.suffix == ".assets" or path.name.startswith("level"):
            paths.append(path)
    return sorted(paths, key=lambda item: item.name.casefold())


def extract(game_root: Path) -> dict:
    try:
        import UnityPy
        from UnityPy.helpers.TypeTreeGenerator import TypeTreeGenerator
    except ImportError as exc:
        raise RuntimeError(
            "缺少可选提取依赖；请安装 UnityPy 与 TypeTreeGeneratorAPI"
        ) from exc

    game_root = game_root.resolve()
    data_dir = game_root / "Mortal_Data"
    managed = data_dir / "Managed"
    global_assets = data_dir / "globalgamemanagers.assets"
    combat_assets = data_dir / "sharedassets6.assets"
    required = (
        managed / "Mortal.Core.dll",
        managed / "Mortal.Combat.dll",
        global_assets,
        combat_assets,
    )
    missing = [str(path) for path in required if not path.is_file()]
    if missing:
        raise RuntimeError("游戏数据不完整：" + "、".join(missing))

    paths = _game_asset_paths(data_dir)
    environment = UnityPy.load(*(str(path) for path in paths))
    generator = TypeTreeGenerator(UNITY_VERSION)
    generator.load_local_game(str(game_root))
    environment.typetree_generator = generator

    skills: list[dict] = []
    effect_keys: set[str] = set()
    for obj in environment.objects:
        if obj.type.name != "MonoBehaviour":
            continue
        try:
            head = obj.parse_monobehaviour_head()
            script = head.m_Script.deref_parse_as_object()
            class_name = script.m_ClassName
            if class_name == "PlayerTalentData":
                data = obj.read_typetree()
                if not data.get("_combatSkill"):
                    continue
                skill_id = str(data.get("_id") or "")
                effect_key = str(data.get("_combatSkillKey") or "")
                max_level = int(data.get("_maxLevel") or 0)
                display_level = bool(data.get("_displayLevel"))
                if not skill_id or not effect_key or max_level < 1:
                    raise RuntimeError(
                        "CombatSkill 资产缺少 id/effect/max_level：" + repr(data)
                    )
                skills.append(
                    {
                        "id": skill_id,
                        "effect_key": effect_key,
                        "max_level": max_level,
                        "display_level": display_level,
                        "priority": int(data.get("_priority") or 0),
                    }
                )
            elif class_name == "CombatStateEffectScriptable":
                key = str(obj.read_typetree().get("_key") or "")
                if key:
                    effect_keys.add(key)
        except RuntimeError:
            raise
        except Exception:
            # Most MonoBehaviours are unrelated.  UnityPy can legitimately be
            # unable to generate third-party editor-only component trees.
            continue

    if not skills or not effect_keys:
        raise RuntimeError("未找到 PlayerTalentData / Combat Effect 资产")

    seen: set[str] = set()
    output_skills = []
    for skill in sorted(skills, key=lambda item: item["id"]):
        skill_id = skill["id"]
        if skill_id in seen:
            raise RuntimeError("重复 CombatSkill id：" + skill_id)
        seen.add(skill_id)
        if skill["display_level"]:
            effects = [
                {"level": level, "key": skill["effect_key"] + "_" + str(level)}
                for level in range(1, skill["max_level"] + 1)
            ]
        else:
            effects = [{"level": 1, "key": skill["effect_key"]}]
        missing_effects = [item["key"] for item in effects if item["key"] not in effect_keys]
        if missing_effects:
            raise RuntimeError(
                "CombatSkill 没有有效 Effect：%s -> %s"
                % (skill_id, ", ".join(missing_effects))
            )
        output_skills.append({**skill, "effects": effects})

    return {
        "schema": SCHEMA,
        "unity_version": UNITY_VERSION,
        "source": {
            "globalgamemanagers.assets.sha256": _sha256(global_assets),
            "sharedassets6.assets.sha256": _sha256(combat_assets),
            "Mortal.Core.dll.sha256": _sha256(managed / "Mortal.Core.dll"),
            "Mortal.Combat.dll.sha256": _sha256(managed / "Mortal.Combat.dll"),
        },
        "effect_database_count": len(effect_keys),
        "skills": output_skills,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("game_root", help="《活侠传》目录（包含 Mortal_Data）")
    parser.add_argument(
        "-o", "--output", default="data/ref/combat_skills.json",
        help="输出快照路径",
    )
    args = parser.parse_args(argv)
    payload = extract(Path(args.game_root))
    target = Path(args.output)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(
        "CombatSkill: %d；EffectDatabase: %d；%s"
        % (len(payload["skills"]), payload["effect_database_count"], target)
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
