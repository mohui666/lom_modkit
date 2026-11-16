# Rust 工具端

Cargo workspace 由 `rust/lom-core`、`rust/lomc`、`rust/lom-editor`、`rust/lom-assets` 和 `rust/lom-host-native` 组成，覆盖剧情编译、包读写、原生桌面编辑、游戏资源读取与 Host 批量计算。C# `runtime/MortalModHost` 保留游戏接入；1.6.2 的包、Story、用户内容版本仍为 **3 / 2 / 1**。

操作指南见 [软件使用](software_usage.md)，验证命令见 [测试入口](test_matrix.md)。

## 当前界面

当前是 Rust／egui 通用外壳，标准系统标题栏显示作品名称与未保存状态。默认窗口 1280×760，保留左侧章节／步骤、中间属性编辑、右侧预览的三栏结构。步骤显示序号、类别、节点备注或 ID 及内容概要；同名章节可按列表序号区分，内部标识留在按需查看或高级编辑入口。

顶部工程标签支持多个工程切换，关闭／重启恢复打开的工程和当前位置；每个标签有独立草稿、历史与控件状态。步骤可通过系统剪贴板在不同窗口间复制粘贴，素材引用仍需目标工程具备对应内容。舞台预览的分辨率是编辑器偏好，可选择预设或输入尺寸，不写入作品节点；人物、背景和 UI 按各自原生几何投影。

新建／复制章节先选择操作，再输入可读名称，内部标识自动生成。素材导入先选类型与文件，再确认名称。相关模态框阻挡背景操作，取消在左、主操作在右，支持 Return 确认和 Esc 取消；输入法确认候选不会提前提交。普通可选字段直接显示语义默认值，技术标识改名在“编辑 → 高级”。

简体中文、繁體中文、日本語、한국어设置即时生效。Mac 的“设置…”位于屏幕顶部系统应用菜单，快捷键为 `⌘,`；非 Mac 提供窗口内设置入口及 `Ctrl+,`。语言偏好不修改项目内容。

「设置 → 常规 → 界面主题」提供经典、浅色、深色、暖纸色、竹青色，切换即时生效，并记住下次启动的选择。主题同时应用到属性面板、流程图、帮助和弹窗，不改写作品或游戏预览画面的配色。

**Swift 外壳与原生 Liquid Glass 尚未完成。** 当前主题、标准 AppKit 标题栏和系统菜单由 Rust 实现，不应描述为已完成玻璃效果。

## 构建与启动

在仓库根目录执行：

```sh
cargo build --locked --release -p lomc -p lom-editor
cargo run --locked -p lom-editor -- samples/showcase3
```

生成 macOS 独立应用：

```sh
sh scripts/build-macos.sh
open "out/LoM Modkit Rust.app"
```

`build-macos.sh` 调用 `build_rust_macos.sh`，生成原生 `lom-editor`、`lomc` 与 Rust 素材提取器，打包图标并执行应用签名检查及编译预览烟测。提取器通过独立进程与编辑器交换进度和结果；图片、动画、剧情和音频资源读取均由 `lom-assets` 实现。使用安装包不需要另装 Python、Qt 或当前源码目录。官方预览资源从作者自己的游戏提取，不随应用分发。

构建提取器使用 `cargo build --locked --release -p lom-assets`；无需 UnityPy 或 PyInstaller。两端均构建本平台的提取器，不能复制另一平台的二进制。`--self-check` 不读取游戏。`tools` 中的旧 Python 读取器仅作为迁移结果的开发对照，不进入发布包。

