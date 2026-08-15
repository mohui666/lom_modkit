#!/usr/bin/env python3
"""Verify the recorded Gameplay API evidence against an installed game build.

This tool is read-only. It hashes managed assemblies and asks ilspycmd to
decompile only the recorded types, then checks stable method/property fragments.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONTRACT = ROOT / "research" / "gameplay_api_contract.json"


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


def _find_ilspy(explicit: str | None) -> Path | None:
    if explicit:
        return Path(explicit)
    found = shutil.which("ilspycmd")
    if found:
        return Path(found)
    candidate = Path.home() / ".dotnet" / "tools" / "ilspycmd.exe"
    return candidate if candidate.is_file() else None


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


def verify(contract: dict, game_dir: Path, ilspy: Path) -> list[str]:
    errors: list[str] = []
    managed = game_dir / "Mortal_Data" / "Managed"
    for item in contract["assemblies"]:
        path = managed / item["file"]
        if not path.is_file():
            errors.append("missing assembly: %s" % path)
            continue
        if path.stat().st_size != item["size"]:
            errors.append("size mismatch: %s" % item["file"])
        if _sha256(path) != item["sha256"]:
            errors.append("sha256 mismatch: %s" % item["file"])

    cache: dict[tuple[str, str], str] = {}
    for probe in contract["probes"]:
        key = (probe["assembly"], probe["type"])
        if key not in cache:
            path = managed / probe["assembly"]
            if not path.is_file():
                continue
            completed = subprocess.run(
                [str(ilspy), "-t", probe["type"], str(path)],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                check=False,
            )
            if completed.returncode:
                errors.append(
                    "ilspy failed for %s: %s" % (probe["type"], completed.stderr.strip())
                )
                continue
            cache[key] = completed.stdout
        source = cache.get(key, "")
        for fragment in probe["required_fragments"]:
            if fragment not in source:
                errors.append("missing fragment in %s: %s" % (probe["type"], fragment))
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--contract", type=Path, default=DEFAULT_CONTRACT)
    parser.add_argument("--game-dir", type=Path)
    parser.add_argument("--ilspy")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    contract = json.loads(args.contract.read_text(encoding="utf-8"))
    game_dir = args.game_dir or _settings_game_dir()
    ilspy = _find_ilspy(args.ilspy)
    setup_errors = []
    if game_dir is None:
        setup_errors.append("game directory not provided and not found in settings")
    if ilspy is None or not ilspy.is_file():
        setup_errors.append("ilspycmd not found")
    errors = setup_errors or verify(contract, game_dir, ilspy)  # type: ignore[arg-type]
    result = {
        "status": "verified" if not errors else "failed",
        "game_dir": str(game_dir) if game_dir else None,
        "contract": str(args.contract),
        "probe_count": len(contract.get("probes", [])),
        "errors": errors,
    }
    if args.json:
        print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    elif errors:
        print("Gameplay API verification FAILED")
        for error in errors:
            print("- " + error)
    else:
        print("Gameplay API verification PASSED (%d probes)" % result["probe_count"])
    return 0 if not errors else 1


if __name__ == "__main__":
    raise SystemExit(main())
