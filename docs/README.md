# lom_modkit 文档

当前原生工具入口与功能迁移表：[Rust 迁移](chs/rust_migration.md)。

权威版是简体中文 [`chs/`](chs/)。译文：
[繁體](cht/README.md) · [日本語](ja/README.md) · [한국어](ko/README.md)。

改文档先改 `chs/`。维护者手册、水印细节和反编译流程只维护中文。

涉及 Combat / Battle / 存档 / 商店 / 原版 UI 时，先读根目录
[AGENTS.md](../AGENTS.md) 和 [反编译接口](chs/decompiled_api.md)。

当前发布版本：**v1.1.1**。本版修复 F5 的 Steam 启动链路、Release 体检定位，
以及冻结编辑器误打包外部 ICU DLL 的启动故障。
完整变更见 [RELEASE_NOTES_v1.1.1.md](../RELEASE_NOTES_v1.1.1.md)。

## 作者

| 文档 | 内容 |
| --- | --- |
| [软件使用](chs/software_usage.md) | 编辑器流程、决斗/战役字段、读档隔离 |
| [用户内容](chs/user_content.md) | 音频、角色、图片的导入与引用 |
| [当前能力](chs/current_capabilities.md) | 已实现与明确不做 |
| [自由模式与人物立绘](chs/free_mode_portraits.md) | 时间地点事件、中文人物选择、赵活外观 |
| [Mac 编辑器](chs/macos.md) | 液态玻璃、本地素材与 Apple Silicon 构建 |

编辑器「帮助 → 文档」按 `models.NODE_SCHEMAS` 动态生成 63 种节点字段，比静态
列表新。完整可玩样例：`samples/showcase3/`。

## 契约 / 开发

| 文档 | 内容 |
| --- | --- |
| [Mod 格式 v3](chs/mod_format.md) | 包结构、节点、Lua 约定、运行时行为 |
| [story_api / CLI](chs/ai_cli.md) | 校验、编译、打包 |
| [反编译接口](chs/decompiled_api.md) | 查原版类型的步骤与已用入口 |
| [Gameplay API 矩阵](../research/gameplay_api.md) | 已确认签名与禁止项 |
| [多语言](chs/i18n.md) | 编辑器与 Host 的 i18n |

## 维护

| 文档 | 内容 |
| --- | --- |
| [维护者手册](chs/maintainers.md) | 恢复、模板、体检、发布、Runtime 安装/回滚、测试矩阵 |
| [来源水印](chs/watermark.md) | 协议、嵌入、截图/视频检测 |
| [Runtime 说明](../runtime/MortalModHost/README.md) | Host 原则与 fail-closed |

`docs/research/decompiled/` 是旧归档，缺 Battle/Combat 程序集，不要当现网证据。