Windows 构建入口为 `scripts/build-windows.ps1 -NoArchive`，需要已构建的同版 `MortalModHost.dll`、`NVorbis.dll`；可用 `-RuntimeDirectory` 指定目录，正式单文件产物为 `out/windows/lom-editor.exe`。CLI、SDK、原生提取器、音频解码器、FFmpeg 与 Host 均由该 exe 内置，配套组件在个人缓存中解包；正式 Release 只上传此 exe。Host 本地引用及嵌入资源要求见 [Runtime 构建说明](../../runtime/MortalModHost/README.md#构建与测试)。

内置 FFmpeg 使用固定的 9.0.2 与 x264 源码裁剪构建，只保留原生视频的 H.264／AAC／MP4、原始画面与 PCM 输入、混音和格式转换，同时保留汇编优化与线程。先运行 `scripts/build-video-encoder.ps1`，默认输出到 `out/video-encoder-9.0.2-minimal`，后续编辑器构建复用该目录。该入口需要现有的 Git Bash、MinGW-w64 GCC／GNU Make／NASM 和 Perl pkg-config；路径参数见脚本。仅缺失的固定源码会下载，已有源码和对象缓存可继续使用。`-VideoEncoderDirectory` 可指定自备编码器目录，分发时同时维护对应许可与来源说明；默认构建来源见 [FFmpeg 说明](../../editor/assets/video/FFMPEG-NOTICE.md)。

## 模块

| 路径 | 职责 |
| --- | --- |
| `rust/lom-core` | 节点校验与 Lua 生成、本地化、用户内容、v3 包校验、工程读写与迁移、离线分析、发布体检、水印、C# 宿主文件管理 |
| `rust/lomc` | 原生命令行与受控创作 API |
| `rust/lom-editor` | egui 界面、字段表单、舞台／流程预览、编辑历史、草稿与恢复、按文件归属保存 |
| `rust/lom-assets` | 原生游戏资源提取器，独立进程报告进度与结果 |
| `rust/lom-host-native` | Windows x64／x86 字节、JSON、剧情参照、事件计划、文本与流式音频计算，通过 C ABI 供 C# Host 调用；构建见 [Runtime 说明](../../runtime/MortalModHost/README.md#构建与测试) |
| `rust/lom-editor/src/macos_window.rs` | 标准 AppKit 窗口外观、现有系统应用菜单中的设置项与重绘通知 |
| `editor/` | 图标、翻译和帮助等静态资源；旧 Python 编辑器已退役 |
| `tools/` | 游戏资源提取／研究脚本；预览提取器单独打包供原生界面调用，其余脚本不作为编辑器或编译器运行依赖 |

## CLI

Python 节点构造函数和完整工程生成见 [Python 对象创作与 CLI](ai_cli.md)。单文件编辑器使用 `lom-editor.exe --cli <命令>`，`sdk -o DIR` 导出配套的 Python 接口；`schema nodes/catalog` 可供 AI 查询完整字段与官方资源清单。

编辑器操作还提供 `asset-query` 图片／布局证据、`render-preview(s)` 纯舞台出图、`playtest-status` 真实执行阶段查询，以及 `playtest-event` 原版前置章衔接测试。`render-video` 默认使用 Windows 独立桌面 Worker，按游戏自动播放时钟输出原生画面与声音，不使用编辑器重绘。详细参数见 [CLI 参数](cli_parameters.md)。顶层 `--help`、`--version` 在 1.6.0 直接输出，不打开窗口；旧版识别版本使用 `--cli --version`。

以下示例使用当前 CLI 参数；`path/to/...` 为作者自己的输入文件：

```sh
cargo run --locked -p lomc -- check samples/showcase3/story/main.json --json
cargo run --locked -p lomc -- build samples/showcase3/story/main.json -o out/main.lua
cargo run --locked -p lomc -- pack samples/showcase3 -o out/showcase3-rust.lommod --json
cargo run --locked -p lomc -- inspect out/showcase3-rust.lommod --json
cargo run --locked -p lomc -- new-story chapter_2 --title "第二章" -o out/chapter_2.json --json
cargo run --locked -p lomc -- statistics samples/showcase3 --json
cargo run --locked -p lomc -- analyze samples/showcase3 --json
cargo run --locked -p lomc -- preflight samples/showcase3 --profile release --json
cargo run --locked -p lomc -- test path/to/project --json
cargo run --locked -p lomc -- release path/to/project -o out/release --json
cargo run --locked -p lomc -- migrate path/to/story.json --kind story --json
cargo run --locked -p lomc -- content-inspect path/to/asset.lomcontent --json
cargo run --locked -p lomc -- author path/to/request.json --json
cargo run --locked -p lomc -- edit path/to/story.json --operations path/to/operations.json -o out/edited-story.json --json
cargo run --locked -p lomc -- detect-watermark path/to/screenshot.png --json
cargo run --locked -p lomc -- detect-watermark-video path/to/video.mp4 --ffmpeg /path/to/ffmpeg --json
```

`build`／`compile` 默认写同名 `.lua`，示例显式输出到 `out/`，避免在受跟踪 JSON 旁留下生成文件。`--json` 返回结构化结果；操作失败退出码为 1，水印未检出为 2。视频检测需要 FFmpeg。`test` 要求项目已定义离线测试；`edit` 未指定 `-o` 时会写回输入文件，以上示例另存输出。

`inspect` 拒绝危险 ZIP 路径；对可安全读取但校验不通过的包提供只读文件预览与错误报告，并返回失败状态。它检查完整性以及用包内资源重新编译后的源码／Lua 一致性，不执行包内 `raw` 脚本，也不是作者身份认证。

## 工程与兼容边界

目录保存保留章节原文件名、未知元数据和无关文件；支持标准 `story/` 布局及直接放置章节 JSON 的目录。编辑器保存时，只清理由上次保存快照证明且未被外部修改的已删除文件。另存与导出不清理原目录；导出重新编译章节，只打入实际引用素材。

有效素材／离线测试草稿纳入统一保存，无效输入不会因切换、保存或撤销静默丢失。自动恢复写入独立目录，不覆盖原项目，并保留原来源信息。详细机制见 [维护者手册](maintainers.md#保存撤销与恢复)。

迁移沿用既有版本规则，不猜测无法可靠转换的旧 Combat／Battle 数据。可迁移文件在写入前保留原字节备份；多文件写盘失败不保证整个目录一次回滚。新项目生成独立 `campaign_id`，导入已有项目不重置存档身份。

Windows 游戏管理调用已有 C# Host 协议；Mac 上禁用游戏接入按钮。BepInEx 安装按 `Mortal.exe` 自动识别 x86／x64，接受对应的官方 **6.0.0-be.692+851521c / Unity Mono** ZIP，界面提供匹配的下载链接；校验值或原生代理位数不符时拒绝写入。C# Host 保持 AnyCPU，正式 exe 使用内置 Host，源码运行才需明确完整宿主目录。游戏从 32 位升级到 64 位后的加载器与旧代理修复见 [维护者手册](maintainers.md#windows-游戏接入与-runtime-回滚)。Mac 构建和离线预览都不能代替 Windows／游戏流程；本次 1.6.0 未运行 Mac 构建或实测。

## Host 的 Rust 计算模块

`rust/lom-host-native` 由 `scripts/build-host-native.ps1` 构建 x64／x86 DLL，作为资源嵌入 C# Host。`NativeHostKernel` 共用一次加载，包／路径检查、JSON、原版来源与补丁、事件计划、存档编解码、文本计数与流式音频通过批量 C ABI 进入 Rust；游戏对象、事件副作用与播放生命周期由 C# 管理。源码中仍保留无原生组件时的原计算入口，原生拒绝不会转为托管重试。包格式、字段、节点和已有作品的身份保持不变。完整构建、所有权与定向差分命令见 [Host 说明](../../runtime/MortalModHost/README.md#构建与测试)。

1.6.2 的决斗专用图片由 `lom-assets` 按资源文件和精确 Sprite 名读取，与普通 `views` 分组保存；运行游戏时由 C# Host 读取 `CombatLevel.BackImage`，不从原版完整关卡继承事件或人物数值。素材库缺少新组仍可打开，补充提取后提供专用背景预览。

## 样例与验证

```sh
cargo test --locked --workspace --release
cargo run --locked -p lom-editor -- --smoke-preview samples/showcase3
cargo run --locked -p lomc --example build_showcase3 -- out/showcase3-check
cargo run --locked -p lomc -- inspect out/showcase3-check/showcase3.lommod --json
```

生成器读取 `samples/showcase3/source.json`；传入显式输出目录时必须使用尚未存在的新目录。无参数模式按已有工程的文件归属检查刷新受跟踪样例，仍应事先审阅差异并保留用户文件。详见 [样例说明](../../samples/showcase3/README.md)。

历史 Python 对照实现只用于生成已保存的 JSON／Lua fixture，当前测试不启动 Python。Lua 以原始字节比较，包按解压条目和逻辑完整性比较，不要求不同压缩库产生完全相同的 ZIP 压缩字节。当前功能与验证范围见 [当前能力](current_capabilities.md)。
