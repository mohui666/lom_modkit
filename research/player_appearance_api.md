# 赵活外观接口确认

2026-10-01 在 Windows 的当前游戏安装中定向反编译以下类型，并读取本机人物资源映射。未启动游戏。

| 类型 / 成员 | 用途 |
| --- | --- |
| `Mortal.Story.CharacterPlaceholder.LoadCharacterAsset(string)` → `IEnumerator` | 官方角色加载入口；通过 Harmony postfix 替换 `player` 的协程 |
| `CharacterPlaceholder._player2` → `Mortal.Core.StoryCharacterData` | 官方自恋主角的只读配置 |
| `CharacterPlaceholder._config` → `StoryCharacterConfig`，`Get("player")` | 普通主角配置 |
| `CharacterPlaceholder._talentData` → `PlayerTalentData`，`Level` | “随游戏天赋”的外观选择 |
| `CharacterPlaceholder.AddCharacterGameObject(StoryCharacterData)` / `UnloadCharacterAsset(string)` | 复用官方实例创建、销毁与资源释放 |
| `StoryCharacterController.Data` | 判断已加载实例是否使用目标配置 |
| `StoryStageController.CharactersOnStage` / `Hide(PortraitOptions)` | 同一演出中切换外观前隐藏旧实例 |

资源配置“唐門_主角_自戀”的 `Id` 同样为 `player`，其 `normal` 地址为 `Assets/__Project/Images/Characters/Player_自戀/normal.png`。编辑器缓存使用独立键 `player_beautified` 区分预览图片；导出剧情中的 ID 仍为 `player`。

实现位于 `runtime/MortalModHost/src/PlayerAppearance.cs`。覆盖仅在所属 MOD 包活动期间生效，进入新脚本或清理包上下文时复位。读取官方配置，不更改 ScriptableObject 或玩家天赋。最初完成的是当前游戏引用下的构建和离线检查；后续实机结果记录如下。

2026-10-02 重新读取当前安装的 `CharacterPlaceholder`，并在 Windows 游戏内用独立战役依次显示普通、美颜、恢复普通赵活，三次外观均与选择一致。

同次检查 `Mortal.Core.SaveSystem.NewGameData()` 确认其调用
`PlayerStatManagerData.SetGameTime(1, 4, MonthStageType.上旬)`；
`Mortal.Core.GameTime` 的默认构造同样从第一年四月上旬开始。
自由模式模板的首个地点事件应设为四月上旬，不能把一月作为新战役初始月份。

`GameTime.operator ==(GameTime, GameTime)` 直接读取两侧的 `Year`、`Month`、
`Stage`，不处理空引用。因此 `time == null` 即使在 `time` 有值时也会抛出异常。
自由模式条件检查使用 `ReferenceEquals(time, null)`；修改前已在实机日志中重现
“触发器时间/好感条件评估失败”，对应月份条件被错误判为不满足。
