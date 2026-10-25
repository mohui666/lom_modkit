# lom_modkit 文档

本文档描述当前 **v1.3.0**，C# 游戏接入层保留。先读 [软件使用](chs/software_usage.md)，构建见 [Rust 工具端](chs/rust.md)，实现与验证范围见 [迁移记录](chs/rust_migration.md)。

[v1.3.0 Windows Release](https://github.com/mohui666/lom_modkit/releases/tag/v1.3.0) 提供完整工具包、中文演示和跟做项目；本次验证见 [v1.3.0 记录](../RELEASE_NOTES_v1.3.0.md)，历史游戏冒烟见 [x64 验证记录](chs/x64_validation.md)。

权威版为简体中文 [`chs/`](chs/)。[繁體](cht/README.md)、[日本語](ja/README.md)、[한국어](ko/README.md) 的入口已注明适用版本；尚未同步的旧版 UI 指南不能作为当前操作路径。维护文档先更新 `chs/`。

## 作者

| 文档 | 内容 |
| --- | --- |
| [Windows 快速开始](chs/windows_quick_start.md) | 完整包内也可阅读的安装、定位与新建指引 |
| [核心功能教程](chs/core_tutorial.md) | 仅软件演示：创建、演出、素材、事件任务、检查、保存与导出 |
| [软件使用](chs/software_usage.md) | 新建、章节、步骤、保存、预览与导出 |
| [用户内容](chs/user_content.md) | 音频、人物、图片的导入与引用 |
| [当前能力](chs/current_capabilities.md) | 已实现功能与限制 |
| [事件与任务配置](chs/events_and_tasks.md) | 两种作品、独立事件任务配置与可重复事件 |
| [Mac 编辑器](chs/macos.md) | Rust 通用外壳、系统设置菜单、本地素材与构建 |
| [多语言](chs/i18n.md) | 界面语言、资源显示名称和作品翻译 |
| [自动恢复](chs/editor_recovery.md) | 快照位置、恢复流程与保存边界 |
| [自由模式与人物立绘](chs/free_mode_portraits.md) | 时间地点事件与赵活外观 |
| [Showcase 3.0](../samples/showcase3/README.md) | 6 章、142 个流程节点、64 类节点的样例与生成方法 |
| [Windows 完整测试清单（2026-10-22）](chs/windows_integrated_checklist_20261022.md) | 保留旧 A～H 全部 49 项，新增 UI、一键配置、MOD 与发布 37 项；共 86 项 |
| [Windows 回归测试包](../tools/windows_test_kit/README.md) | 本机测试包的使用说明；可勾选网页属于另行生成的本地产物，不随源码提供 |
| [完整测试结果模板](chs/windows_integrated_test_results_20261022.md) | 86 项逐项结果、缺图明细与失败复现记录 |
| [Windows 旧版测试清单](chs/windows_postrelease_test.md) | 旧 A～H 原始参考：v1.2.1、提取、外观、同时执行、黑幕与 TEMP |
| [本轮 Windows 构建](chs/windows_postrelease_build_20261020.md) | 2026-10-20 两端路径、实际构建结果与测试入口 |
| [Windows 临时目录权限修复](chs/windows_temp_permissions.md) | TEMP 拒绝访问、无需更新提示与权限复验 |
| [UI 逻辑新版 Windows 部署](chs/windows_ui_deployment_20261021.md) | 四个一级入口、一键配置、完整 ZIP、原测试快捷方式与验证范围 |

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
| [当前确认的接口片段](../research/gameplay_api_contract.json) | 与 API 笔记配合查阅；本机反编译归档不作为可分发源码入口 |

涉及 Combat、Battle、存档、商店或原版 UI 时，先读 [AGENTS.md](../AGENTS.md) 和反编译接口文档。`docs/research/decompiled/` 是旧归档，包含部分 Combat 类型快照，但没有完整 Battle/Combat 程序集反编译结果，不能作为当前游戏接口证据，也不得上传完整反编译源码。

## 维护

| 文档 | 内容 |
| --- | --- |
| [维护者手册](chs/maintainers.md) | 恢复、模板、体检、发布、Runtime 安装/回滚 |
| [测试入口](chs/test_matrix.md) | 当前命令与历史验证边界 |
| [x64 验证记录](chs/x64_validation.md) | 2026-10-17 的 x64 安装、真实游戏冒烟和发布包校验 |
| [事件／任务 Windows 整合验证](chs/windows_event_task_merge_20261025.md) | Host 1.1.3、263 项 Rust、纯 C#、示例及编辑器 GUI，未启动游戏 |
| [Windows 验证记录](chs/windows_validation.md) | 2026-10-04 至 05 的 Rust 构建、离线与实际界面验证 |
| [来源水印](chs/watermark.md) | 协议、嵌入、截图/视频检测 |
| [Runtime 说明](../runtime/MortalModHost/README.md) | C# Host 行为与限制 |
| [v1.3.0 变更记录](../RELEASE_NOTES_v1.3.0.md) | 事件任务、界面、定位游戏与发布验证 |
| [v1.2.0 变更记录](../RELEASE_NOTES_v1.2.0.md) | Rust 变更及迁移前历史 |

当前 Rust 验证包含早期 macOS 记录和本轮 Windows 构建、离线测试与实际界面操作，各自的环境、通过项及限制分别记录。2026-10-17 另完成 x64 游戏启动、F5 与 Combat/Battle 定向冒烟；未逐项验收全部 64 类节点。旧 Python/Qt 的 Windows 和游戏结果只对应当时版本。
