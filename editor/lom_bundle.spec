# -*- mode: python ; coding: utf-8 -*-
"""PyInstaller spec：共享 onedir bundle —— GUI lom_editor.exe + CLI story_api_cli.exe。

相对路径以 spec 所在目录（editor/）为基准（构建时 chdir 到 spec 目录）。
两个入口各自独立 Analysis（入口脚本明确、无歧义），共享同一个 COLLECT/_internal
（Python 运行时与 Qt 插件只装一份，体积可控）。

冻结运行路径契约（与 editor/models.py 的 FROZEN 分支对应）：
- data/editor_data.json（+ preview_map.json）→ 包内 _MEIPASS/data/；
  读取失败走 models.FALLBACK_EDITOR_DATA 内置兜底，不崩溃。
- compiler/lomc 经 pathex 打进 PYZ；运行时 import lomc 由冻结导入器解析，
  sys.path 上的 <root>/compiler 在包内不存在也无需存在。
"""

a_gui = Analysis(
    ["main.py"],
    pathex=[".", "../compiler"],
    datas=[
        ("../data/editor_data.json", "data"),
        ("../data/preview_map.json", "data"),
        ("assets/lom_editor_icon.png", "assets"),
        ("assets/combo_arrow.svg", "assets"),
        ("assets/doorstop/win-x86-doorstop.dll", "assets/doorstop"),
        ("../runtime/MortalModHost/bin/Release/net48/MortalModHost.dll", "runtime"),
        ("../runtime/MortalModHost/bin/Release/net48/NVorbis.dll", "runtime"),
        ("i18n/locales", "i18n/locales"),
        ("i18n/terms", "i18n/terms"),
        ("i18n/help", "i18n/help"),
    ],
    hiddenimports=[
        "i18n",
        "i18n.core",
        "lomc",
        "lomc.codegen",
        "lomc.compiler",
        "lomc.dice_data",
        "lomc.errors",
        "lomc.localization",
        "lomc.pack",
        "lomc.validate",
        "lomc.content",
        "lomc.deterministic_zip",
        "lomc.watermark_protocol",
        "content_registry",
        "content_library_dialog",
        "audio_preview",
    ],
)

b_cli = Analysis(
    ["story_api.py"],
    pathex=[".", "../compiler"],
    hiddenimports=[
        "lomc",
        "lomc.codegen",
        "lomc.compiler",
        "lomc.dice_data",
        "lomc.errors",
        "lomc.localization",
        "lomc.pack",
        "lomc.validate",
        "lomc.content",
        "lomc.deterministic_zip",
        "lomc.watermark_protocol",
    ],
)

# Qt 6 on Windows intentionally resolves the operating-system ICU shim
# (icuuc.dll). A developer PATH may also contain Poppler's private ICU build;
# PyInstaller then mistakes that unrelated DLL (and its icudtXX.dll) for a Qt
# dependency. Bundling it makes QtCore fail with WinError 127 on startup. Keep
# the bundle independent of the build machine's PATH and let Windows resolve
# the same ICU shim used by the unfrozen PySide6 installation.
_foreign_icu_names = {"icuuc.dll", "icudt78.dll"}
a_gui.binaries = [
    item for item in a_gui.binaries if item[0].lower() not in _foreign_icu_names
]
b_cli.binaries = [
    item for item in b_cli.binaries if item[0].lower() not in _foreign_icu_names
]

pyz_gui = PYZ(a_gui.pure)
pyz_cli = PYZ(b_cli.pure)

exe_gui = EXE(
    pyz_gui,
    a_gui.scripts,
    exclude_binaries=True,
    name="lom_editor",
    console=False,  # 图形界面：无控制台窗口
    icon="assets/lom_editor.ico",
)

exe_cli = EXE(
    pyz_cli,
    b_cli.scripts,
    exclude_binaries=True,
    name="story_api_cli",
    console=True,  # AI/脚本子进程：需要 stdout/stderr 与退出码
)

coll = COLLECT(
    exe_gui,
    exe_cli,
    a_gui.binaries,
    a_gui.datas,
    b_cli.binaries,
    b_cli.datas,
    name="lom_modkit",
    upx=False,
)
