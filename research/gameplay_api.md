# Gameplay API 实证矩阵

下方既有矩阵来自 Phase 49 的接口核对；`gameplay_api_contract.json` 保留当时的记录，不能把历史程序集摘要作为现版兼容证明。游戏更新后按根目录 `AGENTS.md` 只读反编译受影响类型与方法，不运行基于程序集哈希的复验。

本页和 `gameplay_api_contract.json` 是我们自己整理的接口摘要，可以进仓库。
用 `ilspycmd` 从游戏 DLL 转出的整份 `.cs` **不要上传**（见根目录 `AGENTS.md`、`.gitignore` 里的 `docs/research/decompiled/`）。

## 2026-10-27 人物持续压暗接口核对

当前 Windows 安装的 `Fungus.SayDialog.SetCharacter` 会自动调用 `Stage.SetDimmed`，`PortraitController.Show/ShowPortrait` 还会创建白色 RGB 补间。显式压暗由 `mod_char_dim` 统一记录；补丁在 `PortraitController.SetDimmed` / `StoryStageController.SetDimmed` 保留作者选择，并改写 `LeanTween.color(RectTransform, Color, float)` 的目标 RGB。`LeanTween.descriptions(GameObject)` / `cancel(int)` 仅用于替换 RGB 补间，保留 alpha 淡入淡出和移动。切章、清舞台和结束会话沿现有 `CustomCharacterRuntime.ClearAll` 清理。接口及 Host 构建已核对，实机效果另行验收。

## 2026-10-17 x64 更新核对

本次从本机 Whisky 安装的 `Mortal_Data/Managed/Mortal.Combat.dll`、`Mortal.Story.dll`、`Mortal.Core.dll` 只读检查以下受影响类型，没有计算游戏程序集哈希：

- `Mortal.Combat.CombatStat` 已不含 `HealthAddition`、`BlockAbsorbAddition`、`IgnoreNormalMode`。`Reset()` 仍从 `DefaultHealth`／`DefaultStamina` 初始化当前值，并清零失败率、中毒和麻痹累积。Host 的 `CombatRuntimeLevelPatch.ResetAuthorFacingState` 保留现有初始化，只对实际仍存在的这三个旧字段反射清零；现版不猜测替代字段。操作对象仍是每场克隆的 `CombatStat`，不修改官方 ScriptableObject。
- `Mortal.Story.StoryViewImage.LoadAsset(string)` 仍通过 `_viewAddressableData.GetByKey(name)` 取得 Sprite 地址。
- `Mortal.Core.AddressableCollectionData.GetByKey(string)` 现在取得当前语言并调用 `GetByKey(string,string)`；`AddressableData` 保留 `_key`／`_addressKey`，增加语言覆写，`GetAddressKeyForLang(string)` 可返回覆写地址。本次仅按既有构建流程提取默认背景映射，未改变 Host 的语言选择行为。

Windows 实机还确认 `Mortal.Core.CharacterIntroPanel` 同时提供 `IEnumerator Show(string key)` 与 `IEnumerator Show(string key, int mode)`。Host 的 Lua 桥接继续调用单参数入口，预检和 Harmony 特性显式指定 `string` 参数，修复仅按方法名查找触发的 `AmbiguousMatchException`。

这些是受影响接口与构建依赖核对，不代表所有游戏接口或实机交互已重新验收。创作字段和 `.lommod` 3／2／1 契约不变。

## 2026-10-19 同时执行组接口核对

本次只读反编译当前安装的 `Fungus.LuaEnvironment`、`Mortal.Story.CharacterPlaceholder` 与 `ViewFlowchartController`，并核对上文已用的舞台和黑幕入口。`LuaEnvironment.RunLuaCoroutine` 每帧推进 Lua 协程；现有 Host 的 `HostRunModLua` 同样逐帧推进，并将异常交给中止路径。`CharacterPlaceholder.LoadCharacterAsset(string)` 是可等待的资源加载，`LoadCharacterPortrait(string,string)` 同步填充表情资源；`PortraitController.Show/Hide(PortraitOptions)` 启动补间，`Stage.FadeDuration` 提供默认聚焦／压暗时长；`StoryTransitionBlack.TransitionIn/Out` 返回协程。

