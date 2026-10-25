#!/usr/bin/env python3
"""Build a self-contained preview helper for the current OS (no game input)."""
import argparse
import importlib.metadata
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    try:
        import UnityPy
        import PyInstaller
    except ImportError as error:
        raise SystemExit("Missing build dependency. Run: python -m pip install -r tools/requirements-preview-extractor.txt") from error
    if UnityPy.__version__ != "1.24.2" or PyInstaller.__version__ != "6.22.3":
        raise SystemExit("Extractor build versions differ; install tools/requirements-preview-extractor.txt")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    build = root / "out/asset-extractor-build"
    build.mkdir(parents=True, exist_ok=True)
    subprocess.run([
        sys.executable, "-m", "PyInstaller", "--noconfirm", "--clean", "--onedir",
        "--name", "lom-asset-extractor", "--distpath", str(output),
        "--workpath", str(build / "work"), "--specpath", str(build),
        "--paths", str(root / "tools"), "--collect-all", "UnityPy",
        # UnityPy.export imports AudioClipConverter eagerly, even for Sprite.image.
        # fmod_toolkit loads this platform's native library via ctypes at import.
        "--collect-all", "fmod_toolkit",
        # Keep archspec's runtime CPU tables, excluding its test fixtures.
        "--additional-hooks-dir", str(root / "scripts/pyinstaller-hooks"),
        "--hidden-import", "archspec",
        "--hidden-import", "PIL.PngImagePlugin", str(root / "tools/preview_asset_worker.py"),
    ], check=True, cwd=root)
    name = "lom-asset-extractor.exe" if sys.platform == "win32" else "lom-asset-extractor"
    subprocess.run([str(output / "lom-asset-extractor" / name), "--help"], check=True)
    subprocess.run([str(output / "lom-asset-extractor" / name), "--self-check"], check=True)
    # Ship the helper's dependency notices, including PyInstaller's bootloader
    # exception. No game data or decompiled source is collected here.
    notices = output / "lom-asset-extractor/third-party-licenses"
    notices.mkdir(exist_ok=True)
    from packaging.requirements import Requirement
    pending = ["UnityPy", "PyInstaller"]
    seen = set()
    while pending:
        package = pending.pop()
        distribution = importlib.metadata.distribution(package)
        canonical = distribution.metadata["Name"]
        if canonical in seen:
            continue
        seen.add(canonical)
        for requirement in distribution.requires or []:
            parsed = Requirement(requirement)
            if parsed.marker is None or parsed.marker.evaluate():
                pending.append(parsed.name)
        for file in distribution.files or []:
            if any(marker in file.name.lower() for marker in ["license", "copying", "copyright"]):
                source = distribution.locate_file(file)
                if source.is_file():
                    target = notices / (canonical + "-" + "-".join(file.parts))
                    target.write_bytes(source.read_bytes())
    (notices / "dependencies.txt").write_text("\n".join(sorted(seen)) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
