# Mac 编辑器

Mac 版与 Windows 版共用 Story、编译器和 `.lommod` 格式，支持编辑、预览、内容库、校验和导出。游戏运行时仍在 Windows 使用，Mac 的安装按钮和 F5 试玩入口不可用。

## 启动与外观

当前构建为 Apple Silicon（arm64）的 `LoM Modkit Rust.app`，不需要另外安装 Python。可将整个 `.app` 拖入应用程序目录，也可以直接双击运行。

当前先提供 Rust 通用外壳：浅色界面、三栏布局和系统标准标题栏。窗口顶部显示作品名称及“活侠传剧情编辑器”；未保存时有 `*` 标记。标题只在内容变化时更新，避免重复刷新。

早期 AppKit 玻璃叠层与当前 OpenGL 窗口发生冲突，导致标题栏异常或正文被遮挡，已移除。当前版本不提供液态玻璃；Mac 专用 Swift 外壳按计划留待后续实现。本次仅在 macOS 27 的 Apple Silicon 主机上验证，尚未验证旧系统或 Intel Mac。

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

安装 Rust，在仓库根目录运行 `scripts/build-macos.sh`。产物为 `out/LoM Modkit Rust.app`，包含 `lom-editor` 和 `lomc`；编辑和编译不依赖 Python 或 Qt。构建会检查 133 个样例节点预览，不启动游戏。详见 [Rust 迁移](rust_migration.md)。