新的 `parallel` 编译到 Lua 协作式子协程，复用这些原有入口与既有 `wait/runwait`，不新增 C# Host 回调，不修改官方 ScriptableObject。仅允许演出动作；共享人物／通道／流图冲突会拒绝。使用游戏 DLL 内的 MoonSharp 进行纯解释器离线测试，所有舞台与游戏全局均为合成替身，未运行 Unity 或游戏。真实补间、资源加载和场景返回仍未实机验收。

编辑器黑幕预览补充核对：当前安装的 `Mortal.Story.StoryTransitionBlack.TransitionIn(string)` 将图像填充量推至 1 并隐藏剧情界面，`TransitionOut(string)` 推至 0 后恢复界面；`SetFillOrigin` 将 `rl` 设为 0、`lr` 及其他值设为 1。当前 `UnityEngine.UI.Image.OriginHorizontal` 的 0／1 分别是 Left／Right。持续时间来自场景组件的序列化字段，本次未读取场景资源，因此编辑器使用独立的 1 秒示意，不能作为实际时长或实机兼容证明。没有修改官方组件或包格式。


## 2026-10-17 x64 更新核对

本次从本机 Whisky 安装的 `Mortal_Data/Managed/Mortal.Combat.dll`、`Mortal.Story.dll`、`Mortal.Core.dll` 只读检查以下受影响类型，没有计算游戏程序集哈希：

- `Mortal.Combat.CombatStat` 已不含 `HealthAddition`、`BlockAbsorbAddition`、`IgnoreNormalMode`。`Reset()` 仍从 `DefaultHealth`／`DefaultStamina` 初始化当前值，并清零失败率、中毒和麻痹累积。Host 的 `CombatRuntimeLevelPatch.ResetAuthorFacingState` 保留现有初始化，只对实际仍存在的这三个旧字段反射清零；现版不猜测替代字段。操作对象仍是每场克隆的 `CombatStat`，不修改官方 ScriptableObject。
- `Mortal.Story.StoryViewImage.LoadAsset(string)` 仍通过 `_viewAddressableData.GetByKey(name)` 取得 Sprite 地址。
- `Mortal.Core.AddressableCollectionData.GetByKey(string)` 现在取得当前语言并调用 `GetByKey(string,string)`；`AddressableData` 保留 `_key`／`_addressKey`，增加语言覆写，`GetAddressKeyForLang(string)` 可返回覆写地址。本次仅按既有构建流程提取默认背景映射，未改变 Host 的语言选择行为。
- Windows 当前安装的 `Mortal.Core.CharacterIntroPanel` 经现场反编译确认同时提供 `IEnumerator Show(string key)` 与 `IEnumerator Show(string key, int mode)`；单参数入口转调双参数入口的 `mode=0`。Host 的 `intropanel.Show(key)` 桥接继续使用单参数入口，预检和 Harmony 特性均显式指定 `string` 参数，避免仅按方法名查找触发 `AmbiguousMatchException`、中断整个 Host 初始化。

这些是受影响接口与构建依赖核对，不代表所有游戏接口或实机交互已重新验收。创作字段和 `.lommod` 3／2／1 契约不变。


## 2026-10-24 事件与任务接入

本轮只读核对当前 Whisky 安装（Steam Build 26073097）的 `Mortal.Core`、`Mortal.Free` 与 `Mortal.Story`，没有启动游戏或计算程序集哈希。

- `MissionManagerData.UpdateCheckMissions/HasAnyMissionTrigger`：返回 Free 时保留官方任务优先，只有官方未选中剧情才调度附加自动事件。
- `FreePositionData.GetExecuteScript(float)`、`PositionController.OnPositionClick`：复用现有地图点击及扣点流程；新配置与旧战役触发器分别处理。
- `FreeManager.Start()` 返回 IEnumerator，末尾启动提醒对话协程而不等待；事件入口等待 `IsProcessing=false` 与 `_menuOpen/_saveOpen/_statOpen=false`。
- `LuaManager.ChangeScene(string,string,string)`：演出返回 Free 或显式结束时结算事件；`SetGameTime(int,int,int)` 用于配置中的时间结果。
- `PlayerStatManagerData.StoryKeyList` 由 `Save/Load` 逐项写入/恢复 `StorySave`；宿主只维护自己的命名空间记录，不修改 GameSave schema 或官方 Mission 资产。
- 结果动作复用 `StatModifyManager.Player/SetPlayer/Character/SetCharacter/AddTalent/AddStory/SetFlag/AddFlag`、`ItemDatabase.GetItem/AddItem/RemoveItem`；先检查原版 ID。这些类型均已本轮定向反编译并以当前 DLL 编译，未实机执行。

