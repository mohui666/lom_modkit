# lom_modkit 文档

本文档对应 **1.4.9** 的源码与功能。编辑器、编译器由 Rust 提供，C# Host 提供游戏接入。当前改动见 [1.4.9 更新说明](../RELEASE_NOTES_v1.4.9.md)，验证范围见 [当前能力](chs/current_capabilities.md)，下载附件见 [发布记录](https://github.com/mohui666/lom_modkit/releases)。

先按用途选择：[安装与开始](#安装与开始) · [创作专题](#创作专题) · [契约与开发](#契约与开发) · [维护](#维护) · [历史构建与验收](#历史构建与验收) · [译文与兼容入口](#译文与兼容入口)。

当前操作与契约以简体中文 `chs/` 为准。版本历史、旧教程和未同步译文单独注明适用范围；历史通过项不视为当前版本重新验收。

## 作者

### 安装与开始

| 文档 | 内容 |
| --- | --- |
| [Windows 快速开始](chs/windows_quick_start.md) | 完整包内也可阅读的安装、定位与新建指引 |
| [软件使用](chs/software_usage.md) | 新建、章节、步骤、保存、预览与导出 |
| [原版剧情编辑与路线图](chs/software_usage.md#原版剧情补丁149) | 整章导入、条件插入、游戏标记与路线图定位 |
| [当前能力](chs/current_capabilities.md) | 当前功能、最低 Host 需求与已验证范围 |
| [核心功能教程](chs/core_tutorial.md) | v1.3.0 录屏与跟做项目；当前操作以软件使用为准 |
| [Mac 编辑器](chs/macos.md) | 源码构建、启动与素材设置；历史 GUI 记录另行注明 |

### 创作专题

| 文档 | 内容 |
| --- | --- |
| [用户内容](chs/user_content.md) | 音频、人物、图片的导入与引用 |
| [事件与任务配置](chs/events_and_tasks.md) | 两种作品、独立事件任务配置与可重复事件 |
| [1.4 系列自定义玩法与存档](chs/v1_4_development.md) | 自定义内容、三种玩法入口与独立存档归属 |
| [多语言](chs/i18n.md) | 界面语言、资源显示名称和作品翻译 |
| [自动恢复](chs/editor_recovery.md) | 快照位置、恢复流程与保存边界 |
| [自由模式与人物立绘](chs/free_mode_portraits.md) | 赵活外观与旧战役时间／地点触发器 |
| [Showcase 3.0](../samples/showcase3/README.md) | 10 章、67 类节点的完整样例与生成入口 |

## 契约与开发

| 文档 | 内容 |
| --- | --- |
| [Mod 格式 v3](chs/mod_format.md) | 包结构、节点、Lua 约定、运行时行为 |
| [脚本 / API 索引](chs/script_reference.md) | 字段契约、受控 API 与相关参考的入口 |
| [受控 API / CLI](chs/ai_cli.md) | Rust API、JSON 请求、校验、编译、打包 |
| [来源水印](chs/watermark.md) | 协议、嵌入及截图／视频检测；不是作者签名 |
| [反编译接口](chs/decompiled_api.md) | 查原版类型的步骤与已用入口 |
| [Gameplay API 矩阵](../research/gameplay_api.md) | 已确认签名与禁止项 |
| [当前确认的接口片段](../research/gameplay_api_contract.json) | 与 API 笔记配合查阅；本机反编译归档不作为可分发源码入口 |

字段定义由 [authoring.json](../rust/lom-editor/data/authoring.json) 提供，Rust 表单与核心编译器共用；包／剧情／内容契约为 **3 / 2 / 1**，具体字段以格式文档和校验器为准。

涉及 Combat、Battle、存档、商店或原版 UI 时，先读 [AGENTS.md](../AGENTS.md) 和反编译接口文档。`docs/research/decompiled/` 是旧归档，包含部分 Combat 类型快照，但没有完整 Battle/Combat 程序集反编译结果，不能作为当前游戏接口证据，也不得上传完整反编译源码。

## 维护

| 文档 | 内容 |
| --- | --- |
| [Rust 工具端](chs/rust.md) | 构建、CLI、程序结构与源码运行 |
| [维护者手册](chs/maintainers.md) | 恢复、模板、体检、发布、Runtime 安装/回滚 |
| [测试入口](chs/test_matrix.md) | 当前测试命令、依赖条件与验证层次 |
| [Runtime 说明](../runtime/MortalModHost/README.md) | C# Host 行为与限制 |

## 历史构建与验收

以下文档保留各轮次的环境、数字与限制。查询当前版本是否通过某项验证，先看更新说明和当前能力；不要累计不同轮次的测试数字。

| 记录 | 对应范围 |
| --- | --- |
| [Rust 迁移记录](chs/rust_migration.md) | 2026-10-04 至 05 的迁移范围、63 类型基线与分平台验收 |
| [Windows 验证记录](chs/windows_validation.md) | 2026-10-04 至 05 的 Rust 构建、离线与实际界面验证 |
| [x64 验证记录](chs/x64_validation.md) | 2026-10-17 的 x64 安装、真实游戏冒烟和发布包校验 |
| [Windows 构建记录（2026-10-20）](chs/windows_postrelease_build_20261020.md) | 当时的两端路径、构建结果与测试入口 |
| [Windows 部署记录（2026-10-21）](chs/windows_ui_deployment_20261021.md) | 当时的界面、一键配置、完整 ZIP 与验证范围 |
| [Windows 完整测试清单（2026-10-22）](chs/windows_integrated_checklist_20261022.md) | 86 项手动检查基线，保留旧 A～H 项 |
| [完整测试结果模板（2026-10-22）](chs/windows_integrated_test_results_20261022.md) | 对应清单的结果、缺图与复现记录模板 |
| [事件／任务 Windows 整合验证](chs/windows_event_task_merge_20261025.md) | Host 1.1.3、263 项 Rust、纯 C#、示例及编辑器 GUI，未启动游戏 |
| [Windows 旧版测试清单](chs/windows_postrelease_test.md) | v1.2.1 及相关提取、外观、同时执行、黑幕与 TEMP 检查 |
| [Windows 临时目录权限修复](chs/windows_temp_permissions.md) | 当时的 TEMP 拒绝访问及权限复验 |
| [Windows 回归测试包](../tools/windows_test_kit/README.md) | 本机测试包用法；网页为本地产物，不随源码提供 |
| [原路线图的完成情况](chs/current_capabilities.md#原路线图的完成情况) | 原计划 1.5 纳入 1.4.2 的实现与验证记录 |
| [v1.3.0 变更记录](https://github.com/mohui666/lom_modkit/releases/tag/v1.3.0) | 事件任务、界面、定位游戏与发布验证 |
| [v1.2.0 变更记录](https://github.com/mohui666/lom_modkit/releases/tag/v1.2.0) | Rust 变更及迁移前历史 |

## 译文与兼容入口

[繁體中文](cht/README.md) · [日本語](ja/README.md) · [한국어](ko/README.md)。三种语言的索引链接当前简中指南；目录内旧译文只作对应历史版本与术语参考。

以下文件已合并为短跳转页，保留原路径供已有链接使用：

| 兼容入口 | 当前正文 |
| --- | --- |
| [项目模板](chs/project_templates.md) | [维护者手册：章节、模板与素材](chs/maintainers.md#章节模板与素材) |
| [项目统计](chs/project_statistics.md)、[语音覆盖](chs/voice_coverage.md)、[发布体检](chs/release_preflight.md) | [维护者手册：检查、统计与发布](chs/maintainers.md#检查统计与发布) |
| [发布构建](chs/release_builder.md) | [维护者手册](chs/maintainers.md) |
| [安装诊断](chs/runtime_installation_doctor.md)、[Runtime 回滚](chs/runtime_rollback.md) | [维护者手册：Windows 游戏接入](chs/maintainers.md#windows-游戏接入与-runtime-回滚) |
| [水印协议](chs/watermark_protocol.md)、[截图检测](chs/watermark_detector.md)、[视频检测](chs/watermark_video_detector.md) | [来源水印](chs/watermark.md) |
