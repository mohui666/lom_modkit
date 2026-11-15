# lom_modkit 文档

当前版本为 **1.6.0**，编辑器和编译器使用 Rust，C# Host 提供游戏接入。更新内容见 [GitHub Release](https://github.com/mohui666/lom_modkit/releases/tag/v1.6.0)，下载见 [发布页](https://github.com/mohui666/lom_modkit/releases/latest)。仓库维护当前操作与接口文档，历史更新正文仅保留在 GitHub Release。

本版加入多工程标签与恢复、跨窗口步骤复制、后台游戏原生视频和预览分辨率选择；CLI／Python 新增素材查询、舞台图与视频导出、试玩状态查询及可选演出助手。

## 开始使用

| 文档 | 内容 |
| --- | --- |
| [Windows 快速开始](chs/windows_quick_start.md) | 安装、升级、配置游戏和提取素材 |
| [软件使用](chs/software_usage.md) | 多工程标签、步骤、预览分辨率、后台视频和导出 |
| [原版剧情编辑与路线图](chs/software_usage.md#原版剧情编辑与路线图) | 整章导入、条件插入、游戏标记与定位 |
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
| [Showcase 样例](../samples/showcase3/README.md) | 全节点样例、73 类节点与生成入口 |

## AI 与脚本制作

使用 `lom-editor.exe --cli sdk -o sdk --json` 导出配套 Python 包、skill、文档和示例。

| 文档 | 内容 |
| --- | --- |
| [Python 对象 API 与 CLI](chs/ai_cli.md) | 推荐的全节点构造函数接口、批量创作、异常诊断及底层 CLI |
| [Python 接口参考](chs/python_api.md) | 节点、工程、演出助手、素材查询与画面导出 |
| [CLI 命令参数](chs/cli_parameters.md) | 目录筛选、输出摘要、素材、舞台图、原生视频与试玩状态 |
| [AI 剧本制作 skill](../skills/lom-modkit-authoring/SKILL.md) | 给 AI 的接口发现、生成与验收工作流 |

## 契约与开发

| 文档 | 内容 |
| --- | --- |
| [Mod 格式 v3](chs/mod_format.md) | 包结构、字段、节点和运行时约定 |
| [脚本与 API 索引](chs/script_reference.md) | 字段契约与受控接口 |
| [旧媒体来源检测](chs/watermark.md) | 当前水印策略、旧截图与视频的来源检测 |
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

仓库不保留逐版本 note；各版本的更新正文和附件统一从 [GitHub Releases](https://github.com/mohui666/lom_modkit/releases) 查阅。