## 已确认

2026-10-24 全人物表情复扫：本机当前安装为 2026-10-20 构建（Steam Build 26073097）。现场只读反编译 `StoryCharacterConfig.Get(string)`、`StoryCharacterData.Id/PortraitResourceList`、`StoryCharaterImageItem.Mapping/AddressKey` 和 `CharacterPlaceholder` 的既有加载入口；补查 `Mortal.Story.StoryCharacterController.LoadPortrait(string)`／`LoadPortraitSync(string)`，两者均以 `FirstOrDefault(x => x.Mapping.Value == key)` 取表情资源。`special405/laugh2` 在原配置中重复出现且地址不同，扫描器须保留第一条，不能以后条覆盖或自行创造新的官方表情 ID。该修正仅作用于本地预览映射，未修改官方资源或 Host 接口；全量范围及核对结果见本轮 `out/portrait-scan-20261024/`，不作为游戏运行验收。

2026-10-20 立绘目录修复只读核对当前本机安装的 `Mortal.Story.CharacterPlaceholder.LoadCharacterAsset(string)`、`LoadCharacterPortrait(string,string)`、`GetPortrait(string,string)`，及 `Mortal.Core.StoryCharacterConfig.Get(string)`、`StoryCharacterData.Id`／`PortraitResourceList`。人物 ID 来自 Mapping.Value，表情与资源地址来自 PortraitResourceList；未从旧剧情中出现的表情推断完整清单。`girl5_9` 是独立的「崆峒_小梅_入魔」配置；`girl5_4` 当前指向普通成年小梅图片。编辑器的外观分组最终保存这些真实 ID，继续使用已有加载入口，没有修改官方对象或新增 Host 接口。当前安装的图片包位于 `StreamingAssets/aa/StandaloneWindows64`，本机只读提取 1,335 张图片成功、0 失败；这是资源提取证据，不是游戏运行验收。

