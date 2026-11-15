# 脚本 / API 文档

当前 Rust 编辑器的节点字段由 `rust/lom-editor/data/authoring.json` 定义，界面按类型显示表单与说明。节点范围、逐字段契约、示例和编译约定见下方格式文档；旧版“帮助 → 文档”的菜单路径不适用于当前界面。

仓库内的权威文本：

- [Mod v3 格式与全部编译契约](mod_format.md)
- [受控 API / CLI](ai_cli.md)
- [CLI 请求与参数](cli_parameters.md)
- [多语言契约](i18n.md)
- [当前能力与边界](current_capabilities.md)

`combat` 与 `battle` 必须区分。决斗：人物管姓名和动画，背景独立，数值手填。
战役：各阵营自带人数并与具名角色相加；可填标题和双方基础血量。旧
`friend_people` / 单个 `friend_faction` 已删除。改这些节点前读
[反编译接口](decompiled_api.md)。

官方资源字段优先显示可读名称，同名不同资源附加数字，内部 ID 在高级输入或悬停信息中查看；脚本和包内只保存稳定 ID。用户内容统一使用 `user:<id>`，不得保存本机绝对路径。

1.6.0 可用 `asset-query` 查询本地素材与布局资料，`render-preview`／`render-previews` 输出纯舞台图，`render-video` 按预设路线生成原生游戏视频。视频默认后台生成，仍会启动独立游戏进程；二维预览、模拟通过和实际游戏执行分别报告，不能相互代替。顶层 `--help` 与 `--version` 只输出信息，不打开编辑器窗口。

Python `Event` 支持 `trigger="story_entry"`、`target` 和可选 `from_`，其中 `from_` 序列化为 `from`。`prepare-event-playtest` 离线生成隔离包；`playtest-event` 对原版章节入口事件默认实际播放其原版前置章，并可通过 `playtest-status` 查询执行状态。单章预览不模拟原版遗留界面。完整字段、起点规则和显式 `boundary` 模式见 [CLI 参数](cli_parameters.md)。

支持的旧工程及 `.lommod` 继续可打开、编辑和导出；原文件与稳定身份保留，新导出使用当前生成器。兼容旧编译输出不代表允许任意替换 Lua，包完整性和源剧情归属检查仍保留。

`parallel`（同时执行组）的动作范围、字段和等待规则见 [同时执行组](mod_format.md#34-同时执行组parallel)。普通表单创作可以直接选择动作类型，无需编写协程或 Lua。

姓名与称号共用“修改姓名与称号”节点：选择赵活或其他人物后，姓名、称号可分别保持不变、设置或恢复，称号另外支持隐藏。它作用于后续对白，可跨章并随当前作品存档保存；不需要逐句填写。赵活原有数值“称号”属性仍可使用，恢复原称号时按该属性显示。
