# lom_modkit

**《活侠传》（Legend of Mortal）可视化剧情 Mod 制作工具。** 用图形编辑器编排人物对白、场景、分支、音乐和音效，导出 `.lommod`，由 C# 游戏插件加载。

当前源码版本为 **v1.2.0**：编辑器和编译器已迁移至 Rust，保留 C# 游戏接入层。公开 Release 最新仍为 **v1.1.1 旧版**（截至 2026-10-04），不包含当前 Rust 界面。当前源码的构建和使用方式见下文。

[软件使用](docs/chs/software_usage.md) · [Mac 编辑器](docs/chs/macos.md) · [文档索引](docs/README.md) · [旧版下载](https://github.com/mohui666/lom_modkit/releases/tag/v1.1.1)

语言：简体中文（本文） · [繁體中文](README.cht.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

## 从源码开始

安装 Rust 后，在仓库根目录运行：

```sh
cargo run --locked -p lom-editor
```

也可以直接打开全节点样例：

```sh
cargo run --locked -p lom-editor -- samples/showcase3
```

Mac 独立应用通过 `scripts/build-macos.sh` 构建，产物为 `out/LoM Modkit Rust.app`，包含编辑器和 `lomc`，运行不依赖 Python 或 Qt。官方立绘预览需要作者自己的外部素材库，配置见 [Mac 编辑器](docs/chs/macos.md)。

Windows 构建入口为 `scripts/build-windows.ps1`，需要已构建的 C# Host。当前 Rust 迁移的验证范围是 macOS 与离线测试，**未进行 Windows 构建、Windows 测试或游戏实机测试**。Mac 可编辑、预览和导出，不能使用游戏安装与 F5 试玩。

## 做第一段剧情

1. 在「文件」中选择「新建项目」或「从模板新建」。
2. 在左侧添加步骤，在中间填写人物、台词和场景，右侧查看画面、立绘或流程图。
3. 增加章节时，先点章节旁的「＋」，选择新建或复制，再填写名称；内部标识自动生成。
4. 使用「创作工具 → 检查项目」或 F6 检查问题，保存全部章节。
5. 点击「导出 Mod」生成 `.lommod`。Windows 游戏侧的安装要求见 [维护者手册](docs/chs/maintainers.md)。

macOS 从屏幕顶部系统应用菜单打开「设置…」，快捷键 `⌘,`；其他平台使用窗口中的「设置…」。界面支持简体中文、繁體中文、日本語和 한국어。当前是浅色 Rust 通用外壳；Mac 专用 Swift 液态玻璃外壳尚未实现。

## 主要功能

| 功能 | 当前入口或说明 |
| --- | --- |
| 63 类剧情节点 | 人物、对白、场景、音频、分支、骰子、战斗与自由模式触发等；字段见 [格式参考](docs/chs/mod_format.md) |
| 多章节编辑 | 保存、撤销/重做、多选、复制粘贴、分组、节点模板与自动恢复 |
| 创作分析 | 「创作工具」内的全局查找、变量管理、条件检查、路径模拟、跨章节复制和离线测试 |
| 用户内容 | 工具栏「用户内容」导入人物、音频和图片；项目保存与导出统一处理有效修改 |
| 四语言剧情 | 「编辑 → 多语言」维护作品译文、回退和覆盖统计；界面语言不会改写作者台词 |
| 检查与交付 | 项目检查、包检查、发布构建、语音覆盖、脱敏诊断和水印检测 |
| 游戏接入 | C# Host 保留，提供 Mod 菜单、试玩与独立战役存档；Rust 迁移未做游戏实机验收 |

资源选择器显示翻译名称；不同资源同名时加稳定数字，例如「武师 1」「武师 2」。数字只用于界面区分，不修改保存的资源标识或台词。人物资源目录也含包子、马车、棋盘等道具立绘；对白模式可为它们配置文字，但不会自动生成语音。详情见 [用户内容](docs/chs/user_content.md)。

静态分析遇到无法确定的游戏状态会返回 `unknown` / `unsupported`，不能替代游戏实测。完整功能对应与验证范围见 [Rust 迁移记录](docs/chs/rust_migration.md)。

## 开发与验证

| 目录 | 用途 |
| --- | --- |
| `rust/lom-core` | 编译、包格式、内容库、受控编辑 API 和离线分析 |
| `rust/lomc` | 命令行工具与样例生成器 |
| `rust/lom-editor` | 原生 Rust 编辑器 |
| `runtime/MortalModHost` | C# 游戏接入层 |
| `editor/` | 图标、翻译、帮助等静态资源和启动入口 |
| `tools/` | 独立的资源提取与接口研究脚本，不是工具运行依赖 |

```sh
cargo build --locked --release --workspace
cargo test --locked --workspace
cargo run --locked -p lomc -- check samples/showcase3/story/main.json --json
cargo run --locked -p lom-editor -- --smoke-preview samples/showcase3
```

样例再生成需要使用全新输出目录，见 [Showcase 3.0](samples/showcase3/README.md)。包契约保持 `package_format=3`、`story_schema=2`、`content_schema=1`；旧 Python `import story_api` 已退役，自动化改用 [Rust API / JSON CLI](docs/chs/ai_cli.md)。

远端主分支为 `master`，通过 PR 和必需的 macOS Rust、Linux Runtime 离线 CI 检查后合并。游戏功能修改须先查 [AGENTS.md](AGENTS.md) 和 [反编译接口](docs/chs/decompiled_api.md)，不得提交整份游戏反编译源码或提取资源。

## 文档与版本

- [文档索引](docs/README.md)：作者指南、格式契约和维护入口。
- [当前能力](docs/chs/current_capabilities.md)：实现范围与限制。
- [Rust 工具端](docs/chs/rust.md)：构建、CLI 与验证命令。
- [维护者手册](docs/chs/maintainers.md)：恢复、体检、发布、安装与回滚。
- [v1.2.0 变更记录](RELEASE_NOTES_v1.2.0.md)：当前 Rust 变更与迁移前历史记录。
- [公开 Releases](https://github.com/mohui666/lom_modkit/releases)：历史安装包；v1.1.1 的旧菜单说明不适用于当前 Rust 界面。

## 许可与来源披露

MIT 许可（[LICENSE](LICENSE)）。粉丝自制工具，与游戏开发商无关，不包含游戏本体文件。`data/editor_data.json` 来自资源研究脚本整理的目录信息，仓库不分发官方资源。

游戏内 Mod 剧情显示「玩家制作 MOD｜非官方内容」、作品与作者自报信息及包指纹；指纹不是官方签名，也不认证作者身份。运行时在无法维持披露时停止 Mod 演出，详见 [Runtime 说明](runtime/MortalModHost/README.md) 和 [来源水印](docs/chs/watermark.md)。
