# 当前能力与边界

本文按仓库当前代码描述能力，不把研究计划写成已实现功能。节点的唯一权威集合是 `editor/models.py` 的 `NODE_SCHEMAS`，当前共 47 种。

## 已实现

- 剧情与演出：对白、人物舞台动作、场景、音乐/音效、背景、CG、Overlay、选项、分支、骰子、结局/死亡卡及多章节跳转。
- 用户内容：离线导入和打包 audio / character / image；自定义角色、逐句语音、BGM、SFX、环境音、背景、CG、Overlay。包只收集实际引用内容。
- 战役入口：`campaign.new_game` 使用 `mod_<id>` 隔离存档槽；Manifest 支持地点、时间、Flag、好感触发器。
- 原版系统节点：属性、好感、天赋、物品、官方 Flag、Mission、面板、时间，以及下节所列战斗底层节点。
- 创作与发布：F5 试玩/热重载/Debugger、Editing 与 Release 体检、恢复副本、项目/节点模板、统计与语音覆盖、本地 Release Builder、安装诊断与 Runtime 回滚。
- 来源披露：运行时强制非官方标识、整包指纹、画面内来源水印及离线截图/视频检测。它们不是数字签名，无法证明作者身份。

可直接打包的完整例子见 `samples/feature_showcase/`。

## 战斗能力：只到已验证的底层接口

当前战斗节点会编译到《活侠传》原版接口：

- `enemy`：`ModifyEnemyTeam` / `ModifyEnemyLevel` / `ModifyEnemyPeople` / `ModifyEnemyId`；
- `battle_skill`：`SetPlayerBattleSkill` / `SetBattleSkillActive` / `ResetBattleSkill`；
- `goto_scene`：通过 `LuaManager.ChangeScene` 进入原版 `Combat` / `Battle`，并传入作者选择的官方 key。
- `combat`：选择原版 Combat key，可选组合敌方 id/队伍/等级/人数设置；Host 从原版 `CombatManager.GameOver(bool)` 取得真实 win/lose，并一次性续接作者指定节点。失败时仅在该 MOD 战斗会话内把 `DeadEnd` 视为 false，以走原版 `LoadNextScene()` 回 Story。

这意味着工具是在编排原版战斗系统，不包含自研 Battle Engine。高层 `combat` 已有经反编译确认的 win/lose 回流，但尚未实机验证；`draw` / `escape` 没有可用的原版 Combat 结果接口。`battle`、Battle Preset、我方配置编辑器和战后奖励聚合节点仍未实现。

## 尚未实现

- 任意商品目录的 Custom Shop；目前只能打开已验证的原版 `shop` / `newshop` panel。
- 独立 `mod_quest` 状态机；`mission` 操作的是原版 Mission 系统。
- 任意持久化 Mod 变量。`modflags` / `modvars` 是 Story 会话表，不写存档；`game_flag` 写原版已存在的 FlagData。新战役的存档槽虽然隔离，但不会自动把任意 Lua 表持久化。
- 高层 `reward`、`stat_check`、`activity`、`result_screen` 等组合节点。作者可用现有原子节点表达其中一部分，但编辑器尚未提供这些封装。
- 自定义战斗地图、模型、AI、战斗动画、机制或战斗引擎。
- 联网社区内容库、自动上传或发布。

若功能依赖尚未由反编译和实机验证确认的游戏结果或生命周期接口，应先研究并记录结论；不以猜测 API 的方式补齐。
