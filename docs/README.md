# lom_modkit 文档

当前版本为 **1.4.19**，编辑器和编译器使用 Rust，C# Host 提供游戏接入。更新内容见 [本版说明](../RELEASE_NOTES_v1.4.19.md)，下载见 [发布页](https://github.com/mohui666/lom_modkit/releases/latest)。

## 开始使用

| 文档 | 内容 |
| --- | --- |
| [Windows 快速开始](chs/windows_quick_start.md) | 安装、升级、配置游戏和提取素材 |
| [软件使用](chs/software_usage.md) | 章节、分组、步骤、气泡、预览和导出 |
| [原版剧情编辑与路线图](chs/software_usage.md#原版剧情编辑与路线图) | 整章导入、条件插入、游戏标记与定位 |
| [核心功能教程](chs/core_tutorial.md) | 从创建项目到事件、任务与发布 |
| [Mac 编辑器](chs/macos.md) | 源码构建、启动与素材设置 |
| [当前能力与边界](chs/current_capabilities.md) | 功能范围、Host 需求和本版验证范围 |

## 创作专题

| 文档 | 内容 |
| --- | --- |
| [用户内容](chs/user_content.md) | 导入音频、人物、图片和原版人物补绘 |
| [原版特效与音效常用组合](chs/effect_sound_combinations.md) | 按打击、格挡、暗器、内力和环境选择现成搭配 |
| [事件与任务](chs/events_and_tasks.md) | 触发条件、重复事件、任务状态和结果动作 |
| [自定义玩法与存档](chs/custom_gameplay.md) | 物品、消耗品、招式、三种玩法与独立存档 |
| [多语言](chs/i18n.md) | 界面、资源名称与作品翻译 |
| [自动恢复](chs/editor_recovery.md) | 恢复位置、流程和保存边界 |
| [自由模式与人物立绘](chs/free_mode_portraits.md) | 赵活外观与旧战役触发器 |
| [Showcase 样例](../samples/showcase3/README.md) | 10 章、67 类节点与生成入口 |

## 契约与开发

| 文档 | 内容 |
| --- | --- |
| [Mod 格式 v3](chs/mod_format.md) | 包结构、字段、节点和运行时约定 |
| [脚本与 API 索引](chs/script_reference.md) | 字段契约与受控接口 |
| [受控 API 与 CLI](chs/ai_cli.md) | JSON 请求、校验、编译和打包 |
| [来源水印](chs/watermark.md) | 嵌入、来源判定和截图／视频检测 |
| [反编译接口](chs/decompiled_api.md) | 查询当前游戏类型和已用入口 |
| [Gameplay API 笔记](../research/gameplay_api.md) | 已确认签名与使用边界 |
| [接口片段](../research/gameplay_api_contract.json) | 与 API 笔记配合查阅 |

包／剧情／内容契约为 **3 / 2 / 1**。编辑表单见 [authoring.json](../rust/lom-editor/data/authoring.json)，字段校验见 [schema.json](../rust/lom-core/data/schema.json)。游戏功能的修改规则见 [AGENTS.md](../AGENTS.md)。

## 构建与维护

| 文档 | 内容 |
| --- | --- |
| [Rust 工具端](chs/rust.md) | 构建、模块和 CLI |
| [维护者手册](chs/maintainers.md) | 保存、模板、体检、Runtime 安装、回滚与发布 |
| [测试入口](chs/test_matrix.md) | 验证命令、依赖条件和层次 |
| [Runtime 说明](../runtime/MortalModHost/README.md) | C# Host 行为、构建与测试 |
| [Windows 测试包](../tools/windows_test_kit/README.md) | 可编辑测试项目与人工检查清单 |

## 其他语言与版本历史

[繁體中文](cht/README.md) · [日本語](ja/README.md) · [한국어](ko/README.md)。当前操作和契约以简体中文指南为准，各语言索引标注译文适用范围。

仓库保留本版更新说明。历史更新内容和附件从 [GitHub Releases](https://github.com/mohui666/lom_modkit/releases) 查阅。
