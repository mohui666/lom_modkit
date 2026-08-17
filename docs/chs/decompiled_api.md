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
| 决斗四帧 | `CombatEnemyController.SetData` + `Animator.Play` |
| 决斗详情滑条 | `CombatCharacterStatusUI.SetSliderValue`（玩家 `GameStat.Max`，不是 CombatStat 上限） |
| 战役人数 | `BattleLevel.GetFriendPeople` / `GetEnemyPeople` |
| 战役刷怪 | `NpcSpawner.Setup` → `InitNpcList`（`prefab.name` 做 Dictionary 键） |
| 战役标题 | `ReadyPanel.Setup` 写 `EnemyTeam/<NameKey>` |
| 战役血量 | `CharacterHealth.MaxHealth` = `HealthData.Health` + 转换加值 |
| 战役结果 | `GameLevelManager.ShowGameOver` 且 `finish=true` 的 FriendWin / EnemyWin |
| 商店 | `ShopDatabase` 三类列表 + `ShopPanel` |

禁止：猜不存在的 `draw`/`escape` 结果、改官方 ScriptableObject 资产、把 Addressables 里的玩家技能 Animator 当成可生成 NPC prefab。
