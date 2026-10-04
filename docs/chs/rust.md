# Rust 工具端

Cargo workspace 将剧情编译、包读写和桌面编辑器放在同一套原生核心上。C# `runtime/MortalModHost` 保留现有游戏接入与运行行为。

Mac 界面沿用现有液态玻璃主题：深色内容面板、蓝色选中态、半透明工具层与高光边框；系统支持时接入 AppKit 原生玻璃材质，遵从“减少透明度”辅助功能设置。

主要排版沿用原版：默认窗口 1280×760，左侧剧情导航、中间两列属性表单、右侧画面预览／人物立绘／流程图／编译；添加步骤固定在左栏底部，播放控制位于预览底部。初始三栏约为 280／420／560。Rust 控件与 Qt 的细节、菜单呈现仍有差别，不是逐像素复刻。

## 构建与启动

在仓库根目录执行：

```sh
cargo build --locked --release
cargo run -p lom-editor -- samples/showcase3
```

macOS 独立应用：

```sh
sh scripts/build_rust_macos.sh
open "out/LoM Modkit Rust.app"
```

构建脚本生成原生 `lom-editor` 与 `lomc`，并执行应用签名检查和内置编译预览冒烟。应用不需要 Python、Qt 或当前源码目录。官方预览图片仍从作者自己提取的外部素材库读取，不随应用分发。

## 模块

| 路径 | 职责 |
| --- | --- |
| `rust/lom-core` | 63 类节点校验与 Lua 生成、本地化、用户素材交换、v3 包校验、工程读写与迁移、离线分析、发布体检、水印、C# 宿主文件管理 |
| `rust/lomc` | 原生命令行入口 |
| `rust/lom-editor` | egui 桌面界面、动态节点表单、舞台与流程图、编辑历史和工程操作 |
| `editor/` | 图标、翻译和帮助等静态资源；旧 Python 实现已退役 |

## CLI

```sh
cargo run -p lomc -- check samples/showcase3/story/main.json --json
cargo run -p lomc -- build samples/showcase3/story/main.json -o out/main.lua
cargo run -p lomc -- pack samples/showcase3 -o out/showcase3-rust.lommod --json
cargo run -p lomc -- inspect out/showcase3-rust.lommod --json
cargo run -p lomc -- new-story out/new-story.json --json
cargo run -p lomc -- detect-watermark screenshot.png --json
cargo run -p lomc -- detect-watermark-video video.mp4 --ffmpeg /opt/homebrew/bin/ffmpeg --json
cargo run -p lomc -- preflight samples/showcase3 --profile release --json
cargo run -p lomc -- statistics samples/showcase3 --json
cargo run -p lomc -- analyze samples/showcase3 --json
cargo run -p lomc -- test path/to/project --json
cargo run -p lomc -- release path/to/project -o out/release --json
cargo run -p lomc -- migrate path/to/story.json --kind story --json
cargo run -p lomc -- content-inspect asset.lomcontent --json
```

`build` / `compile` 默认写同名 `.lua`；`--json` 返回结构化输出路径与诊断，保留原命令行的文件生成行为。`new-story chapter_2 --title "第二章" -o out/chapter_2.json` 也接受旧 `story_api` 的创建参数。失败退出码为 1；水印检测结果为未检出时退出码为 2。视频检测需要 FFmpeg。

## 兼容与验证

包、Story 和用户内容版本继续为 **3 / 2 / 1**；没有增加游戏 API，也没有修改 C# 宿主。目录保存保留章节原文件名、未知项目元数据和无关文件；标准 `story/` 布局与直接放置章节 JSON 的目录都能打开。导出时重新编译全部章节，资源只打入实际引用项。

编辑器包含多章节与多选编辑、拖动、撤销/重做、带范围校验的分区、连续节点模板、原有 6 种项目模板、最近项目、自动保存恢复、全文搜索与替换、批量字段修改、内容本地化、共享内容库、流程与演出预览、离线测试、节点参考和四语言界面入口。原辞典未覆盖的少量新增提示、说明和诊断语句保留中文，故事文本及字段 ID 不参与界面翻译。

“创作工具”中的发布体检会检查实际资源、流程、元数据和最低 Host 版本；修复建议先展示，再应用到可撤销历史。发布目录包含 `.lommod`、SHA-256 与说明。包检查不仅验证 ZIP 路径和哈希，还在隔离临时目录中用包内资源重新编译 Story，并逐字节比较 Lua。它不代表原始 `raw` 脚本安全或作者身份认证。

迁移沿用旧实现的真实版本规则；不猜测无法可靠转换的旧 Combat/Battle 数据。可迁移文件在写入前保留原字节备份，工程打开先完成全部文档与资源检查。迁移和安装修复采用逐文件原子写入及备份，跨文件写盘失败时不保证整个目录一次回滚。新建项目生成独立且持久的 `campaign_id`，导入已有项目不会重置存档身份。

Windows 游戏管理入口保留在原生代码中，调用已有 C# 宿主协议；macOS 上禁用游戏接入按钮。BepInEx 安装接受原版本固定哈希的官方 x86 ZIP；C# 宿主目录需显式选择，包含 `MortalModHost.dll` 与 `NVorbis.dll`。本次没有运行这些 Windows 流程，也没有连接或启动真实游戏。

```sh
cargo test --locked --workspace
cargo run -p lom-editor -- --smoke-preview samples/showcase3
```

测试中的历史 fixture 由开发阶段运行 Python 对照实现采集；Rust 测试与发行程序不启动 Python。Lua 以原始字节比较；包按解压条目与逻辑完整性比较，不把不同压缩库的 ZIP 压缩字节要求为跨语言一致。

本次仅在 Apple Silicon macOS 构建和验证。遵照任务要求，不做 Windows 测试或游戏实机测试；这两项不能由 Mac 的编译与预览结果替代。

后续补齐功能、受控 API、清理范围与当前验证入口见 [迁移验收表](rust_migration.md)。
