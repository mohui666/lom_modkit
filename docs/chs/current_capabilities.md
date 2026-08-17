# 当前能力与边界

本文按仓库当前代码描述能力，不把研究计划写成已实现功能。节点的唯一权威集合是 `editor/models.py` 的 `NODE_SCHEMAS`，当前共 62 种。

## 已实现

- 剧情与演出：对白、人物舞台动作、场景、音乐/音效、背景、CG、Overlay、选项、分支、骰子、结局/死亡卡及多章节跳转。
- 用户内容：离线导入和打包 audio / character / image；自定义角色、逐句语音、BGM、SFX、环境音、背景、CG、Overlay。包只收集实际引用内容。
- 战役入口：标题“开始 MOD 战役”复用原版读档槽显示已有 MOD 存档和“新战役”入口；手动槽、三类自动槽、Universe 最近槽和持久变量均与原版隔离。Manifest 支持地点、时间、Flag、好感触发器。
- 原版系统节点：属性、好感、天赋、物品、官方 Flag、Mission、面板、时间，以及下节所列战斗底层节点。
- 创作与发布：F5 试玩/热重载/Debugger、Editing 与 Release 体检、恢复副本、项目/节点模板、统计与语音覆盖、本地 Release Builder、安装诊断与 Runtime 回滚。
- 来源披露：运行时强制非官方标识、整包指纹、画面内来源水印及离线截图/视频检测。它们不是数字签名，无法证明作者身份。

可直接打包的完整例子见 `samples/feature_showcase/`。

## 战斗能力：只到已验证的底层接口

当前战斗节点会编译到《活侠传》原版接口：

- `enemy`：`ModifyEnemyTeam` / `ModifyEnemyLevel` / `ModifyEnemyPeople` / `ModifyEnemyId`；
- `battle_skill`：`SetPlayerBattleSkill` / `SetBattleSkillActive` / `ResetBattleSkill`；
- `goto_scene`：只用于普通场景跳转，不再向作者暴露 Combat / Battle 场景预设。
- `combat`：人物选择只决定决斗名称与四类战斗动画；背景从官方 `views` 独立选择。HP、气力、`stamina_power`（内力）、`internal`（阴阳／内功倾向）、其余能力、天赋及行动概率均自由填写且不会自动带入。原版从不读取的三格绝招字段已删除，只保留实际生效的 `ultimate_rate`。Host 从 `CombatManager.GameOver(bool)` 取得真实 win/lose，原版资产不被修改。
- `battle`：只设置敌我阵营、双方总人数以及已核实的官方具名角色；具名角色计入总人数。地图与阵容基线是 Runtime 内部细节，不暴露中立阵容、NPC HP 或技能预设。
- `battle_result`：按包完整 SHA-256、剧情 id 和可选 Combat/Battle 类型读取 Host 的最后真实结果，只提供已验证的 win/lose 分支。
- `reward`：把现有 `stat` / `affinity` / `talent` / `item` / `flag` 原子接口聚合为 1~32 项奖励。
- `result_screen`：用原版 `mainui.DisplayMessageText` 显示作者填写的结算标题与说明，再逐项执行与 `reward` 相同的现有奖励接口；不创建新的结算 UI。
- `custom_shop`：临时替换原版 `ShopDatabase` 的书籍、杂物、贵重品库存并复用 `ShopPanel`；支持数量、MOD/原版条件和原版统一折扣，关闭或故障时恢复原库存。原版没有公开逐商品价格接口，因此不支持 `price`。
- `stat_check` / `affinity_check` / `item_check` / `talent_check` / `flag_check`：分别读取原版属性、好感、物品、天赋与 MOD/原版旗标后走成功/失败分支；好感、物品、天赋只使用已验证的只读 Host bridge。

这意味着工具是在编排原版战斗系统，不包含自研 Battle Engine。高层 `combat` / `battle` 已有经反编译确认的结果回流，但尚未实机验证；`draw` / `escape` 没有可用结果接口。

## MOD 战役状态

- `mod_quest` / `quest_check` 提供按包完整指纹隔离的任务状态机；它不调用、不污染原版 Mission 系统，并在同一 MOD 战役会话内跨 Story / Free 保留。
- `persistent_var` / `persistent_check` 提供 Int32 持久变量：只允许当前 `mod_campaign_<campaign_id>` 隔离槽，Host sidecar 与稳定战役身份绑定，并在原版 `SaveGameData` 成功返回后原子落盘；不修改 GameSave schema。缺失值为 0，每包最多 256 项。
- `modflags` / `modvars` 仍是 Story 会话表，不写存档；需要跨重启保存的数值应显式使用 `persistent_var`。

## 尚未实现

- 消耗品目录或逐商品自定义价格；当前 `custom_shop` 严格限于原版 `ShopPanel` 实际展示的三类库存及统一折扣。
- `mod_quest` 跨重启持久化，以及任意 Lua 对象和字符串持久化；普通/F5 官方槽始终不会写入 MOD sidecar。
- 自定义战斗地图、模型、AI、战斗动画、机制或战斗引擎。
- 联网社区内容库、自动上传或发布。

若功能依赖尚未由反编译和实机验证确认的游戏结果或生命周期接口，应先研究并记录结论；不以猜测 API 的方式补齐。
