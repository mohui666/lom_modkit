# Windows 事件／任务整合验证（2026-10-25）

目标源码：`C:\Users\mohui666\Documents\ChatGPT\lom-postrelease-20261020\out\event-task-merge-20261025\source`。
整合分支：`codex/windows-event-task-modes-20261025`。
实际日志、示例与成品位于目标源码的上一级目录。

## 整合方式

- 以当前 Windows `checklist-handoff-20261024/source` 的自有文件为维护基线，逐文件保留至隔离工作副本；原目录和旧 `lom_modkit` 脏工作区未修改。
- 注册交付包 preimages 的 Git blob，应用 `handoff/task-changes.patch` 的 57 个文件；没有只合并传入 HEAD，没有整目录覆写，没有提交 preimages 基线。
- 唯一文本冲突为 `workspace.rs` 的素材导入弹窗。保留 Windows 连续追加、去重、同名资源保护、播放条换行及按钮边界，接入统一 egui 弹窗。
- 额外修复：查看可选字段误写 null、窄栏日期和长任务标题溢出、CLI 宿主版本仍为 1.1.2、自动官方主线优先条件、物品减少数量与结算状态大小预检；同步首次使用帮助。

## 本次实际运行

| 层级 | 结果 | 原始记录 |
| --- | --- | --- |
| Rust 工作区 release | 263 通过、0 失败、3 个已有 ignore | `rust-workspace-tests-final.log` |
| 纯 C# SmokeTest + Rust 示例交叉加载 | 39 个共享契约案例、87 条 Scenario 断言通过；Runtime 离线总测试通过 | `csharp-smoke-with-packages.log` |
| Host 实际构建 | MortalModHost 1.1.3；引用本机现有游戏程序集，0 警告、0 错误 | `host-build.log` |
| Showcase | 生成及 inspect 通过，完整性和源码/Lua 一致 | `showcase-build.log`、`showcase-inspect.json` |
| addon / sandbox | 两包生成及 inspect 通过；两种模式的 CLI release 预检通过 | `*-pack.json`、`*-inspect.json`、`*-preflight-final.json` |
| Windows 编辑器 | `scripts/build-windows.ps1 -NoArchive` 完成；随附本次 Host 和 NVorbis；提取器 self-check 通过 | `windows-build-final.log` |

3 个既有 ignore 涉及需本机外部资源或专用环境的测试，未算作通过。首次窄栏回归失败后修复并重跑；首次 Host 构建缺少 `view_map.json`，复用本机现有映射后构建通过。CLI 首次 release 预检实际返回旧宿主不兼容，修复后通过。

## 实际编辑器 GUI

使用本次 Windows 成品，配置和示例均为 `gui/` 下隔离副本；没有更改用户原项目、游戏配置、MOD 或存档。

- 原版附加剧情与自由剧本入口存在；自由剧本新建后有独立开场事件。作品设置位于作品层，保留三栏和“剧情内容／剧情安排”导航。
- GUI 选择第 2 年、7 月、中旬并开启“可以重复发生”、改为“每旬”。真实自动恢复副本中的 `when`、`once=false`、`check=stage` 与操作一致，原有前置任务条件保留。
- 事件结果显示“接下信件／婉言拒绝”以及“已接取／已拒绝”；关联任务按钮可在事件和任务表单间导航。只显示所选项，列表与表单控件对齐。
- 最终日期控件完整显示“不限年份／不限月份／不限旬”；240、280、360 像素窄栏和长中文任务名称另有 Rust 布局回归。
- 创建、导入、帮助和工具结果弹窗 Esc 关闭；标识修改及未保存提示右上角关闭。取消未保存提示后编辑仍在。
- 真实未正常退出副本触发恢复提示；右上角关闭后，7 个恢复文件逐字节未变。
- “游戏与 MOD”页按钮边界与间距保留；未选择真实游戏目录或触发安装。
- 本机 F1 被已有截图工具全局快捷键拦截，已取消截图并用可见“帮助”按钮完成弹窗检查；没有修改全局快捷键。

截图和可访问性记录保存在 `gui/evidence/`。部分操作后的第一帧截图包含淡入动画，稳定帧另有记录。

## 成品与边界

- 编辑器：`windows/lom_modkit/lom-editor.exe`。
- 同目录含 `lomc.exe`、`runtime/MortalModHost.dll`、`runtime/NVorbis.dll`、x86/x64 Doorstop 和自包含素材提取器。
- 示例：`packages/event-addon.lommod`、`packages/event-sandbox.lommod`、`showcase/showcase3.lommod`。
- 官方接口经本机反编译只读核对并通过 Host 编译；未启动游戏，因此调度、存档和奖励的游戏实机行为仍未验收。多个官方结果动作没有整体回滚或严格一次执行保证。
- 没有安装到游戏目录、发布、推送或创建 Release。整合提交 `9507d8d` 记录 61 个任务文件；随后按用户明确授权，将此前保留的 83 个 Windows 基线文件单独纳入版本控制。


## Windows 基线提交

83 个基线文件与整合测试前保留的 `windows-carried-baseline.patch` 完全一致，涵盖既有编辑器、并行动作、游戏配置、素材提取、Windows 构建、本地化及相关测试和文档。本次提交保持这些文件的内容不变，并同步本节记录。

本次仅做清单、补丁内容一致性及 Git diff 检查，没有重新运行 Rust、C#、构建或游戏测试；上面的测试数据仍是此前整合验收的实际记录。提交完成后的分支包含整合功能与这批 Windows 基线源码，复现源码无需再应用该历史保留补丁。构建使用的官方游戏程序集和本机提取素材继续留在原有所有权边界内。
