# lom_modkit v1.1.0

本版本集中修复 1.0.1 之后的 Combat/Battle 实机问题，并把 MOD 战役存档整理为与原版一致的可用流程。Compiler、Editor 与 MortalModHost 统一为 `1.1.0`。

## 核心更新

- **MOD 独立存档栏位**：每个 `campaign_id` 使用原版风格的 001～020 手动栏位；空栏位可以开始新战役，已有栏位可以覆盖或读取。剧情、自由模式和战斗自动档继续使用同一 MOD 的隔离命名空间。
- **原版存档界面复用**：标题页和游戏内读档入口保持原版槽布局；游戏内入口默认进入当前 MOD 的次级存档页，不改变原版读档 UI。
- **Combat/Battle 读档修复**：保存和读取绑定 MOD、完整包指纹、剧情节点与 Gameplay checkpoint，避免跨包冒名、读到原版槽或恢复到错误战斗。
- **战斗状态稳定**：修复自动读档后回合数递增、战斗数值变化、赵活生命缓存重复叠加以及第 0 回合启动的问题。
- **赵活基准属性覆盖**：支持 `player_*` 与 `player_talents` 覆盖本场 Combat；生命先使用原版体力、属性和被动的最终结算，再叠加 `player_max_health`，不会覆盖原版增益。
- **剧情背景恢复**：跨章节 `next_script` 和剧情自动存档读取时保留 MOD 自定义背景，避免恢复后变成黑色背景或被 Host 清空。
- **战役编排**：Battle 支持可验证的具名角色 roster、人数和生命配置；Combat/Battle 结果回流继续绑定真实原版 win/lose 结果。

## Editor / Compiler

- 更新 Combat/Battle 节点表单、校验、代码生成和字段说明，覆盖玩家基准属性、天赋、战斗 roster 与生命配置。
- Showcase3 更新为 62 节点的完整实机验收包，包含 Combat/Battle、背景、自动存档和新生命规则。
- 发布仓库清理为单一示例入口：仅保留 Showcase3，移除不兼容的历史示例源码、README、`.lommod` 与旧版本发布说明。
- 冻结编辑器、Release Builder、版本校验和多语言文档同步到 `1.1.0`。

## Runtime / 质量

- 增加存档隔离、Gameplay checkpoint、战斗数值和来源包切换的回归测试。
- 继续遵守反编译接口优先规则；不修改官方 ScriptableObject 资产，不上传完整游戏反编译源码。
- Runtime Release 构建、SmokeTest、Gameplay API 校验、编辑器冻结版自检和完整测试矩阵通过。

## 升级提示

旧的 `showcase3` 战斗自动档可能仍保存 `player_max_health=120` 或空背景。升级后请从当前 MOD 的空白栏位开始新战役，重新生成剧情和战斗自动档；不要用旧自动档验证新数值。

## 发布文件

- `lom_modkit-v1.1.0_windows_x64.zip`
- `lom_modkit-v1.1.0_windows_x64.zip.sha256`
