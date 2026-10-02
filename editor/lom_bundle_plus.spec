# -*- mode: python ; coding: utf-8 -*-
"""PyInstaller spec（增强版）：产出 lom_editor_plus.exe + story_api_cli.exe。

与原版 lom_bundle.spec 的差别，只有两点：

1. 产物名改为 ``lom_editor_plus`` / ``lom_editor_plus.exe``，和原版并存互不覆盖，
   方便把原版当对照组。
2. 额外打进 ``UnityPy`` 与 ``PIL``——增强版新增了「从本机游戏目录按需解出
   立绘 / 背景」的能力，需要这两个库解析 Unity 资源包。

其余打包契约（冻结运行路径、data/ 落点、lomc 走 PYZ、ICU DLL 过滤）与原版一致。
"""

from PyInstaller.utils.hooks import collect_all

unitypy_datas, unitypy_binaries, unitypy_hidden = collect_all("UnityPy")
pil_datas, pil_binaries, pil_hidden = collect_all("PIL")

# fmod_toolkit 是 UnityPy 解音频时用的绑定，它在
# UnityPy/export/AudioClipConverter.py 里是「导入期」加载的，而 PyInstaller 的
# 依赖图看不到这条路径；漏掉它会让整个 UnityPy.export 导入失败，进而连贴图
# 解码都跑不起来（报的却是 fmod.dll 找不到，很容易误判成图片问题）。
# 它自带的 libfmod/Windows/x64/fmod.dll 是随包分发的数据文件，collect_all 会
# 一并带走；pyfmodex 只在真正解码音频时懒加载，本项目用不到，不打。
fmod_datas, fmod_binaries, fmod_hidden = collect_all("fmod_toolkit")

# archspec 是 etcpak / astc_encoder 用来探测宿主 CPU 指令集、挑选对应贴图解码器的库。
# 它的判定数据是随包分发的 JSON（archspec/json/cpu/*.json），PyInstaller 只收了
# 模块、没收数据文件，冻结后读 cpuid.json 会直接 FileNotFoundError，
# 表现为「贴图解码失败」，很容易误判成图片或资源包有问题。
archspec_datas, archspec_binaries, archspec_hidden = collect_all("archspec")

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
    ]
    + unitypy_datas
    + pil_datas
    + fmod_datas
    + archspec_datas,
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
        "game_assets",
        "asset_picker_dialog",
        "asset_extract_dialog",
        "fmod_toolkit",
        "fmod_toolkit.fmod",
        "fmod_toolkit.importer",
        "texture2ddecoder",
        "etcpak",
        "astc_encoder",
    ]
    + unitypy_hidden
    + pil_hidden
    + fmod_hidden
    + archspec_hidden,
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

# Qt 6 on Windows 有意使用系统自带的 ICU shim；开发机 PATH 上如果还挂着 Poppler
# 的私有 ICU 构建，PyInstaller 会把它误认成 Qt 依赖，打进去反而让 QtCore
# 以 WinError 127 启动失败。这里把外来的 ICU 二进制剔掉（与原版 spec 同理）。
_foreign_icu_names = {"icuuc.dll", "icudt78.dll"}


def _strip_foreign_icu(binaries):
    return [item for item in binaries if item[0].lower() not in _foreign_icu_names]


# collect_all 抓到的 UnityPy / PIL 附带二进制要并进来，否则部分解码扩展会缺失。
# 注意只能并 2 元组来源里「本来就空」的列表：a_gui.binaries 已经是 3 元组，
# 再混进 collect_all 的 2 元组会让 COLLECT 的 TOC 归一化直接报错。
# fmod.dll 已经由上面的 fmod_datas 按 fmod_toolkit/libfmod/... 相对路径带走了，
# 不需要也不能再当成 binary 混进来。
a_gui.binaries = _strip_foreign_icu(a_gui.binaries + unitypy_binaries + pil_binaries)
b_cli.binaries = _strip_foreign_icu(b_cli.binaries)

pyz_gui = PYZ(a_gui.pure)
pyz_cli = PYZ(b_cli.pure)

exe_gui = EXE(
    pyz_gui,
    a_gui.scripts,
    exclude_binaries=True,
    name="lom_editor_plus",
    console=False,
    icon="assets/lom_editor.ico",
)

exe_cli = EXE(
    pyz_cli,
    b_cli.scripts,
    exclude_binaries=True,
    name="story_api_cli",
    console=True,
)

coll = COLLECT(
    exe_gui,
    exe_cli,
    a_gui.binaries,
    a_gui.datas,
    b_cli.binaries,
    b_cli.datas,
    name="lom_editor_plus",
    upx=False,
)
