# lom_modkit 文档

> 语言：简体中文（本文） · [繁體中文](zh_TW/README.md) · [日本語](ja/README.md) · [한국어](ko/README.md)

权威版为简体中文（`zh_CN/`），译文按语言码分目录、同名存放。
改文档先改 `zh_CN/`，再同步译文；约定见 [i18n.md §4](zh_CN/i18n.md)。

## 按读者找文档

| 文档 | 读者 | 内容 |
| --- | --- | --- |
| [mod_format](zh_CN/mod_format.md) | 全部组件开发者 | **v3 契约**：包结构、48 种节点、story→Lua 编译约定、editor_data、运行时行为、story_api 契约、用户内容协议。改代码先改它 |
| [ai_cli](zh_CN/ai_cli.md) | AI 代理 / 脚本作者 | story_api 操作手册：CLI 子命令、--json 字段、Python API 速查、硬性规则、错误对照表 |
| [user_content](zh_CN/user_content.md) | mod 作者 | 用户内容库：导入自定义音频、对白语音、导出与分享、运行时行为 |
| [current_capabilities](zh_CN/current_capabilities.md) | 作者 / 维护者 | 当前已实现能力、仅有底层接口的能力和尚未实现的边界 |
| [release_builder](zh_CN/release_builder.md) | 发布者 | Release 严格体检、本地打包、整包 SHA-256；不自动发布 |
| [runtime_installation_doctor](zh_CN/runtime_installation_doctor.md) | 玩家 / 维护者 | Runtime 离线诊断与确定性安全修复 |
| [runtime_rollback](zh_CN/runtime_rollback.md) | 玩家 / 维护者 | 受管 Runtime 更新失败自动恢复与手工回滚 |
| [test_matrix](zh_CN/test_matrix.md) | 维护者 | Compiler、Editor、GUI 与 Runtime 的离线测试矩阵 |
| [i18n](zh_CN/i18n.md) | 维护者 | 多语言架构：编辑器界面、游戏内 Mod 菜单、名词对照表再生成、文档翻译约定 |

`research/` 是逆向研究材料（反编译脚本等），仅存档，不翻译。

`F6` 是日常 Editing 体检；`Ctrl+F6` 是发布用 Release 严格体检。两者都检查编译、流程、内容引用与数据流问题；Release 另加发布元数据和兼容性规则。未使用内容不会自动删除；自动修复只处理不改变剧情含义的机械问题。

「帮助 → 导出诊断包」生成固定白名单 ZIP：编辑器/Runtime/游戏版本、Manifest、F6 结果、脱敏项目计数，以及限长的编辑器崩溃日志和仅筛选 MortalModHost 的 Runtime 日志。不会遍历或复制剧情正文、用户内容库、Mod、存档、私人目录、无关日志或游戏文件；绝对路径与用户名会替换为占位符。
