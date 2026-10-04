# Mac 编辑器

Mac 版与 Windows 版共用 Story、编译器和 `.lommod` 格式，支持编辑、预览、内容库、校验和导出。游戏运行时仍在 Windows 使用，Mac 的安装按钮和 F5 试玩入口不可用。

## 启动与外观

当前构建为 Apple Silicon（arm64）的 `LoM Modkit Rust.app`，不需要另外安装 Python。可将整个 `.app` 拖入应用程序目录，也可以直接双击运行。

当前先提供 Rust 通用外壳：浅色界面、三栏布局和系统标准标题栏。窗口顶部显示作品名称及“活侠传剧情编辑器”；未保存时有 `*` 标记。标题只在内容变化时更新，避免重复刷新。

早期 AppKit 玻璃叠层与当前 OpenGL 窗口发生冲突，导致标题栏异常或正文被遮挡，已移除。当前版本不提供液态玻璃；Mac 专用 Swift 外壳按计划留待后续实现。本次仅在 macOS 27 的 Apple Silicon 主机上验证，尚未验证旧系统或 Intel Mac。

编辑器偏好、自动恢复和用户内容保存在 `~/Library/Application Support/lom_modkit/`，应用程序目录不保存这些数据。

常规操作先选动作再填写内容：“＋”菜单的新建/复制章节会打开独立命名窗口，内部标识自动生成；素材导入先选类型和文件，再确认名称。人物、表情、模式与语音可直接选择，“默认/无”恢复省略字段。修改技术标识位于“编辑 → 高级”。内容属性与离线测试的有效修改随项目统一保存；未完成的输入会保留并阻止误保存或切换，不再需要另点“应用修改”。

屏幕顶部的“活侠传剧情编辑器 → 设置…”或 `⌘,` 打开界面语言设置；窗口工具栏不再重复放置单项菜单。设置菜单由 Rust 接入系统应用菜单，保留隐藏、退出等系统操作。简体中文、繁體中文、日本語和 한국어 使用各自原名；语言切换立即生效并同步菜单标题，不修改作品内容。系统字体分别补齐中、日、韩字符。新建、导入和修改标识使用模态对话框，取消在左、主要操作在右，支持回车确认和 Esc 取消；中文输入法选词时的回车不会误提交。交互参照 [Apple Human Interface Guidelines](https://developer.apple.com/design/human-interface-guidelines/settings)，保留 Rust 通用外壳与三栏编辑布局。

资源列表显示翻译名称；同名但标识不同的选项按稳定顺序显示为“武师 1、武师 2”等，搜索不会重新编号。编号只用于选择界面，不写入作品名称、台词或底层资源标识。人物、受限战斗角色、作品内容、章节和分区使用同一编号规则；没有名称证据的官方检查点仍保留编码，避免把推测当成翻译。

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
