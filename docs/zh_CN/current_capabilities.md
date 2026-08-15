# 当前能力与边界

本文按仓库当前代码描述能力，不把研究计划写成已实现功能。节点的唯一权威集合是 `editor/models.py` 的 `NODE_SCHEMAS`，当前共 57 种。

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
- `battle`：选择原版 Battle key；Host 只把 `ShowGameOver(FriendWin/EnemyWin, finish:true)` 映射为 win/lose。`PlayerDie(false)` 保持原版重试/标题流程。
- Battle Preset：章节设置中可保存 `combat` / `battle` 原版模板与已验证敌方参数，剧情节点按 ID 复用；编译时展开，不引入新的运行时接口。
- `battle_result`：按包完整 SHA-256、剧情 id 和可选 Combat/Battle 类型读取 Host 的最后真实结果，只提供已验证的 win/lose 分支。
- `battle_setup`：把 `ModifyEnemy*` 与 `SetPlayerBattleSkill` / `SetBattleSkillActive` / `ResetBattleSkill` 组合为战前表格配置。
- `reward`：把现有 `stat` / `affinity` / `talent` / `item` / `flag` 原子接口聚合为 1~32 项奖励。
- `custom_shop`：临时替换原版 `ShopDatabase` 的书籍、杂物、贵重品库存并复用 `ShopPanel`；支持数量、MOD/原版条件和原版统一折扣，关闭或故障时恢复原库存。原版没有公开逐商品价格接口，因此不支持 `price`。
- `stat_check` / `affinity_check` / `item_check` / `talent_check` / `flag_check`：分别读取原版属性、好感、物品、天赋与 MOD/原版旗标后走成功/失败分支；好感、物品、天赋只使用已验证的只读 Host bridge。

这意味着工具是在编排原版战斗系统，不包含自研 Battle Engine。高层 `combat` / `battle` 已有经反编译确认的结果回流，但尚未实机验证；`draw` / `escape` 没有可用结果接口。

## 尚未实现

- 消耗品目录或逐商品自定义价格；当前 `custom_shop` 严格限于原版 `ShopPanel` 实际展示的三类库存及统一折扣。
- 独立 `mod_quest` 状态机；`mission` 操作的是原版 Mission 系统。
- 任意持久化 Mod 变量。`modflags` / `modvars` 是 Story 会话表，不写存档；`game_flag` 写原版已存在的 FlagData。新战役的存档槽虽然隔离，但不会自动把任意 Lua 表持久化。
- 高层 `activity`、`result_screen` 等组合节点。作者可用现有原子节点表达其中一部分，但编辑器尚未提供这些封装。
- 自定义战斗地图、模型、AI、战斗动画、机制或战斗引擎。
- 联网社区内容库、自动上传或发布。

若功能依赖尚未由反编译和实机验证确认的游戏结果或生命周期接口，应先研究并记录结论；不以猜测 API 的方式补齐。
