# 活侠传剧情编辑器

用可视化步骤制作《活侠传》MOD，支持多章节、人物演出、分支、事件任务和自定义素材，也支持 Python 批量制作。

**当前版本：1.5.11 · Windows x64 · 只需一个 exe**

[下载编辑器](https://github.com/mohui666/lom_modkit/releases/download/v1.5.11/lom-editor.exe) · [更新内容](RELEASE_NOTES_v1.5.11.md) · [全部文档](docs/README.md)

[繁體中文](README.cht.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

## 开始使用

1. 下载 `lom-editor.exe`，双击打开，配套组件已内置。
2. 退出游戏，在“游戏与 MOD”中定位游戏目录，点击“一键配置游戏”。
3. 在“素材管理 → 游戏预览素材”提取原版素材。
4. 新建作品、添加剧情，预览并检查后导出 MOD 或完整游玩包。

1.4.19 用户可通过“检查更新 → 更新并重启”升级，也可退出编辑器后手动替换 exe。已有作品、素材和存档身份保留。

| 要做什么 | 从这里开始 |
| --- | --- |
| 安装、更新、配置游戏 | [Windows 快速开始](docs/chs/windows_quick_start.md) |
| 编写剧情、预览和导出 | [软件使用教程](docs/chs/software_usage.md) |
| 导入立绘、图片和音频 | [素材使用](docs/chs/user_content.md) |
| 设置事件和任务 | [事件与任务](docs/chs/events_and_tasks.md) |
| 查看完整步骤示例 | [十章样例](samples/showcase3/README.md) |
| 查询功能和限制 | [当前能力](docs/chs/current_capabilities.md) |

## 用 AI 或 Python 制作

从 exe 导出 Python 接口、AI skill、文档和示例，无需额外下载附件：

```powershell
.\lom-editor.exe --cli sdk -o sdk --json
```

使用 Python 接口需要 Python 3.10+，无需第三方 Python 包。普通编辑器用户无需安装 Python。

[制作流程](docs/chs/ai_cli.md) · [Python 接口](docs/chs/python_api.md) · [CLI 参数](docs/chs/cli_parameters.md) · [AI Skill](skills/lom-modkit-authoring/SKILL.md)

## 仓库目录

| 目录 | 内容 |
| --- | --- |
| `rust/` | 编辑器、编译器、命令行和资源提取器源码 |
| `runtime/` | C# 游戏接入组件 Host |
| `python/` | Python 创作接口和测试 |
| `editor/` | 图标、翻译、内置帮助和加载器资源 |
| `data/` | 编辑器使用的数据与资源索引 |
| `samples/` | 可编辑样例和制作脚本 |
| `docs/` | 使用教程、接口与开发文档 |
| `skills/` | AI 制作指引 |
| `scripts/`、`tools/` | 构建和维护工具 |
| `research/` | 已核对的游戏接口笔记 |

本地的 `out/`、`target/`、缓存、提取的游戏素材和反编译源码均不提交到仓库。

## 开发

```sh
cargo run --locked -p lom-editor
```

[仓库规则](AGENTS.md) · [构建与维护](docs/chs/maintainers.md) · [格式契约](docs/chs/mod_format.md) · [游戏接口核对](docs/chs/decompiled_api.md) · [macOS 构建](docs/chs/macos.md)

## 许可

采用 [MIT 许可](LICENSE)，属于粉丝自制工具，与游戏开发商无关。原版素材从使用者自己的游戏中提取。[第三方组件](runtime/MortalModHost/THIRD_PARTY_NOTICES.md) · [作品来源标识](docs/chs/watermark.md)
