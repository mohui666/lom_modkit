#!/bin/bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
python_bin="${LOM_MODKIT_PYTHON:-python3}"
if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
    echo "此构建脚本需要 Apple Silicon Mac。" >&2
    exit 2
fi
"$python_bin" -c 'import sys; sys.exit("需要 Python 3.12 或更高版本，请设置 LOM_MODKIT_PYTHON。") if sys.version_info < (3, 12) else None'
"$python_bin" -m venv "$repo_dir/editor/.venv"
"$repo_dir/editor/.venv/bin/python" -m pip install -r "$repo_dir/editor/requirements-macos.txt"
"$repo_dir/editor/.venv/bin/python" "$repo_dir/editor/build_exe.py"
