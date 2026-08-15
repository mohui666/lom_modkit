"""PyInstaller 打包脚本：产出共享 onedir bundle（editor/dist/lom_modkit/）。

用法（在 editor/ 目录下）：
    .venv/Scripts/python build_exe.py

产物：
    dist/lom_modkit/lom_editor.exe     图形编辑器（windowed，无控制台）
    dist/lom_modkit/story_api_cli.exe  AI 友好 CLI（check/compile/pack/new-story，--json）
    dist/lom_modkit/_internal/         共享运行时（Python、Qt 插件、打包数据）

前置：editor/.venv 已装 PySide6；缺 PyInstaller 时先
    .venv/Scripts/pip install pyinstaller
"""
from __future__ import annotations

import subprocess
import sys
import os
from pathlib import Path

EDITOR_DIR = Path(__file__).resolve().parent
SPEC = EDITOR_DIR / "lom_bundle.spec"
DIST = EDITOR_DIR / "dist"
BUILD = EDITOR_DIR / "build"

OUTPUTS = (
    DIST / "lom_modkit" / "lom_editor.exe",
    DIST / "lom_modkit" / "story_api_cli.exe",
)


def main() -> int:
    runtime_dll = (
        EDITOR_DIR.parent
        / "runtime"
        / "MortalModHost"
        / "bin"
        / "Release"
        / "net48"
        / "MortalModHost.dll"
    )
    runtime_dependencies = (runtime_dll.parent / "NVorbis.dll",)
    missing_runtime_files = [
        path for path in (runtime_dll, *runtime_dependencies) if not path.is_file()
    ]
    if missing_runtime_files:
        print(
            "缺少内置运行时文件：%s。请先构建 runtime/MortalModHost"
            % "、".join(str(path) for path in missing_runtime_files),
            file=sys.stderr,
        )
        return 2
    try:
        import PyInstaller  # noqa: F401
    except ImportError:
        print(
            "缺少 PyInstaller：请先执行 editor/.venv/Scripts/pip install pyinstaller",
            file=sys.stderr,
        )
        return 2
    print(f"打包 {SPEC.name} → {DIST / 'lom_modkit'}（onedir，双入口共享运行时）")
    r = subprocess.run(
        [
            sys.executable,
            "-m",
            "PyInstaller",
            "--noconfirm",
            "--distpath",
            str(DIST),
            "--workpath",
            str(BUILD),
            str(SPEC.name),
        ],
        cwd=EDITOR_DIR,
        check=False,
    )
    if r.returncode != 0:
        print(f"PyInstaller 失败（退出码 {r.returncode}）", file=sys.stderr)
        return r.returncode
    ok = True
    for p in OUTPUTS:
        good = p.exists()
        ok = ok and good
        print(f"{'OK  ' if good else 'MISS'} {p}")
    if ok:
        # 冻结产物必须能真正 import 内置 lomc、编译剧情，并找到自动安装所需
        # 的 runtime DLL / 应用图标 / 下拉箭头资源。
        sample = EDITOR_DIR.parent / "samples" / "snack_case" / "story" / "confront.json"
        env = os.environ.copy()
        env["QT_QPA_PLATFORM"] = "offscreen"
        check = subprocess.run(
            [str(OUTPUTS[0]), "--smoke-preview", str(sample)],
            cwd=EDITOR_DIR,
            env=env,
            timeout=30,
            check=False,
        )
        ok = check.returncode == 0
        print(
            f"{'OK  ' if ok else 'FAIL'} 冻结版 Lua 预览自检"
            f"（退出码 {check.returncode}）"
        )
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
