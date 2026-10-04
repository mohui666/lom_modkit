# 脚本 / API 文档

当前 Rust 编辑器的 63 类节点字段由 `rust/lom-editor/data/authoring.json` 定义，界面按类型显示表单与说明。逐字段契约、示例和编译约定见下方格式文档；旧版“帮助 → 文档”的菜单路径不适用于当前界面。

仓库内的权威文本：

- [Mod v3 格式与全部编译契约](mod_format.md)
- [受控 API / CLI](ai_cli.md)
- [多语言契约](i18n.md)
- [当前能力与边界](current_capabilities.md)

`combat` 与 `battle` 必须区分。决斗：人物管姓名和动画，背景独立，数值手填。
战役：各阵营自带人数并与具名角色相加；可填标题和双方基础血量。旧
`friend_people` / 单个 `friend_faction` 已删除。改这些节点前读
[反编译接口](decompiled_api.md)。

官方资源字段优先显示可读名称，同名不同资源附加数字，内部 ID 在高级输入或悬停信息中查看；脚本和包内只保存稳定 ID。用户内容统一使用 `user:<id>`，不得保存本机绝对路径。
