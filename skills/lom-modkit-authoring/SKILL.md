---
name: lom-modkit-authoring
description: 使用 LoM Modkit 的 Python 节点对象和 Rust CLI 制作、修改、校验活侠传多章节剧本，生成编辑器工程与 lommod。用于作者要求 AI 批量创作或修改作品时。
---

# 活侠传剧本制作

交付可编辑工程和可重跑的 Python 制作文件；用户要求导出时生成 `.lommod`。保持作者给出的剧情、分支与作用范围。

## 找到实际接口

- 独立 skill 附件带有 `references/python_api.md`、`references/cli_parameters.md` 和示例，可先阅读；运行前仍从本次使用的 exe 导出匹配 SDK。
- 有源码：先读 `docs/chs/python_api.md` 和 `docs/chs/cli_parameters.md`；需要完整示例再读 `docs/chs/ai_cli.md`。
- 只有编辑器：执行 `lom-editor.exe --cli sdk -o 一个新目录 --json`，读取导出的 `python_api.md`、`cli_parameters.md`，使用同目录的 lom_modkit 包。
- 查询 `--cli schema nodes/catalog` 获取当前字段、枚举和官方资源 ID；查询 `--cli automation-schema` 获取当前操作命令。不要猜官方人物、表情、音频或游戏方法。
- 用用户提供或工作区实际构建的编辑器，不自动覆盖安装版或下载其他版本。SDK 与本次使用的 exe 配套。

## 编写与生成

以一个 Python 文件组织多个 `Chapter`，放入 `Project(chapters=[...])`。每个步骤使用 `Say/Show/Choice/...` 构造函数。先登场再发言／移动；跨章用 `End(next_script=...)`；同组子动作用 `.action()`，不带 id/goto。构造函数检查参数，完整关系由 Rust 校验。

按任务选择配置对象：

- 事件任务：Task/Event/Scenario、Condition/All/Any/Not、Outcome/VariableChange。
- 译文：Localization/Translation，路径通过 texts 查询，保留 ID 和资源路径。
- 素材：ImageAsset/CharacterAsset/AudioAsset；其余类型用 Asset。节点引用 `.ref`，不把本地绝对文件路径写进演出字段。
- 路线验收：RouteTest/ChoiceStep 写入章节 `_editor.tests`。option 从0开始，同一 choice 的重复动作按顺序消费，预期写 assert。

执行 `cli.build(project, 新工程目录)`；需要包时再执行 `cli.export(...)`。成功结果应包含原生体检、编译和工程／包重开检查。遇到 CLIError 读取 `.result` 的 issues/errors，按 story_id/node_id 修改；不要捕获异常后继续宣称成功，也不要伪造初始旗标来掩盖警告。

修改已有作品先 `Project.open(cli, path)`，保留 source、campaign_id、节点元数据及资源。recipe 没有 assets 键时，资源仍由 source 读取。删除章节／事件／资源后另存新目录。不要通过手写 Lua 或重算校验记录绕过包的源码归属检查。

## 检查与交付

按实际内容选择体检和路线测试。检查关键分支、循环出口与结局；`unsupported` 不算通过。需要确认编辑器可读时使用 `--smoke-preview`；用户希望直接检查界面时用 `cli.open_editor(project_or_package, output=可选新目录, script=章节, node=步骤)`。

资源读取、位置调整和试玩包准备可用 `cli.operation(...)` 调用 Rust 命令。普通生成不自动执行 playtest、capture-effects 或安装游戏组件；作者明确要求游戏操作时才执行相应命令。

区分构造检查、编译／包检查、离线模拟、编辑器界面与游戏实机证据。剧情合理性、视觉听感、随机结果、战斗与事件调度的最终表现交给作者确认。

交付时给出 Python 文件、工程目录、MOD 路径和本次实际测试结果，简要列出未验证事项。版本发布、提交和推送按用户当前授权执行。
