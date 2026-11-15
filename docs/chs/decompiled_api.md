# 反编译接口

> 权威流程。改 Combat / Battle / 存档 / 商店 / 属性 / 场景 / 原版 UI 之前必读。
> 仓库规则见根目录 [AGENTS.md](../../AGENTS.md)。

本仓库**不实现自研战斗引擎**。凡是调用《活侠传》的功能，入口必须是当前安装里
已经反编译确认的类型与方法。

## 先读哪份

| 材料 | 用途 |
| --- | --- |
| [`research/gameplay_api.md`](../../research/gameplay_api.md) | 已确认能力、可实现范围、明确不支持项 |
| [`research/gameplay_api_contract.json`](../../research/gameplay_api_contract.json) | 已确认的接口片段；不计算程序集哈希 |
| 本机 `Mortal_Data/Managed/*.dll` | 当前游戏的真实接口；以它为准。用 `ilspycmd` **只读**查询，结果留在本机 |
| `docs/research/decompiled/` | 本机历史归档，**已 gitignore，不随仓库提供，不要上传**。包含部分 Combat 类型快照，没有完整 Battle/Combat 程序集反编译结果，不能当现网证据 |
| 本机 `research/windows-reference-20261015/README.md` | 2026-10-15 的接口迁移索引，不随公开仓库提供；当前公开接口摘要见上方 gameplay_api 与 contract |

游戏更新后，只查询本次涉及的当前类型和方法，再核对已有接口结论。按照根目录规则，不运行基于程序集哈希的验证，也不因文档维护而启动 Windows 或游戏测试。相关类型或安全入口不可用时，记录具体限制，不猜测替代 API。

## 怎么查一个类型

```powershell
$managed = "C:\Program Files (x86)\Steam\steamapps\common\LegendOfMortal\Mortal_Data\Managed"
ilspycmd -l c "$managed\Mortal.Battle.dll"
ilspycmd -t Mortal.Battle.ReadyPanel "$managed\Mortal.Battle.dll"
ilspycmd -t Mortal.Combat.CombatEnemyController "$managed\Mortal.Combat.dll"
```

只读反编译。不要把反编译结果写回游戏目录，也不要 commit / 打进 Release。
可以上传的是我们自己写的接口摘要（本页、`research/gameplay_api.md` 和 contract
中的接口说明），不是游戏程序集转出来的整份 `.cs`。

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
| 原版前置章衔接 | `LuaManager.SetNextScript/SetCurrentStoryScript/Init/ExecuteLuaScript`。前置模式只播放真实 `source_script`，原版目标与所测事件必须由实际调度进入；分别记录原版 Lua 放行、事件 ID 获选和 MOD Lua／节点执行，准备章不能充当事件成功 |
| 原生视频画面 | `LuaManager.Init`／`SceneController.UnloadLoading` 的先后顺序决定录制起点；`ScreenCapture.CaptureScreenshotIntoRenderTexture` 读取最终合成画面。支持时用 `AsyncGPUReadback.RequestIntoNativeArray`，按提交顺序消费有界缓冲；不把编辑器绘制当成游戏帧 |
| 原生视频选项与音轨 | `StoryManager.AutoDialog`、`MenuDialog.CachedButtons`／按钮 `onClick`；Wwise `SetOfflineRendering/SetOfflineRenderingFrameTime` 和主设备 `StartDeviceCapture/UpdateCaptureSampleCount/GetCaptureSamples/StopDeviceCapture`，与既有 `RenderAudio` 帧时钟配合。参数与采样计数语义见 gameplay_api，不能另造混音或跳过交互结果 |
| 后台视频存档隔离 | 专用 Worker 的 `SaveSystem.GetUserFolder/GetSaveDataFile/GetLegendDataFile/GetBackupDataFile` 前缀只映射本任务 profile，常规游戏进程不启用。四个入口分别核对，不能凭单个保存路径推断完整隔离 |
| 立绘容器与跨槽移动 | `PortraitController.SetupPortrait/SetRectTransform/DoMoveTween`：官方容器尺寸来自 from 槽，普通位移只改坐标；并行独立位移保留容器。自定义人物移动结束才按目标槽重新适配。锚点不是可见脚底，预览不得提前按目标槽尺寸切换 |

禁止：猜不存在的 `draw`/`escape` 结果、改官方 ScriptableObject 资产、把 Addressables 里的玩家技能 Animator 当成可生成 NPC prefab。

1.6.0 的普通试玩和原生视频统一停用可见与隐藏画面水印；这不解除 Host 的包身份、脚本注册与存档归属规则。当前挂钩事实写入接口笔记，具体构建、离线和实机结果写入 [测试矩阵](test_matrix.md)；仓库不另存版本 notes。
