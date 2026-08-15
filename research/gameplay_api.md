# Gameplay API 实证矩阵

Phase 49 使用本机 Steam《活侠传》当前安装的 Managed 程序集，通过 `ilspycmd 8.2.0.7535` 只读反编译。程序集的大小、SHA-256、类型和必需源码片段记录在 `gameplay_api_contract.json`；可运行：

```powershell
python tools/verify_gameplay_api.py --json
```

该命令不写游戏目录。游戏更新导致程序集哈希或签名变化时会失败，必须重新审计，不能静默沿用旧结论。

## 已确认

| 能力 | 原版证据 | 可实现范围 |
| --- | --- | --- |
| 场景战斗 | `LuaManager.ChangeScene(name,key,nextScene)` 写入 `CurrentSceneKey` / `CurrentNextScene` | 选择原版 Combat / Battle key 并在结束后回 Story |
| Combat 结果 | `CombatManager.GameOver(bool win)` | `win` / `lose`；原版先应用关卡 WinResult/LoseResult，再 `LoadNextScene()` |
| Battle 结果 | `GameLevelManager.ShowGameOver(GameOverType,bool)` | `finish=true` 的 `FriendWin` / `EnemyWin` 可继续；`PlayerDie` 只给重试/标题，不伪造可继续分支 |
| Battle 模板 | `GameLevelManager.Setup()` 查找 `BL_<CurrentSceneKey>` | 只能引用游戏内已有 BattleLevel，不动态造地图、Prefab 或 AI |
| 战斗技能 | `SetPlayerBattleSkill` / `SetBattleSkillActive` / `SetBattleSkillLevel` / `ResetBattleSkill` | 可做战前技能配置；尚无已确认的通用临时状态效果 API |
| 商店库存 | `ShopDatabase` 公共 Books/Miscs/Specials 列表、`ShopItem(ItemData,int)` | Host 可在受控会话中临时替换库存并复用官方 ShopPanel |
| 单品价格 | `ShopPanel` 私有 `AddBuyPanel` 最终调用 `ItemData.GetBuyPrice(discount)` | 原版没有公共逐商品改价入口；需要 Host 限域 patch，不能宣称已有 API |
| 物品/天赋检定 | `ItemDatabase.HasItem`、`PlayerStatManagerData.Talents` / `PlayerTalentData.Level` | 可由 Host 提供只读 Lua bridge |
| 存档 | `SaveSystem.CurrentSlot` / `SetSlot` / `SaveGameData` | 不修改 GameSave schema；Mod 持久变量应使用与 `mod_<id>` 槽绑定的 Host 原子 sidecar |

## 明确不支持

- `draw` / `escape`：Combat 没有对应结果，Battle 虽有 `Timeout` 枚举但当前关卡逻辑按双方人数转成 FriendWin/EnemyWin。
- 动态创建 Battle 地图、NPC Prefab、模型、AI、武学动画或战斗机制。
- 把 `mission` 当 Mod Quest：它直接写 `MissionManagerData`，会污染原版任务命名空间。
- 把包哈希、水印或作者自报字段当官方认证。

## 验证等级

- **DECOMPILE VERIFIED**：本页表格和 JSON contract 中的签名、控制流与程序集哈希。
- **AUTO VERIFIED**：后续 Compiler/Editor/Host 单测和 Host 构建。
- **NOT VERIFIED IN GAME**：新增高层 Gameplay 节点在真实战斗、战役、商店中的交互与返回时序；实现后仍需实机验收。
