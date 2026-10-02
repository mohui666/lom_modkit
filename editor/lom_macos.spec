# -*- mode: python ; coding: utf-8 -*-
from PyInstaller.utils.hooks import collect_submodules
from app_version import EDITOR_VERSION

shared_data = [
    ("../data/editor_data.json", "data"),
    ("../data/preview_map.json", "data"),
    ("../data/stage_positions.json", "data"),
    ("assets/lom_editor_icon.png", "assets"),
    ("assets/combo_arrow.svg", "assets"),
    ("i18n/locales", "i18n/locales"),
    ("i18n/terms", "i18n/terms"),
    ("i18n/help", "i18n/help"),
]
hidden = collect_submodules("lomc")
a_gui = Analysis(["main.py"], pathex=[".", "../compiler"], datas=shared_data,
                 hiddenimports=hidden + ["AppKit", "Foundation", "objc"])
a_cli = Analysis(["story_api.py"], pathex=[".", "../compiler"], hiddenimports=hidden)
# Editor previews read the owner's external cache; no extracted game images or
# Windows executables are included in this native app.
gui = EXE(PYZ(a_gui.pure), a_gui.scripts, exclude_binaries=True,
          name="lom_editor", console=False, target_arch="arm64")
cli = EXE(PYZ(a_cli.pure), a_cli.scripts, exclude_binaries=True,
          name="story_api_cli", console=True, target_arch="arm64")
bundle = COLLECT(gui, cli, a_gui.binaries, a_gui.datas, a_cli.binaries, a_cli.datas,
                 name="lom_modkit_macos", upx=False)
app = BUNDLE(bundle, name="LoM Modkit.app", icon="assets/lom_editor.icns",
             bundle_identifier="com.mohui666.lom-modkit",
             info_plist={"CFBundleShortVersionString": EDITOR_VERSION,
                         "CFBundleVersion": EDITOR_VERSION,
                         "NSHighResolutionCapable": True,
                         "LSBackgroundOnly": False,
                         "LSUIElement": False,
                         "LSMinimumSystemVersion": "13.0",
                         "NSRequiresAquaSystemAppearance": False})
