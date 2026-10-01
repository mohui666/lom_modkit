# Mac 编辑器

Mac 版与 Windows 版共用 Story、编译器和 `.lommod` 格式，支持编辑、预览、内容库、校验和导出。游戏运行时仍在 Windows 使用，Mac 的安装按钮和 F5 试玩入口不可用。

## 启动与外观

当前构建为 Apple Silicon（arm64）的 `LoM Modkit.app`，不需要另外安装 Python。可将整个 `.app` 拖入应用程序目录，也可以直接双击运行。

macOS 26 及以上通过 AppKit 的 `NSGlassEffectView` 显示液态玻璃材质。窗口和标题栏保留实色底，导航与表单使用不透明深色面板；工具栏保留半透明材质，背景窗口的文字不会直接透到标题或正文上。切换步骤、人物和页签时清除旧的透明画布，避免残影。系统打开“减少透明度”时使用不透明背景。较早的系统使用 `NSVisualEffectView`；本次仅在 macOS 27 的 Apple Silicon 主机上验证，尚未验证旧系统或 Intel Mac。

编辑器偏好、自动恢复和用户内容保存在 `~/Library/Application Support/lom_modkit/`，应用程序目录不保存这些数据。

应用图标以本机游戏中的赵活立绘为参考重新绘制，结合纸墨、毛笔和“编”字朱砂印，外框保留玻璃质感。源图位于 `editor/assets/lom_editor_icon.png`，对应 macOS 的多尺寸 `lom_editor.icns` 和 Windows 的 `lom_editor.ico`；生成提示保留在同目录的 `lom_editor_icon.prompt.txt`。

## 立绘素材

在“文件 → 选择游戏预览素材目录…”中选择包含 `preview_map.json` 与 `assets/portraits/` 的目录。选中人物或表情后，右侧“人物立绘”页显示完整图片，也可以点击人物栏下的同名按钮。

缓存由用户自己的 Windows 游戏安装提取，并传到自己的 Mac。应用程序和导出的 MOD 不包含这些官方图片。Windows 提取命令：

```powershell
python tools/extract_preview_assets.py --game-dir "C:\Program Files (x86)\Steam\steamapps\common\LegendOfMortal" --output-dir out/previews --max-size 640 --portraits-only
```

提取环境需要 UnityPy 和 Pillow。将整个 `out/previews` 目录传到 Mac，再从编辑器选择它。自定义人物则使用项目及用户内容库中的图片。

## 从源码构建

使用现有 Python 3.12 或更高版本，在仓库根目录执行：

```bash
LOM_MODKIT_PYTHON=/path/to/python3 scripts/build-macos.sh
```

产物位于 `editor/dist/LoM Modkit.app`，内含 GUI 与 `Contents/MacOS/story_api_cli`。构建脚本执行冻结版 Lua 预览自检；不启动游戏。

Mac 上完成的项目可以导出 `.lommod`，传到 Windows 使用新 Host。赵活外观选择需要 Host 1.1.2 或更高版本，详见[自由模式与立绘](free_mode_portraits.md)。