| 能力 | 原版证据 | 可实现范围 |
| --- | --- | --- |
| 场景战斗 | `LuaManager.ChangeScene(name,key,nextScene)` 写入 `CurrentSceneKey` / `CurrentNextScene`；`CombatLevelConfig.Get()` 是 Combat 关卡的实际解析入口 | Battle 继续选择原版地图；Combat 使用运行时临时 ID，由 Host 即时组装隔离关卡，结束后回 Story，不借用固定原版决斗配置 |
| 场景切换就绪 | `LuaManager.Init()` 先 `StartCoroutine(UnloadLoading())`、不等待便立即 `ExecuteLuaScript()`；`SceneController` 在目标场景激活后仍以 `_currentLoadingScene` 记录 `Loading1` | Combat/Battle 由 Host 延迟切换：同时等待 `IsPrepare=false`、`IsLoading=false` 且 `_currentLoadingScene` 为空，再调用原版 `ChangeScene`，避免章节首节点与 Story 的异步卸载冲突后永久停在“读取中” |
| Combat 结果 | `CombatManager.GameOver(bool win)` | `win` / `lose`；原版先应用关卡 WinResult/LoseResult，再 `LoadNextScene()` |
| Combat 对手配置 | `CombatManager` 读取 `CL_<key>.EnemyStat` 并调用 `CombatActionController.SetStat(CombatStat)`；人物与动画的真实关系是 `CombatLevel.EnemyStat.Name` → `CombatStat.CombatAvatar`，资源目录不是主键；`CombatStatUI.Setup()` 直接从 `_statData.GetNameKey()` 写顶部姓名；Story 尚在时 `CharacterPlaceholder._config.Get(id)` 可取得本地化名与 normal 立绘 | Host 即时创建隔离 `CombatLevel` 并清空原版事件/结果/作者数值；官方人物按 `EnemyStat.Name` 精确取得 Name、四帧、详情头像和 HeadImage，不再猜资源路径。`max_health` 是 `CombatStatItem.BaseValue`，不覆盖原版 `ModifyList`；技能初始化后界面显示 `FinalValue`，未显式写 `health` 时以最终满血进入。 |
| Combat 赵活覆盖 | `SetPlayerStat`：`MaxHealth=GetStatCollectionSummary(_playerTotalHealth)`，六维=`GameStat.FinalValue`；`GameStatConvertData.GetValue()` 返回 `_statData.FinalValue`；`PlayerStatData.Set`/`Talents.Set`；`CombatStatController.SetStat` 用 `new CombatStatItem(MaxHealth)`；`CombatStatItem` 分离 `BaseValue`、`ModifyList`、`FinalValue`，而 `CombatStatUI.UpdateHealth` 读 `FinalValue`；`CombatManager.InitCombatRound()` 在 `SetPlayerStat` 后执行双方 `InitSkill` | 战前把 `player_*` 当基准写入 GameStat.Value，让官方换算和被动生效；`player_max_health` 是额外基础值，必须在 `InitSkill` 后累加到原版 `_playerTotalHealth` 换算结果并保留 `ModifyList`，不能覆盖它。战后只还原改过的 GameStat/Talent。敌我都以最终血量同步原版治疗/受伤上限。 |
| Combat 独立背景与首回合 | `CombatManager.Start()` 在 `yield return LoadCombatLevelAsset()` 后执行 `_round.Reset()`；`LoadCombatLevelAsset()` 末尾才 `StartCoroutine(InitCombatRound())`，后者在 `UnloadLoading()` 后调用 `_round.NextRound()`；`CombatRoundUI.Update()` 直接显示 `CombatRound.Value` | 自定义背景 Addressables 必须在原版 `LoadCombatLevelAsset()` 开始前完成预载，并写入本场克隆 `CombatLevel._backImage`，由原版同时赋给 `_backSprite` / `_backImage`。不能在原协程尾部异步等待，否则首回合会被后续 `Reset()` 显示为 0。 |
| Battle 结果 | `GameLevelManager.ShowGameOver(GameOverType,bool)` | `finish=true` 的 `FriendWin` / `EnemyWin` 可继续；`PlayerDie` 只给重试/标题，不伪造可继续分支 |
| Battle 模板 | `GameLevelManager.Setup()` 查找 `BL_<CurrentSceneKey>` | 只能引用游戏内已有 BattleLevel，不动态造地图、Prefab 或 AI |
| Battle 双方配置 | `GetFriendPeople/GetEnemyPeople`、`NpcSpawner.InitNpcList`（`prefab.name` 字典）、`ReadyPanel.Setup`、`CharacterHealth`/`HealthData`、`ReadyState` 跑向预制体 `ReadyPoint`、`Entity._originType` | 各阵营 `id:people` 相加为总人数；标题写 ReadyPanel；血量克隆 HealthData。具名角色先对同侧 BattleLevelConfig 预设，没有则用 catalog Animator 套到同侧底板。生成后只改写 Entity 阵营，不改官方 ReadyPoint/OutsidePosition。不改官方资产 |
| 战斗技能 | `SetPlayerBattleSkill` / `SetBattleSkillActive` / `SetBattleSkillLevel` / `ResetBattleSkill` | 可做战前技能配置；尚无已确认的通用临时状态效果 API |
| 商店库存 | `ShopDatabase` 公共 Books/Miscs/Specials 列表、`ShopItem(ItemData,int)`；`ShopPanel.Init()` 只枚举这三类 | Host 在包指纹绑定的受控会话中临时替换库存并复用官方 ShopPanel；结束或故障恢复原库存。Consumables 不在该面板买入列表中 |
| 单品价格 | `ShopPanel.AddBuyPanel` 调 `ItemData.GetBuyPrice(discount)`；`Open(int)` 只把 0/非0 映射为原版关系折扣/统一 50% 折扣 | 原版没有公共逐商品改价入口；`custom_shop` 只暴露 0/1 原版折扣，拒绝 `price` |
| 属性/好感/物品/天赋检定 | `LuaManager.GetStatData`、`Relationships.Get(type).Value`、`ItemDatabase.HasItem`、`Talents.Get(id).Level` | `stat_check` 直接调用原版 LuaManager；其余缺少 Story Lua 入口的读操作由 Host 提供严格只读 bridge，未知 id fail-closed |
| 存档 | `SaveSystem.CurrentSlot` / `SetSlot` / `SaveGameData(string)` / 私有 `AutoSaveData(string)` / `AutoLoadGameData(string)` / `SaveUniverseData()`；`SaveGamePanel.OnPanelOpen` / `SaveSlotPanel.Setup` | 每个 `campaign_id` 独占原版风格 001～020 手动栏位，分别映射到 `mod_campaign_<campaign_id>` 与 `_sNNN`；三类自动槽分别追加 `_auto`、`_auto_free`、`_auto_battle`。保存页和读档页都只显示当前 MOD 的栏位，空手动档可新建、已有手动档可覆盖/读取。Universe 保存时临时隐藏 MOD 槽并保持最后一个原版槽。Host Int32 sidecar 与 Gameplay checkpoint 同样按实际 MOD 槽绑定，不修改 GameSave schema |
| 标题读档 UI | `TitleManager._slotPanel/OpenSlot()`；`LoadGamePanel._saveSlots/_autoSaveSlot/CreateSlot/SetButtonNavigation`；`LoadSlotPanel`/`AutoSaveSlotPanel` 的 `_slot`、`Setup`、`OnTitleClick` | 临时接管原版槽位：左侧三类自动栏按预制体 `_slot` 绑隔离档，空栏走 `System/NoData`；右侧复用官方 001～020。`OnTitleClick` 在标题页必须重定向，不能读原版 `auto*`。关闭时调用原版重建逻辑恢复 001～020 |
| 游戏内/战役读档 | `MenuPanel.LoadButtonClick` 打开 `_loadPanel`；战役暂停是 `PausePanel.LoadButtonClick` 打开另一份 `_loadPanel` | MOD 战役中两处都复用标题那套战役存档页，并默认进入当前 MOD 的次级存档页，按当前 `campaign_id` 显示 001～020 手动隔离槽和三类自动槽 |
| 结算展示 | 已验证剧情入口 `mainui.DisplayMessageText`，以及 `reward` 已使用的属性/好感/天赋/物品/旗标原子接口 | `result_screen` 仅把标题/说明提示与现有奖励顺序组合；不新增、Patch 或猜测结算 UI |
| 自定义骰子 | `DiceMenuDialog.ExecuteRoll(string[],...)` 逐项写入 `CachedButtons[i]`，`UpdateSelection` 按结果序号选中按钮；`DiceCheckResult.ResultCount` 是随机点数与加值之和；当前提取的原版检查点结果档数最大为 4 | 作者直接设置 max、标题、固定加值及 2～4 个结果分段。Host 仅为特殊 MOD 检查名构造结果，继续复用原版 DiceMenuDialog。超过 4 档没有原版按钮容量证据，因此不开放 |

