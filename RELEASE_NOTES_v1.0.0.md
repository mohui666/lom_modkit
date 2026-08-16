# lom_modkit v1.0.0

v1.0.0 是从 v0.7.0 的剧情编辑器扩展为完整 MOD 制作与安全运行工具链的阶段版本。Compiler、Editor 和 Runtime 首次统一为 `1.0.0`。

## 从 v0.7.0 到 v1.0.0

- **可视化编辑**：多章节项目、63 类剧情/舞台/玩法节点、节点图、全局搜索、历史记录、自动恢复、模板、批量检查和 Story CLI。
- **自定义内容**：人物与多表情立绘、介绍卡、称号、体型/朝向，背景与结局图、音乐、音效、对白语音，以及可复用内容包和引用/未使用素材检查。
- **四语支持**：Editor 和 Story 支持简中、繁中、日文、韩文；Runtime 按包内默认语言、回退语言和当前游戏语言选择完整脚本与文本。
- **试玩与诊断**：从选中节点 F5 试玩、演出预览、路径模拟、条件与变量检查、发布前检查、结构化 Runtime 错误、脱敏诊断包和截图/视频来源水印检测。
- **安全包格式**：`.lommod` v2 对路径穿越、绝对路径、重复条目、大小写冲突、文件/目录前缀冲突、条目数量、单项与总解压大小做 Editor/Runtime 双端一致校验；`package-content.sha256` 和 `story-lua.sha256` 防止包内容及 Story/Lua 配对被替换。
- **Runtime 隔离**：不同 MOD 的 Lua 全局状态隔离，异常退出、切换、禁用和卸载时清理生命周期；F9→F8 迁移只执行一次，`Application.runInBackground` 在退出时恢复原值。
- **战役与玩法接入**：在已验证的原版 API 边界内支持 Combat/Battle、战役入口与位置触发器、隔离存档、持久变量、结果回流和非官方来源披露；不改写官方存档 schema，也不伪造战斗结果。
- **自动化质量门**：Runtime 纯 C# 离线测试覆盖异常 ZIP、版本兼容、Lua 隔离和生命周期清理；GitHub Actions 在每次 push/PR 自动运行 Compiler、Editor 和 Runtime 测试。

## v1.0.0 发布文件

- `lom_modkit-v1.0.0_windows_x64.zip`
- `lom_modkit-v1.0.0_windows_x64.zip.sha256`

后续 v1.0.1 将包格式升级为 v3，并要求稳定 `campaign_id`；这项破坏性兼容变化不属于 v1.0.0 本身。
