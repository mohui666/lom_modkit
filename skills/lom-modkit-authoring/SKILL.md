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
- 目录可用 `schema catalog --filter 关键词` 缩小范围；配合 `-o 文件 --summary` 把完整结果写入文件，仅在标准输出读取摘要。不要把摘要当完整目录。
- 用用户提供或工作区实际构建的编辑器，不自动覆盖安装版或下载其他版本。SDK 与本次使用的 exe 配套。

## 编写与生成

以一个 Python 文件组织多个 `Chapter`，放入 `Project(chapters=[...])`。每个步骤使用 `Say/Show/Choice/...` 构造函数。先登场再发言／移动；跨章用 `End(next_script=...)`；同组子动作用 `.action()`，不带 id/goto。构造函数检查参数，完整关系由 Rust 校验。

可选的 `NodeIds` 生成不重复编号，`Staging` 简化登退场、并行动作与站位恢复；它们返回普通节点，不替代 `Say`、`Show`、`Parallel` 等接口。`restore` 只复用作者通过 `enter`／`remember` 明确记录的 Show 参数，不读取游戏当前站位，也不因中途 Move 自动更新记录。

按任务选择配置对象：

- 事件任务：Task/Event/Scenario、Condition/All/Any/Not、Outcome/VariableChange。
- 译文：Localization/Translation，路径通过 texts 查询，保留 ID 和资源路径。
- 素材：ImageAsset/CharacterAsset/AudioAsset；其余类型用 Asset。节点引用 `.ref`，不把本地绝对文件路径写进演出字段。
- 路线验收：RouteTest/ChoiceStep 写入章节 `_editor.tests`。option 从0开始，同一 choice 的重复动作按顺序消费，预期写 assert。

执行 `cli.build(project, 新工程目录)`；需要包时再执行 `cli.export(...)`。成功结果应包含原生体检、编译和工程／包重开检查。遇到 CLIError 读取 `.result` 的 issues/errors，按 story_id/node_id 修改；不要捕获异常后继续宣称成功，也不要伪造初始旗标来掩盖警告。

修改已有作品先 `Project.open(cli, path)`，保留 source、campaign_id、节点元数据及资源。recipe 没有 assets 键时，资源仍由 source 读取。删除章节／事件／资源后另存新目录。不要通过手写 Lua 或重算校验记录绕过包的源码归属检查。

## 演出节奏与站位

先确定一段戏中在场人物的相对位置、朝向和互动关系，以稳定构图为默认。演出服务于剧情，不按对白句数分配动作；平静交谈允许连续多句保持原位。

1. **位移要有剧情原因。** 走近、退避、让路、追赶、受击等确实改变空间关系时才安排移动，终点应能读出这个变化。不要为了“有演出”在每句前后微调坐标、反复进退或小幅缩放；换人说话、普通情绪变化不自动触发位移。仅需换表情时保留现有站位，不重复登场或调用站位恢复助手把人物拉回原处。小动作有明确表意作用时可以使用，但不能用全身漂移冒充肢体表演。
2. **动作时长、完成等待和结果停留分别考虑。** 查对应节点的实际等待语义，不能只填 `duration` 就假定下一句会等待。明确需要完成后再说话／切镜时，使用节点支持的 `wait_for_completion` 或适当的等待步骤；已等待完成的动作不要再重复等待整段时长。再按需要用 `wait_after` 等留出看清结果、迟疑或另一人反应的一拍，不给所有节点机械追加同一固定延时。有因果先后的动作串行，只有确实同时发生的动作才用 `Parallel`；衔接要等需要完成的动作中最晚结束者，不能只等最快的一个。对白已有的阅读停留也要计入节奏，避免刚走完、刚转完或音效尚未收束就抢进下一句／换场。
3. **人物距离由互动和实际轮廓决定。** 先看实际立绘、裁切、缩放和镜头，再安排相对站位；舞台锚点不是脚底，两个锚点的差值不等于两个人的身体间距，半身／全身也不能由锚点推断。普通交谈应有可见的身体间隔，避免无意贴身或遮住对方脸；并肩、对峙、亲近、递物、搀扶按各自关系调整，接触前有合理接近，不能隔着半个画面声称已接触。保持同一空间中人物比例和站位连续；需要远近层次时有明确构图依据。透明边界只作粗略参考，头脸、肩身及衣袖的遮挡仍要实际看图；素材不能表现的肢体动作可用文字、构图或镜头交代。

涉及双人／多人站位、接近或退开时，用可用的素材查询与舞台图检查动作前后构图；缺少素材或未看图的距离判断要标明未验证。节奏要看目标自动播放速度下的连续演出：单张截图不能证明等待合适，编译、体检及状态模拟通过也不算画面或节奏验收。检查时重点找无动机的小位移、被下一句抢断的动作、与互动不符的距离，不要求逐节点额外生成报告。

`cli.query_assets(...)` 返回素材缩略图及可确认的尺寸、裁切与锚点依据；`cli.render_preview(...)`／`render_previews(...)` 实际输出编辑器舞台 PNG、批量帧和联系图，生成后需看图。这些画面是编辑器预览，不是游戏本体截图，也不代表人工已经检查。宽高与可选时间点按配套 CLI 文档填写。

## 检查与交付

按实际内容选择体检和路线测试。检查关键分支、循环出口与结局；`unsupported` 不算通过。需要确认编辑器可读时使用 `--smoke-preview`；用户希望直接检查界面时用 `cli.open_editor(project_or_package, output=可选新目录, script=章节, node=步骤)`。

`story_entry` 用 `Event(..., target=原版目标, from_=可选来源)` 表达。区分单章演出、入口条件和原版前置→MOD→原版续接三种检查；单章预览不能发现原版遗留界面。有对应 CLI 时可先用 `prepare-event-playtest` 离线准备，实机授权范围内用 `playtest-event` 的 predecessor 模式实际播放原版起点；boundary 仅直达入口，不算完整衔接。测试状态显式指定，不为保证触发而猜旗标或修改条件、概率和优先级；状态确认也不能代替看连续画面。

资源读取、位置调整和试玩包准备可用 `cli.operation(...)` 调用 Rust 命令。试玩返回的 `request_id` 可用 `cli.playtest_status(game, request_id)` 查询一次，区分请求发出、游戏接收、章节与节点进入；收到请求不等于完整路线或画面验收通过。

作者要求生成游戏原生视频时，用 `cli.render_video(project, game, output, **options)`：默认独立桌面后台渲染，按游戏自动播放时长和预设选项输出 MP4，不切换当前桌面；它仍会启动独立游戏进程并使用 GPU。当前 Host 不绘制可见或隐藏画面水印。普通工程生成不自动执行 playtest、render-video、capture-effects 或安装游戏组件；作者明确要求相应游戏操作时再执行。

区分构造检查、编译／包检查、离线模拟、编辑器界面与游戏实机证据。剧情合理性、视觉听感、随机结果、战斗与事件调度的最终表现交给作者确认。

交付时给出 Python 文件、工程目录、MOD 路径和本次实际测试结果，简要列出未验证事项。版本发布、提交和推送按用户当前授权执行。
