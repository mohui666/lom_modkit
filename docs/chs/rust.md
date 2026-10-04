# Rust 工具端

Cargo workspace 由 `rust/lom-core`、`rust/lomc`、`rust/lom-editor` 组成，覆盖剧情编译、包读写和原生桌面编辑。C# `runtime/MortalModHost` 保留游戏接入；包、Story、用户内容版本仍为 **3 / 2 / 1**。

2026-10-04 至 2026-10-05 已完成 Windows 工具端构建、自动化及新 exe 的具体 UI 路径验证；最终 Rust 完整回归为 **196 通过、0 失败、2 默认跳过**。范围、修复与限制见 [Windows 验证记录](windows_validation.md)，历史 Mac 结果见 [测试矩阵](test_matrix.md)。游戏实机未执行。

## 当前界面

当前是 Rust／egui 浅色通用外壳，标准系统标题栏显示作品名称与未保存状态。默认窗口 1280×760，保留左侧章节／步骤、中间属性编辑、右侧预览的三栏结构。步骤使用序号、类别和文字摘要；同名章节可按列表序号区分，内部标识留在按需查看或高级编辑入口。

新建／复制章节先选择操作，再输入可读名称，内部标识自动生成。素材导入先选类型与文件，再确认名称。相关模态框阻挡背景操作，取消在左、主操作在右，支持 Return 确认和 Esc 取消；输入法确认候选不会提前提交。普通可选字段直接显示语义默认值，技术标识改名在“编辑 → 高级”。

简体中文、繁體中文、日本語、한국어设置即时生效。Mac 的“设置…”位于屏幕顶部系统应用菜单，快捷键为 `⌘,`；非 Mac 提供窗口内设置入口及 `Ctrl+,`。语言偏好不修改项目内容。

**Swift 外壳与原生 Liquid Glass 尚未完成。** 当前浅色界面、标准 AppKit 标题栏和系统菜单由 Rust 实现，不应描述为已完成玻璃效果。

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

`build-macos.sh` 调用 `build_rust_macos.sh`，生成原生 `lom-editor` 与 `lomc`、打包图标、执行应用签名检查及内置编译预览烟测。应用不需要 Python、Qt、PyInstaller 或当前源码目录。官方预览图片仍从作者自己提取的外部素材库读取，不随应用分发。

Windows 构建入口为 `scripts/build-windows.ps1 -NoArchive`，需要已构建的 `MortalModHost.dll`、`NVorbis.dll`；可用 `-RuntimeDirectory` 指定目录，默认产物为 `out/windows/lom_modkit/lom-editor.exe`。2026-10-05 已使用正式 net48 Host 产物，在独立输出目录完成打包并实际启动新 exe；构建成功、自动化与 UI 验收分别记录。Host 本地引用及嵌入资源要求见 [Runtime 构建说明](../../runtime/MortalModHost/README.md#构建与测试)。

## 模块

| 路径 | 职责 |
| --- | --- |
| `rust/lom-core` | 63 类节点校验与 Lua 生成、本地化、用户内容、v3 包校验、工程读写与迁移、离线分析、发布体检、水印、C# 宿主文件管理 |
| `rust/lomc` | 原生命令行与受控创作 API |
| `rust/lom-editor` | egui 界面、字段表单、舞台／流程预览、编辑历史、草稿与恢复、按文件归属保存 |
| `rust/lom-editor/src/macos_window.rs` | 标准 AppKit 窗口外观、现有系统应用菜单中的设置项与重绘通知 |
| `editor/` | 图标、翻译和帮助等静态资源；旧 Python 编辑器已退役 |
| `tools/` | 仍有独立的游戏资源提取／研究脚本；不是编辑器或编译器运行依赖 |

## CLI

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

Windows 游戏管理调用已有 C# Host 协议；Mac 上禁用游戏接入按钮。BepInEx 安装接受代码限定的官方 x86 ZIP，宿主 DLL 目录由作者选择。本轮 Windows 编辑器、Host 构建、离线测试及 133 节点预览均不能代替游戏接入实机流程；没有启动游戏、按 F5、安装/更新插件或修改游戏存档。

## 样例与验证

```sh
cargo test --locked --workspace --release
cargo run --locked -p lom-editor -- --smoke-preview samples/showcase3
cargo run --locked -p lomc --example build_showcase3 -- out/showcase3-check
cargo run --locked -p lomc -- inspect out/showcase3-check/showcase3.lommod --json
```

生成器读取 `samples/showcase3/source.json`，输出目录请使用尚未存在的新目录；不要把默认目录或已有输出视为已获得覆盖权限。详见 [样例说明](../../samples/showcase3/README.md)。

2026-10-05 Windows 最终完整回归为 196 项通过、0 失败、2 项默认跳过，Showcase 预览检查 133 / 133；新建输出目录的样例生成、包检查及 C# 离线冒烟通过。此前 2026-10-04 Mac 的 179 / 0 / 2 和同日仅文档维护的记录保持为历史，不算本轮 Windows 结果。

历史 Python 对照实现只用于生成已保存的 JSON／Lua fixture，当前测试不启动 Python。Lua 以原始字节比较，包按解压条目和逻辑完整性比较，不要求不同压缩库产生完全相同的 ZIP 压缩字节。最终测试数不累加阶段重跑、预览状态或 UI 操作；受阻与未执行项见 [Windows 验证记录](windows_validation.md#受阻未执行与剩余限制)。

功能对照与清理范围见 [迁移验收表](rust_migration.md)。