## 2026-10-27 官方插图预览与大图底色

- 实时反编译 `Mortal.Story.StoryPicture.Show(string key)` 确认加载 `Addressables.LoadAssetAsync<Sprite>("pic_" + key)`；提取器因此仅从本机 catalog 的 `pic_` 地址生成图键，使用 catalog 的真实内部路径解析 Sprite。当前目录共 537 个图键，`data/picture_keys.json` 只保存键和目录分类；图片仅写入用户本地预览缓存。
- `StoryMainUI.ShowPicture(string)`、`ShowItemPicture(string)` 与 `ShowBigPicture(string,int)` 调用相同的 `StoryPicture.Show`。大图的第二参 `color` 为 0 白底、1 黑底、2 透明；`cg kind=big` 必须传此参数，省略编辑字段时生成 0。`ShowMap(string,string)` / `ShowFamilyTree(string,string,float,float)` 为地图点位演出，`DisplayTitle(string)` 为本地化标题文本，不能当作图片地址。
- 本次实际从本机解码了普通插图、物品图和大图各一张；未启动游戏验证显示时序。自动用例检查图键映射、失败文件排除、缓存路径与大图参数。

## 明确不支持

- `draw` / `escape`：Combat 没有对应结果，Battle 虽有 `Timeout` 枚举但当前关卡逻辑按双方人数转成 FriendWin/EnemyWin。
- 动态创建 Battle 地图、NPC Prefab、模型、AI、武学动画或战斗机制。
- 把 `mission` 当 Mod Quest：它直接写 `MissionManagerData`，会污染原版任务命名空间。
- 把包哈希、水印或作者自报字段当官方认证。

## 验证等级

- **DECOMPILE VERIFIED**：本页表格和 JSON contract 中的签名、控制流与程序集哈希。
- **AUTO VERIFIED**：后续 Compiler/Editor/Host 单测和 Host 构建。
- **NOT VERIFIED IN GAME**：新增高层 Gameplay 节点在真实战斗、战役、商店中的交互与返回时序；实现后仍需实机验收。
