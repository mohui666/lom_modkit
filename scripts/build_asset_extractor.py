#!/usr/bin/env python3
"""Compatibility build wrapper for the native Rust resource reader (stdlib only)."""
import argparse
from pathlib import Path
import shutil
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    subprocess.run(["cargo", "build", "--locked", "--release", "-p", "lom-assets"], check=True, cwd=root)
    name = "lom-asset-extractor.exe" if sys.platform == "win32" else "lom-asset-extractor"
    output = args.output_dir.resolve() / "lom-asset-extractor"
    output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(root / "target/release" / name, output / name)
    subprocess.run([str(output / name), "--self-check"], check=True)


if __name__ == "__main__":
    main()
