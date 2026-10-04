# lom_modkit 文档

本文档描述当前 **v1.2.0 Rust 编辑器与编译器**，C# 游戏接入层保留。先读 [软件使用](chs/software_usage.md)，构建见 [Rust 工具端](chs/rust.md)，实现与验证范围见 [迁移记录](chs/rust_migration.md)。

截至 2026-10-04，公开 Release 最新仍是 [v1.1.1](https://github.com/mohui666/lom_modkit/releases/tag/v1.1.1) 旧版，不包含当前 Rust 界面。旧版说明见 [v1.1.1 变更记录](../RELEASE_NOTES_v1.1.1.md)。

权威版为简体中文 [`chs/`](chs/)。[繁體](cht/README.md)、[日本語](ja/README.md)、[한국어](ko/README.md) 的入口已注明适用版本；尚未同步的旧版 UI 指南不能作为当前操作路径。维护文档先更新 `chs/`。

## 作者

| 文档 | 内容 |
| --- | --- |
| [软件使用](chs/software_usage.md) | 新建、章节、步骤、保存、预览与导出 |
| [用户内容](chs/user_content.md) | 音频、人物、图片的导入与引用 |
| [当前能力](chs/current_capabilities.md) | 已实现功能与限制 |
| [Mac 编辑器](chs/macos.md) | Rust 通用外壳、系统设置菜单、本地素材与构建 |
| [多语言](chs/i18n.md) | 界面语言、资源显示名称和作品翻译 |
| [自动恢复](chs/editor_recovery.md) | 快照位置、恢复流程与保存边界 |
| [自由模式与人物立绘](chs/free_mode_portraits.md) | 时间地点事件与赵活外观 |
| [Showcase 3.0](../samples/showcase3/README.md) | 5 章、133 节点、63 类节点的样例与生成方法 |

字段定义由 [authoring.json](../rust/lom-editor/data/authoring.json) 提供，Rust 表单与核心编译器共用；节点契约以格式文档和校验器为准。

## 契约与开发

| 文档 | 内容 |
| --- | --- |
| [Rust 工具端](chs/rust.md) | 构建、CLI、测试与程序结构 |
| [Rust 迁移记录](chs/rust_migration.md) | 功能对应、验证边界与删除/保留范围 |
| [Mod 格式 v3](chs/mod_format.md) | 包结构、节点、Lua 约定、运行时行为 |
| [受控 API / CLI](chs/ai_cli.md) | Rust API、JSON 请求、校验、编译、打包 |
| [反编译接口](chs/decompiled_api.md) | 查原版类型的步骤与已用入口 |
| [Gameplay API 矩阵](../research/gameplay_api.md) | 已确认签名与禁止项 |

涉及 Combat、Battle、存档、商店或原版 UI 时，先读 [AGENTS.md](../AGENTS.md) 和反编译接口文档。`docs/research/decompiled/` 是旧归档，缺 Battle/Combat 程序集，不能作为当前游戏接口证据，也不得上传完整反编译源码。

## 维护

| 文档 | 内容 |
| --- | --- |
| [维护者手册](chs/maintainers.md) | 恢复、模板、体检、发布、Runtime 安装/回滚 |
| [测试入口](chs/test_matrix.md) | 当前命令与历史验证边界 |
| [来源水印](chs/watermark.md) | 协议、嵌入、截图/视频检测 |
| [Runtime 说明](../runtime/MortalModHost/README.md) | C# Host 行为与限制 |
| [v1.2.0 变更记录](../RELEASE_NOTES_v1.2.0.md) | Rust 变更及迁移前历史 |

当前 Rust 验证记录限 macOS、预览和离线测试；没有新增 Windows 或游戏实机验收。历史 Windows 结果只对应其记录的旧版本。
