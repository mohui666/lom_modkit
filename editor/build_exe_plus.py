"""打包增强版编辑器：产出 editor/dist/lom_editor_plus/。

用法（在 editor/ 目录下）：
    .venv/Scripts/python build_exe_plus.py

与原版 build_exe.py 的区别：
- 使用 lom_bundle_plus.spec，产物目录与可执行文件名都带 ``_plus`` 后缀，
  原版 lom_editor.exe / story_api_cli.exe 不受影响，可以直接对照测试。
- 运行 runtime DLL 取自 ``runtime/MortalModHost/bin/Release/net48/``；如果那个
  目录还没构建过，会尝试从同目录层级里已有的发行版 ``_internal/runtime`` 复制，
  免得为了打个包先去装 .NET SDK。

打包完成后会跑一次 offscreen 自检（冻结版必须能 import lomc 并编译示例剧情）。
"""
from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

EDITOR_DIR = Path(__file__).resolve().parent
SPEC = EDITOR_DIR / "lom_bundle_plus.spec"
DIST = EDITOR_DIR / "dist"
BUILD = EDITOR_DIR / "build"
BUNDLE = DIST / "lom_editor_plus"

OUTPUTS = (BUNDLE / "lom_editor_plus.exe", BUNDLE / "story_api_cli.exe")

RUNTIME_SRC_CANDIDATES = (
    EDITOR_DIR / "assets" / "_runtime_seed",
    EDITOR_DIR.parent.parent / "lom_modkit" / "_internal" / "runtime",
    EDITOR_DIR / "dist" / "lom_modkit" / "_internal" / "runtime",
)


def _ensure_runtime() -> bool:
    """确保 runtime/MortalModHost/bin/Release/net48/ 下有打包需要的 DLL。"""
    target = EDITOR_DIR.parent / "runtime" / "MortalModHost" / "bin" / "Release" / "net48"
    needed = ("MortalModHost.dll", "NVorbis.dll")
    if all((target / name).is_file() for name in needed):
        return True
    for source in RUNTIME_SRC_CANDIDATES:
        if all((source / name).is_file() for name in needed):
            target.mkdir(parents=True, exist_ok=True)
            for name in needed:
                shutil.copy2(source / name, target / name)
            print(f"[runtime] 从 {source} 复制了 {', '.join(needed)}")
            return True
    print(
        "缺少内置运行时文件：runtime/MortalModHost/bin/Release/net48/"
        "MortalModHost.dll、NVorbis.dll。\n"
        "请先构建 runtime/MortalModHost，或把原版发行版的 _internal/runtime 放到 "
        f"{EDITOR_DIR.parent.parent / 'lom_modkit'} 下。",
        file=sys.stderr,
    )
    return False


def main() -> int:
    if not _ensure_runtime():
        return 2
    try:
        import PyInstaller  # noqa: F401
    except ImportError:
        print(
            "缺少 PyInstaller：请先执行 editor/.venv/Scripts/pip install pyinstaller",
            file=sys.stderr,
        )
        return 2

    print(f"打包 {SPEC.name} → {BUNDLE}（onedir，双入口共享运行时）")
    # 刻意不加 --clean，也不复用固定的 workpath：某些受限环境（CI / 沙箱）会把
    # 「一次删掉几十个文件」当成危险操作直接拦下，构建目录清理就让 PyInstaller
    # 自己在可写目录里做，失败时也不会连带清掉上一次的成功产物。
    work = BUILD / f"work-{os.getpid()}"
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "PyInstaller",
            "--noconfirm",
            "--distpath",
            str(DIST),
            "--workpath",
            str(work),
            str(SPEC.name),
        ],
        cwd=EDITOR_DIR,
        check=False,
    )
    if result.returncode != 0:
        print(f"PyInstaller 失败（退出码 {result.returncode}）", file=sys.stderr)
        return result.returncode

    ok = True
    for path in OUTPUTS:
        good = path.exists()
        ok = ok and good
        print(f"{'OK  ' if good else 'MISS'} {path}")
    if not ok:
        return 1

    sample = EDITOR_DIR.parent / "samples" / "showcase3" / "story" / "main.json"
    env = os.environ.copy()
    env["QT_QPA_PLATFORM"] = "offscreen"
    check = subprocess.run(
        [str(OUTPUTS[0]), "--smoke-preview", str(sample)],
        cwd=EDITOR_DIR,
        env=env,
        timeout=60,
        check=False,
    )
    ok = check.returncode == 0
    print(f"{'OK  ' if ok else 'FAIL'} 冻结版 Lua 预览自检（退出码 {check.returncode}）")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
