# 反编译接口

> 权威流程。改 Combat / Battle / 存档 / 商店 / 属性 / 场景 / 原版 UI 之前必读。
> 仓库规则见根目录 [AGENTS.md](../../AGENTS.md)。

本仓库**不实现自研战斗引擎**。凡是调用《活侠传》的功能，入口必须是当前安装里
已经反编译确认的类型与方法。

## 先读哪份

| 材料 | 用途 |
| --- | --- |
| [`research/gameplay_api.md`](../../research/gameplay_api.md) | 已确认能力、可实现范围、明确不支持项 |
| [`research/gameplay_api_contract.json`](../../research/gameplay_api_contract.json) | 程序集大小、SHA-256、必须仍存在的源码片段 |
| 本机 `Mortal_Data/Managed/*.dll` | 当前游戏的真实接口；以它为准。用 `ilspycmd` **只读**查询，结果留在本机 |
| [`docs/research/decompiled/`](../research/decompiled/) | 本机归档，**已 gitignore，不要上传**。缺 Battle/Combat，也不能当现网证据 |

游戏更新后先跑：

```powershell
python tools/verify_gameplay_api.py --json
```

哈希或片段对不上：停下来重新审计，不要沿用旧结论。

## 怎么查一个类型

```powershell
$managed = "C:\Program Files (x86)\Steam\steamapps\common\LegendOfMortal\Mortal_Data\Managed"
ilspycmd -l c "$managed\Mortal.Battle.dll"
ilspycmd -t Mortal.Battle.ReadyPanel "$managed\Mortal.Battle.dll"
ilspycmd -t Mortal.Combat.CombatEnemyController "$managed\Mortal.Combat.dll"
```

只读反编译。不要把反编译结果写回游戏目录，也不要 commit / 打进 Release。
可以上传的是我们自己写的接口摘要（本页、`research/gameplay_api.md`、contract
哈希），不是游戏程序集转出来的整份 `.cs`。

## 已经用过的入口（摘要）

这些是 Host 实际挂钩的原版入口，细节以当前 DLL 为准：

| 能力 | 原版入口 |
| --- | --- |
| 决斗胜负 | `CombatManager.GameOver(bool)` |
| 决斗数值 | `CombatActionController.SetStat` → `CombatStatController.SetStat` |
| 决斗赵活覆盖 | 战前 `PlayerStatData.Set` / `Talents.Set` 写入基准值；`SetPlayerStat` 经 `_playerTotalHealth` 读取 `GameStat.FinalValue` 换算血量，随后原版 `InitCombatRound` 执行 `InitSkill`。`player_max_health` 是额外基础值，必须在 `InitSkill` 后累加在这个原版结果上并保留 Combat `ModifyList`，不能覆盖体力和被动加成。`CombatStatItem` 必须在 `SetStat` 后按 `CombatStat.MaxHealth` 重建。战后把快照写回 GameStat/Talent，不写 `SaveGameData` |
| 决斗四帧 | `CombatEnemyController.SetData` + `Animator.Play` |
| 决斗详情滑条 | `CombatCharacterStatusUI.SetSliderValue`：评语只走官方 `GameStat.LevelText`；填充按 CombatStat 100 重画 |
| 决斗六维雷达 | `CombatStatController.SetStat` 写入 `CombatStatItem`；`UpdateRadarStat` 分母固定 100。玩家页来自 `SetPlayerStat` 的 `GameStat.FinalValue`，`GameStat.Max` 官方默认 100 |
| 战役人数 | `BattleLevel.GetFriendPeople` / `GetEnemyPeople` |
| 战役刷怪 | `NpcSpawner.Setup` → `InitNpcList`（`prefab.name` 做 Dictionary 键）；生成后只改 `Entity._originType`。`ReadyState` 继续跑向预制体自己的 `ReadyPoint`，不改出生格坐标。具名角色按 catalog 身份匹配；`special401` 故事名是王二壮、资源目录是毛二壮，两边都认。catalog Animator 套底板后必须重新激活实例 |
| 战役标题 | `ReadyPanel.Setup` 写 `EnemyTeam/<NameKey>` |
| 战役血量 | `CharacterHealth.MaxHealth` = `HealthData.Health` + 转换加值 |
| 战役结果 | `GameLevelManager.ShowGameOver` 且 `finish=true` 的 FriendWin / EnemyWin |
| 战役暂停读档 | `PausePanel.LoadButtonClick` → `_loadPanel.Show`；MOD 战役复用标题同一套 `LoadGamePanel` |
| MOD 读档页 | 保持已有的 MOD 战役读档页：主手动档加三类自动档；不改原版读档 UI 的槽布局 |
| MOD 保存页 | `SaveGamePanel.OnPanelOpen` / `SaveSlotPanel.Setup`：只显示当前 `campaign_id` 的 `mod_campaign_<id>` 手动槽；确认保存走官方 `SaveGameData(string)`，sidecar 与 Gameplay checkpoint 同步写入该 MOD 槽 |
| 商店 | `ShopDatabase` 三类列表 + `ShopPanel` |

禁止：猜不存在的 `draw`/`escape` 结果、改官方 ScriptableObject 资产、把 Addressables 里的玩家技能 Animator 当成可生成 NPC prefab。
