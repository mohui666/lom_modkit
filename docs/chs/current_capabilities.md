# 当前能力与边界

本文按当前 Rust 源码描述能力。节点集合以 [`authoring.json`](../../rust/lom-editor/data/authoring.json) 的 `NODE_SCHEMAS` 为准，当前共 63 种。编辑器、编译器由 Rust 提供，游戏接入仍由 C# `MortalModHost` 提供，包／剧情／内容契约仍为 **3 / 2 / 1**。

2026-10-04 本轮只核对文档与源码入口，没有重跑构建、测试或游戏。下述游戏能力是现有 Host 与接口契约的说明，不能视为 Rust 版已通过 Windows／游戏实机验收。

## 已实现

- 剧情与演出：对白、人物舞台动作、场景、音乐/音效、背景、CG、Overlay、选项、分支、骰子、结局/死亡卡及多章节跳转。
- 用户内容：离线导入和打包 audio / character / image；自定义角色、逐句语音、BGM、SFX、环境音、背景、CG、Overlay。包只收集实际引用内容。
- 战役入口：标题“开始 MOD 战役”复用原版读档槽显示已有 MOD 存档和“新战役”入口；手动槽、三类自动槽、Universe 最近槽和持久变量均与原版隔离。Manifest 支持地点、时间、Flag、好感触发器。
- 原版系统节点：属性、好感、天赋、物品、官方 Flag、Mission、面板、时间，以及下节所列战斗底层节点。
- 创作与发布：多章节、多选与撤销／重做、项目／节点模板、分区／分组、恢复副本、全文查找替换、离线测试、统计与配音覆盖、Editing／Release 体检、本地发布目录及包检查。Windows 代码保留 F5 试玩请求、MOD 管理、安装检查与 C# Host 回滚；Mac 上禁用游戏接入。
- 来源披露：运行时强制非官方标识、整包指纹、画面内来源水印及离线截图/视频检测。它们不是数字签名，无法证明作者身份。

完整样例与生成入口见 [Showcase 3.0](../../samples/showcase3/README.md)。

## 当前桌面界面

- Rust／egui 浅色通用外壳，保留左侧章节和步骤、中间属性、右侧预览三栏；标准标题栏显示作品名称与未保存状态。
- 新建／复制章节先选操作，再在独立模态框填写名称；内部标识自动生成。素材导入先选类型和文件，再确认名称，取消不导入。
- 可选字段直接显示语义明确的默认值，不再用“启用字段”复选框作为入口；内部步骤／章节标识改名放在“编辑 → 高级”。真正的布尔选项仍使用复选框。
- 简体中文、繁體中文、日本語、한국어设置即时生效。Mac 的“设置…”位于屏幕顶部系统应用菜单，快捷键为 `⌘,`；非 Mac 提供窗口内设置入口与 `Ctrl+,`。
- 素材与离线测试草稿纳入统一保存流程；无效输入保留并提示，不能因切换、保存或撤销静默丢失。模态框阻挡背景操作，支持确认／取消及输入法候选保护。

Swift 外壳与原生 Liquid Glass 尚未完成。当前浅色面板和正常系统标题栏不等于玻璃效果验收。

## 战斗能力：只到已验证的底层接口

当前战斗节点会编译到《活侠传》原版接口：

- `enemy`：`ModifyEnemyTeam` / `ModifyEnemyLevel` / `ModifyEnemyPeople` / `ModifyEnemyId`；
- `battle_skill`：`SetPlayerBattleSkill` / `SetBattleSkillActive` / `ResetBattleSkill`；
- `goto_scene`：只用于普通场景跳转，不再向作者暴露 Combat / Battle 场景预设。
- `combat`：人物只决定姓名与四类动画；背景从官方 `views` 独立选。对手 HP、气力、六维、评语、技能、天赋和行动概率由节点填写。赵活的 `player_*` 覆盖写入本场 Combat 基准值，生命仍先按原版 `GameStat.FinalValue`（含体力和被动）结算，再叠加 `player_max_health`，不会覆盖原版加成或重复累加。无独立四帧时回退 normal 立绘并钉在官方待机中心。胜负来自 `CombatManager.GameOver(bool)`。
- `battle`：每个附加阵营单独设人数，该方总人数 = 阵营人数 + 具名角色。可选 `title`（`ReadyPanel`）、`friend_health` / `enemy_health`（克隆 `HealthData`）。具名角色必须能对应原版可生成 preset 或 catalog 里已核实的 Battle Animator。不暴露地图、中立路人和技能预设。
- `battle_result`：按包完整 SHA-256、剧情 id 和可选 Combat/Battle 类型读取 Host 的最后真实结果，只提供已验证的 win/lose 分支。
- `reward`：把现有 `stat` / `affinity` / `talent` / `item` / `flag` 原子接口聚合为 1~32 项奖励。
- `result_screen`：用原版 `mainui.DisplayMessageText` 显示作者填写的结算标题与说明，再逐项执行与 `reward` 相同的现有奖励接口；不创建新的结算 UI。
- `custom_shop`：临时替换原版 `ShopDatabase` 的书籍、杂物、贵重品库存并复用 `ShopPanel`；支持数量、MOD/原版条件和原版统一折扣，关闭或故障时恢复原库存。原版没有公开逐商品价格接口，因此不支持 `price`。
- `stat_check` / `affinity_check` / `item_check` / `talent_check` / `flag_check`：分别读取原版属性、好感、物品、天赋与 MOD/原版旗标后走成功/失败分支；好感、物品、天赋只使用已验证的只读 Host bridge。

工具是在编排原版战斗系统，不包含自研 Battle Engine。接口证据见 [游戏 API 笔记](../../research/gameplay_api.md) 与 [契约片段](../../research/gameplay_api_contract.json)；历史 Host 验收与当前 Rust 工具验收分别记录。此次 Rust 迁移没有运行 Windows 或游戏实机测试；`draw` / `escape` 没有可用结果接口。

## MOD 战役状态

- `mod_quest` / `quest_check` 提供按包完整指纹隔离的任务状态机；它不调用、不污染原版 Mission 系统，并在同一 MOD 战役会话内跨 Story / Free 保留。
- `persistent_var` / `persistent_check` 提供 Int32 持久变量：只允许当前 MOD 的 `mod_campaign_<campaign_id>` / `_sNNN` 隔离手动槽，Host sidecar 与稳定战役身份绑定，并在原版手动/自动存档成功返回后原子落盘；不修改 GameSave schema。缺失值为 0，每包最多 256 项。
- `modflags` / `modvars` 仍是 Story 会话表，不写存档；需要跨重启保存的数值应显式使用 `persistent_var`。

## 尚未实现

- 消耗品目录或逐商品自定义价格；当前 `custom_shop` 严格限于原版 `ShopPanel` 实际展示的三类库存及统一折扣。
- `mod_quest` 跨重启持久化，以及任意 Lua 对象和字符串持久化；普通/F5 官方槽始终不会写入 MOD sidecar。
- 自定义战斗地图、模型、AI、战斗动画、机制或战斗引擎。
- 联网社区内容库、自动上传或发布。

若功能依赖尚未由反编译和实机验证确认的游戏结果或生命周期接口，应先研究并记录结论；不以猜测 API 的方式补齐。

## 验证记录

2026-10-04 上一轮 Mac 验收记录为 **179 项通过、0 失败、2 项默认跳过**，样例为 5 章、133 节点，预览检查为 133 / 133。本轮文档维护仅引用该记录；完整范围、外部依赖与未验证项见 [测试矩阵](test_matrix.md) 和 [迁移验收表](rust_migration.md)。
