# 脚本 / API 文档

编辑器的“帮助 → 文档 → 脚本 / API 文档”由 `editor/models.py` 的实际 schema 动态生成，是 63 种节点逐字段查询的首选入口：每页包含 JSON 键、界面含义、是否必填、类型/枚举、默认值、最小示例和编译后的原版/Host 接口。

仓库内的权威文本：

- [Mod v3 格式与全部编译契约](mod_format.md)
- [story_api / CLI](ai_cli.md)
- [多语言契约](i18n.md)
- [当前能力与边界](current_capabilities.md)

`combat` 与 `battle` 必须区分：前者是原版一对一 Combat 对手覆盖，后者是原版多人 Battle 三方阵容覆盖。`battle_setup` 仅是旧版 Battle 兼容节点，不应当作 Combat 战前配置。

所有官方资源字段在界面中显示“可读名称 + 稳定 ID”，脚本和包内只保存稳定 ID。用户内容统一使用 `user:<id>`，不得保存本机绝对路径。
