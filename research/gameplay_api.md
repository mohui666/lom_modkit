# Gameplay API 实证矩阵

Phase 49 使用本机 Steam《活侠传》当前安装的 Managed 程序集，通过 `ilspycmd 8.2.0.7535` 只读反编译。程序集的大小、SHA-256、类型和必需源码片段记录在 `gameplay_api_contract.json`；可运行：

```powershell
python tools/verify_gameplay_api.py --json
```

该命令不写游戏目录。游戏更新导致程序集哈希或签名变化时会失败，必须重新审计，不能静默沿用旧结论。

本页和 `gameplay_api_contract.json` 是我们自己整理的接口摘要，可以进仓库。
用 `ilspycmd` 从游戏 DLL 转出的整份 `.cs` **不要上传**（见根目录 `AGENTS.md`、`.gitignore` 里的 `docs/research/decompiled/`）。

## 已确认

| 能力 | 原版证据 | 可实现范围 |
| --- | --- | --- |
| 场景战斗 | `LuaManager.ChangeScene(name,key,nextScene)` 写入 `CurrentSceneKey` / `CurrentNextScene`；`CombatLevelConfig.Get()` 是 Combat 关卡的实际解析入口 | Battle 继续选择原版地图；Combat 使用运行时临时 ID，由 Host 即时组装隔离关卡，结束后回 Story，不借用固定原版决斗配置 |
| 场景切换就绪 | `LuaManager.Init()` 先 `StartCoroutine(UnloadLoading())`、不等待便立即 `ExecuteLuaScript()`；`SceneController` 在目标场景激活后仍以 `_currentLoadingScene` 记录 `Loading1` | Combat/Battle 由 Host 延迟切换：同时等待 `IsPrepare=false`、`IsLoading=false` 且 `_currentLoadingScene` 为空，再调用原版 `ChangeScene`，避免章节首节点与 Story 的异步卸载冲突后永久停在“读取中” |
| Combat 结果 | `CombatManager.GameOver(bool win)` | `win` / `lose`；原版先应用关卡 WinResult/LoseResult，再 `LoadNextScene()` |
| Combat 对手配置 | `CombatManager` 读取 `CL_<key>.EnemyStat` 并调用 `CombatActionController.SetStat(CombatStat)`；人物与动画的真实关系是 `CombatLevel.EnemyStat.Name` → `CombatStat.CombatAvatar`，资源目录不是主键；`CombatStatUI.Setup()` 直接从 `_statData.GetNameKey()` 写顶部姓名；Story 尚在时 `CharacterPlaceholder._config.Get(id)` 可取得本地化名与 normal 立绘 | Host 即时创建隔离 `CombatLevel` 并清空原版事件/结果/作者数值；官方人物按 `EnemyStat.Name` 精确取得 Name、四帧、详情头像和 HeadImage，不再猜资源路径。确无原生四帧时才用该人物 normal 并适配 Combat 占位边界；自定义人物使用包内动画。所有姓名入口绑定同一人物 |
| Combat 独立背景 | `StoryViewImage._viewAddressableData.GetByKey(view)` 可解析官方 view 的 Addressables 地址；`CombatManager` 最终把 `CombatLevel.BackImage` 同时赋给 `_backSprite` / `_backImage` | Story 卸载前冻结作者选择的官方 view 地址，Combat 加载后独立写入两个原版背景渲染器。人物、背景、数值互不决定，不继承临时关卡技术依赖的背景 |
| Battle 结果 | `GameLevelManager.ShowGameOver(GameOverType,bool)` | `finish=true` 的 `FriendWin` / `EnemyWin` 可继续；`PlayerDie` 只给重试/标题，不伪造可继续分支 |
| Battle 模板 | `GameLevelManager.Setup()` 查找 `BL_<CurrentSceneKey>` | 只能引用游戏内已有 BattleLevel，不动态造地图、Prefab 或 AI |
| Battle 双方配置 | `GetFriendPeople/GetEnemyPeople`、`NpcSpawner.InitNpcList`（`prefab.name` 字典）、`ReadyPanel.Setup`、`CharacterHealth`/`HealthData` | 各阵营 `id:people` 相加为总人数；标题写 ReadyPanel；血量克隆 HealthData。具名角色先对 BattleLevelConfig 预设，没有则用 catalog 里已核实的 Boss/Npc Animator。不改官方资产 |
| 战斗技能 | `SetPlayerBattleSkill` / `SetBattleSkillActive` / `SetBattleSkillLevel` / `ResetBattleSkill` | 可做战前技能配置；尚无已确认的通用临时状态效果 API |
| 商店库存 | `ShopDatabase` 公共 Books/Miscs/Specials 列表、`ShopItem(ItemData,int)`；`ShopPanel.Init()` 只枚举这三类 | Host 在包指纹绑定的受控会话中临时替换库存并复用官方 ShopPanel；结束或故障恢复原库存。Consumables 不在该面板买入列表中 |
| 单品价格 | `ShopPanel.AddBuyPanel` 调 `ItemData.GetBuyPrice(discount)`；`Open(int)` 只把 0/非0 映射为原版关系折扣/统一 50% 折扣 | 原版没有公共逐商品改价入口；`custom_shop` 只暴露 0/1 原版折扣，拒绝 `price` |
| 属性/好感/物品/天赋检定 | `LuaManager.GetStatData`、`Relationships.Get(type).Value`、`ItemDatabase.HasItem`、`Talents.Get(id).Level` | `stat_check` 直接调用原版 LuaManager；其余缺少 Story Lua 入口的读操作由 Host 提供严格只读 bridge，未知 id fail-closed |
| 存档 | `SaveSystem.CurrentSlot` / `SetSlot` / `SaveGameData()` / 私有 `AutoSaveData(string)` / `AutoLoadGameData(string)` / `SaveUniverseData()` | 手动槽为 `mod_<id>`，三类自动槽重定向为 `mod_<id>_auto*`；Universe 保存时临时隐藏 MOD 槽并保持最后一个原版槽。Host Int32 sidecar 仍与 MOD 槽绑定，不修改 GameSave schema |
| 标题读档 UI | `TitleManager._slotPanel/OpenSlot()`；`LoadGamePanel._saveSlots/CreateSlot/SetButtonNavigation`；`LoadSlotPanel` 的原版 Text/Button 字段 | 临时接管原版槽位显示已有 MOD 存档和“新战役”入口；关闭时调用原版重建逻辑恢复 001～020，不复制或仿造官方素材 |
| 结算展示 | 已验证剧情入口 `mainui.DisplayMessageText`，以及 `reward` 已使用的属性/好感/天赋/物品/旗标原子接口 | `result_screen` 仅把标题/说明提示与现有奖励顺序组合；不新增、Patch 或猜测结算 UI |
| 自定义骰子 | `DiceMenuDialog.ExecuteRoll(string[],...)` 逐项写入 `CachedButtons[i]`，`UpdateSelection` 按结果序号选中按钮；`DiceCheckResult.ResultCount` 是随机点数与加值之和；当前提取的原版检查点结果档数最大为 4 | 作者直接设置 max、标题、固定加值及 2～4 个结果分段。Host 仅为特殊 MOD 检查名构造结果，继续复用原版 DiceMenuDialog。超过 4 档没有原版按钮容量证据，因此不开放 |

## 明确不支持

- `draw` / `escape`：Combat 没有对应结果，Battle 虽有 `Timeout` 枚举但当前关卡逻辑按双方人数转成 FriendWin/EnemyWin。
- 动态创建 Battle 地图、NPC Prefab、模型、AI、武学动画或战斗机制。
- 把 `mission` 当 Mod Quest：它直接写 `MissionManagerData`，会污染原版任务命名空间。
- 把包哈希、水印或作者自报字段当官方认证。

## 验证等级

- **DECOMPILE VERIFIED**：本页表格和 JSON contract 中的签名、控制流与程序集哈希。
- **AUTO VERIFIED**：后续 Compiler/Editor/Host 单测和 Host 构建。
- **NOT VERIFIED IN GAME**：新增高层 Gameplay 节点在真实战斗、战役、商店中的交互与返回时序；实现后仍需实机验收。
