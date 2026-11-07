# Gameplay API 实证矩阵

## 2026-11-07 人物震动预览

现场只读反编译当前游戏 `Fungus.ShakePosition.DoTween()`，确认其读取 `_amount.Value`、`_duration.Value`、`_targetObject.Value`，调用 `iTween.ShakePosition` 并回调完成。现有官方人物生成代码设置 common 流图的 `ShockPosition`／`ShockDuration`，运行 `shock` 块；自定义人物仍使用 Host `CustomCharacterRuntime.Shock`／`ShakeImage` 的衰减位移。本次只补编辑器播放时遗漏的临时位移和 0.5 秒默认计时，不修改上述游戏执行逻辑。预览为衰减动画示意，不声称逐帧等同官方随机抖动。

## 2026-11-07 原版音频试听

本次现场只读反编译确认 `LuaManager.PlayMusic/PlaySound/PlayEnvSound(string)` 分别经 `MusicDataCollection`、`SoundDataCollection`、`EnvSoundDataCollection.GetKey(string)` 按资产名取 `Key`；这些条目继承的 `CollectionData<T>._list` 为序列化引用列表，`MusicData/SoundData/EnvSoundData` 的 `_key` 为字符串。`SoundManager.PlayMusic/PlaySound/PlayEnvSound(string)` 分别向 Wwise 提交 `Music_`、`Sound_`、`Env_` 加该 Key 的事件名。试听只读取当前安装的对应集合和 `SoundbanksInfo.xml` 的事件媒体引用，并从 BNK 索引读取单个 WEM，用外置 vgmstream r2168 解码；不调用游戏播放接口、不修改官方资产。多源事件不任意挑一段替代运行时混音，明确提示实机确认。

## 2026-11-06 数值写入与系统提示

重新只读核对 `Mortal.Story.StatModifyManager`：`UpdateSetPlayerStat(string,int)` 仅按目标值计算变化量，调用 `StoryMainUI.DisplayStatModification`；真正写入需调用 `SetPlayer(string,int)`，由 `PlayerStatData.Set` 转到 `GameStat.SetValue` 并保留游戏自身上下限。生成器显示变化后始终执行写入。`LoverInfoPanel.UpdateStatus` 按 `FinalValue` 读取 `PlayerInfo/Lover/<编号>`；本机简体语言表确认 0～8 与 99 的名称，编辑器只提供选择，不改官方资产或上下限。

`ItemDatabase.AddItem` / `ItemData.AddCount` 只更新数量。新物品回调沿用现有官方／用户物品 ID 解析，比较实际变更前后数量，有变更且启用提示时调用 `StoryMainUI.DisplayItemMessage(type,"add"|"remove",id)`。静默同步回调仅阻止 `GameMessagePanel.DisplayMessage`、`StatModifyMessagePanel.DisplayStatModification` 和 `RelationshipModifyMessagePanel.DisplayRelationshipModification` 创建提示物件，保留上层数据操作与记录，异常退出立即恢复。回调不包含 Lua 等待。

## 2026-11-05 原版章节补丁入口

2026-11-06 再次只读核对当前 `LuaManager.SetCurrentStoryScript(string)` 与 `Fungus.LuaEnvironment.LoadLuaFunction/RunLuaFunction`。整章导入复用这些入口，来源核对读取同一路径的 `TextAsset.text`，每个包的每章首次核对后缓存结果。原生代码保留局部作用域，可编辑片段用局部函数执行，插入对白后恢复原生 `setcharacter`、`setsaydialog` 参数和 `sayoptions`。

同日实机测试发现，无参数 `Action` 绑定在游戏 Mono 环境中被 MoonSharp 识别为 userdata，原版步骤结束调用 `mod_vanilla_source_continue` 时中断。改为显式 `CallbackFunction` 并返回 `DynValue.Nil` 后，第一章开场、原版数值旗标条件的命中与跳过路径均通过，原文／新增对白／原文的标记切换正常。离线协程与语法检查另行通过，完整章节和全部战斗分支尚未实测。

当前本机 `Lua/fungus` 的 `M.sayoptions` 为 Lua 表，`M.setcharacter/setsaydialog/setmenudialog` 为同步设置入口。`Mortal.Core.NormalOptionButton.UpdateContent` 经 `LeanLocalizationResolver.GetStoryText` 解析选项，找不到翻译时返回原字符串，整章转换后的原文选项修改可直接使用文字。

只读反编译当前安装的 `Mortal.Story.LuaManager.ExecuteLuaScript()`：脚本名来自 `PlayerStatManagerData.CurrentStoryScript`，加载路径固定为 `Story/ChineseTraditional/<ScriptName>`，资源为 `TextAsset`，通过 `LuaEnvironment.LoadLuaFunction` 与协程执行。Host 在既有入口按当前启用作品匹配原版章节，保持游戏中的原脚本名和触发流程；补丁只包含定位片段与替换内容，运行时读取本机资源，不改写官方资产。直接试玩补丁时，在编译成功后调用已核对的 `LuaManager.SetCurrentStoryScript(string)` 恢复原版章节标识，该入口只设置当前脚本并记录演出日志，不切换场景。

旗标插入复用现有 `CheckPointManager.Condition(string)`（转给条件表的 `GetResult`）及 `LuaManager.GetFlagData(string)`（读取 `MissionManagerData.GetFlag(name).State.ToString()`），不重写旗标判断规则。新增文字使用本 MOD 的已读文字键注册，不覆盖原版语言键；原句未改时保留原 `GetStoryText` 调用。

编辑器从本机 `ResourceManager.m_Container` 的原版路径读取章节，用 `TextAsset.m_Name` 保留实际脚本名大小写。对白显示按 `Lean.Localization.LeanLanguageCSV` 已反编译的 `Language`、序列化 `Entries`（`Name`/`Text`）读取简体文字，不按重名 TextAsset 的顺序或汉字猜测语言。这里只记录接口依据；离线解释器验证不等于游戏实机验证。

### 章节标记与任务入口时间线

本次只读核对 `LuaManager.ScriptName/GetStoryText(string)`、`PlayerStatManagerData.GameTime`、`MissionManagerData.Main/_subMissions`、`MissionData._missions` 和 `MissionCheckData`。标记在 `GetStoryText` 的后置钩子记录最后的文字键；显式快捷键写出当前章节、文字和 `GameTime.Year/Month/Stage`，不调用存档、任务或旗标写接口。`GameTime` 重载了相等运算，判空使用 `ReferenceEquals`。

入口索引读取 `MissionCheckData.StartScript/Key/Position` 和 `_active/_desc/_dayCheck/_timeCheckType/_specificTime/_minTime/_maxTime/_periodStage`；时间类型 1／2／3 分别为指定日期／区间／每月某旬，未指定时明确显示时间不定。时间线定位方式参考[咖啡伴侣 StoryAtlas](https://github.com/kevinnie2003/LOMAssistant/blob/main/LOMAssistant.Features.Story/StoryAtlas.cs)，本功能只读取任务入口，不实现存档回溯或修改原版任务数据。

`StatModifyManager.Mission(string,string)` 调用 `MissionManagerData.SetKey`；后者仅对保留名称 `main` 忽略大小写，其余名称由 `SubMissionsData.Get` 精确匹配。路线索引按相同规则关联脚本中的任务推进与时间线入口，保留任务键和支线名称的大小写。

## 2026-11-06 来源标记刷新

只读核对当前安装的 `Fungus.SayDialog.Awake/OnDestroy`，它们维护受保护静态表 `activeSayDialogs`。Host 从该表读取全部实例，并继续检查可变的 `ActiveSayDialog` 和已绑定对白框，替代每个渲染回调的全场景查找。

当前 `UnityEngine.UI.Graphic.color/material` 等属性在值改变时自行标记重绘；`SetAllDirty()` 还会注册布局、材质和顶点重建。来源标记保留各渲染阶段的检查与修复，取消无变化时的强制全量重建及重复重新挂父节点。

## 2026-11-05 原版粒子帧捕获接口

针对原版预览，重新只读核对当前游戏 Unity／URP 的 `UniversalRenderPipeline.RenderSingleCamera(ScriptableRenderContext,Camera)`、`RenderPipelineManager.endFrameRendering`、`UniversalAdditionalCameraData.SetRenderer`、`Camera.CopyFrom`、`RenderTexture`、`Texture2D.ReadPixels` 与 `ParticleSystem.Simulate`。当前使用 URP，不能只凭 `Camera.Render` 存在就认定它支持本场景；新捕获相机使用 Base 类型，并匹配源相机实际 renderer index，CopyFrom 不复制 URP 附加数据。仅给自建粒子实例分配捕获层，不改变原场景相机或官方资源。

当前 Gamma 工程中，`EventBubbleMat_Alpha_001` 的 RGB 混合为 SrcAlpha／OneMinusSrcAlpha，`GlowMat_Add_001` 为 SrcAlpha／One，但两者实际 shader pass 都只写 RGB；直接透明 RenderTexture 的 alpha 不足以区分普通透明与加色。捕获用浮点目标分别渲染黑／白背景，推导原 RGB 和透射率，再写预乘 RGBA；这样可保留加色 RGB>0、A=0，也不会把普通透明误当加色。Blood 使用的 shader 则写 RGBA，不能统一猜材质规则。这些为本轮实际 shader／材质及 API 依据，缓存生成结果另行记录。

全目录实测发现原编辑器清单混入 `驚訝_001`：当前共享资源中该对象的脚本类型是 `Mortal.Core.SoundData`，真实音效键为 `Surprised1`，没有 Prefab；原版 StoryParticleConfig 的 124 个唯一特效键中没有此项。仅从 effects 选择目录剔除，保留音效目录，本工具原有可用特效目录为 96 项，不把音效名猜改为特效别名。

手动 `ParticleSystem.Simulate` 后系统保持暂停，`IsAlive` 不能单独用来判断捕获自然结束。本轮实机核对后，尾段在原 StopEmitting 后以全部活动子系统真实 particleCount 归零结束；非循环播放同时核对实例 particle.time、持续发射区间或最后 burst，避免在延迟与间歇期提前结束。编辑定位使用实际已录制帧中像素总量最高的一帧 `preview_time`，正常播放保留原版起始空帧与时序，不合成图形或寿命。

同日读取当前 `Texture2D.GetRawTextureData<T>()`：它返回纹理 CPU 存储的 `NativeArray<T>` 视图。捕获在每次 ReadPixels 后局部读取 float Color、直接写输出 Color32，保留双浮点透明度恢复，省去每帧约 7.91 MiB 显式数组分配。输出仅用于 CPU 的 EncodeToPNG，不再 Apply 上传 GPU；不释放或跨纹理内存变更持有这些视图。预览入口改成按需提取当前所缺特效，多批缓存合并复用，全目录仍为可选操作。

实际游戏完整捕获 96 项共 4661 PNG，96 项均有非空画面且代表帧对应实际播放帧。换成 raw 视图后重捕 Blood、Glow、EventBubble、Rain、Hit、Sword_Fight，255 帧解码 RGBA 与此前逐像素一致，包含 Glow 的 RGB>0/A=0 加色数据。实际按需两批分别约 3.53 秒／1 项、15.74 秒／5 项，完整缓存复读约 0.96 秒；首次冷启游戏单项约 16.19 秒。该测量包含任务编排开销，不作为受控 GPU／PNG 性能基准。编辑器自身导出画面另确认原版事件气泡和加色辉光可见。

## 2026-11-05 特效实例与画面坐标

定向只读反编译当前 Windows 游戏的 `Mortal.Story.StoryEffectsController.SetupEffect(string,float,float,float,float,float,float)` 及 `Fungus.LuaBindings.AddBindings`。SetupEffect 返回 void，x/y 是世界位置，a/b 为 localScale；unique=1 复用原缓存，其余值新建，play=1 播放，其余值 Stop。旧的非唯一停止实际会新建对象再停止，不能据此停止前一个叠加实例。当前 `_nonUniqueInstances` 为 `List<GameObject>`，Addressables 分支也在 Instantiate 前同步 WaitForCompletion。Lua 的 effects 全局来自实际绑定组件，Host 据此取得当前控制器。

Host 1.4.5 的 `mod_effect` 始终经原 SetupEffect(unique=0,play=1) 创建自己的新实例，再登记原列表追加的对象。复用、叠加、tag 与后台停止均只管理这些实例，不改 `_dicEffect` 或粒子资产；清舞台时仅移除并销毁登记对象。逐子 ParticleSystem 保存初始 `MainModule.simulationSpeed` 后乘作者倍率，旋转叠加初始 localRotation；Stop 使用实际提供的 `Stop(bool,ParticleSystemStopBehavior)`，支持保留余粒或 StopEmittingAndClear。

只读核对当前 level2：StageCamera 为正交相机、orthographicSize≈5.4、全视口、cullingMask=262144（层18），位于 MainCamera 下，默认相机世界 z=-10。z=0 平面在16:9下宽19.2、高10.8。新 screen 坐标按控制器所在场景可渲染实例的 StageCamera，以 ViewportPointToRay 与 z=0 平面求交，允许屏外；不把默认宽高硬编码为运行时映射。编辑器仅用默认舞台做示意换算，不能还原原版 Unity 粒子材质与全部随机行为。本条是本轮接口和资源依据，离线测试与实机结果分开记录。

同日用 Host 1.4.5 在独立试玩战役实际观察两组 Glow 的 25%／75% 位置、单组停止、速度 0 定格及恢复、两个叠加实例按标记一起清空、按名称清除，以及重播越过旧 2 秒计时点后仍继续。结束返回 Free 后清场，日志未出现 Error／Exception／mod-runtime-error。此为本轮定向结果，没有逐个验收 96 种特效或镜头与分辨率组合。

## 2026-11-05 立绘预览的透明画布

只读核对当前安装游戏的 `UnityEngine.UI.Image.GetDrawingDimensions`：原版按 `activeSprite.rect` 计算比例，并通过 `DataUtility.GetPadding` 保留裁切留白。UnityPy 的 `Sprite.image` 返回已按纹理裁切并处理旋转的图，不能直接视为完整立绘画布。提取人物预览时采用实际 Atlas（指针或 tag）或 `m_RD` 的 `textureRectOffset`，把图放回 `m_Rect` 的透明画布后再缩略。

本地读取 `special4`、`girl1` 的 normal/laugh1 四张当前 Sprite，在内存处理后，两组分别得到相同的缩略尺寸和透明边界：638×1024 / (91,375,637,1024)，764×1024 / (46,355,708,1024)。未导出原版图片、未启动游戏；此结果验证这两组素材的边距恢复，不代表用户尚未指明的具体人物已逐一验收。旧 PNG 没有原画布信息，需要重新提取；不对 normal 应用统一缩小倍率。

## 2026-11-05 预览拖拽布局的实例变换接口

针对 Host 1.4.4 新布局字段，重新只读反编译当前 Windows 安装的 `CharacterPlaceholder.Get/MoveOffsetCoroutine`、`StoryCharacterController.MoveOffsetCoroutine`、`PortraitController.SetupPortrait/SetRectTransform/Show`、`PortraitState` 与 `PortraitOptions`。原版相对偏移累加 `State.holder.anchoredPosition`，完成后将 `State.position` 改为 holder；后续绝对偏移不能再把这个字段当作原始站位。Host 在 `SetupPortrait(PortraitOptions)` 记录实际目标槽位，新的 `mod_character_layout` 从该槽计算绝对位置或从当前位置累加，并以自有协程移动当前实例的 holder，完成后沿用 `State.position=holder` 语义。旧相对偏移仍调用原版 `MoveOffsetCoroutine`，不改变其行为。

`SetupPortrait` 每次复制站位矩形并将 holder.localScale 重置为 ±1，因此 Host 在该入口后恢复作者的额外倍率，保留水平翻转；100% 表示素材默认体型。倍率即时生效，duration 只控制位移。显式 show 后的 `mod_character_layout_reset` 恢复 100%，换表情、转身和移动不重置倍率；移动仍按原版目标槽清除旧偏移。自定义人物使用已有 `Actor.Slot` 与独立倍率，转身按倍率重新组合正负号。清舞台时停止本模块自己的协程并清除实例状态，不修改官方 ScriptableObject 或站位资产。

补查 `CharacterMoodPanel.Show(string,float)`：原版用 `scaleX==1` 选择左向气泡，其余值选右向；对采用额外倍率的原版人物，仅在这个实例的气泡调用中把朝向值归一为 ±1，保留 holder 的实际大小。插图沿用 Host 自建 Canvas 子物件的 `anchoredPosition`，在原 position 锚点上叠加舞台宽高百分比 x/y；旧七参数调用仍以零偏移运行。本条为接口和实现依据，构建、离线冒烟与游戏实际运行分开记录。

同日用 Host 1.4.4 在独立测试战役中实际观察了编辑器拖动结果：夏灵犀额外缩放约 149.1%、用户人物约 70.6%，人物偏移和插图新位置／比例正确显示；转身、原版换表情后保持布局，显式重新登场恢复素材默认大小。正常结束后返回 Free，人物与插图清除；该次日志未出现 Error、Exception 或布局调用失败。此为定向实机结果，不代表复杂动画、全部人物及分辨率组合均已覆盖。

## 2026-11-05 原版人物补绘与站位尺寸

定向只读反编译当前 Windows 游戏的 `CharacterPlaceholder.LoadCharacterPortrait/GetPortrait`、`StoryCharacterController.LoadPortrait/LoadPortraitSync/GetPortraitName/AddPortraitObject`、`Fungus.Character.Portraits`、`PortraitController.SetupPortrait/CreatePortraitObject/SetRectTransform` 和 `PortraitState.SetPortraitImageBySprite`。新增补绘拦截原版表情加载与名字解析，将包内 Sprite 放进当前人物实例的 `Portraits` 与 `State.allPortraits`，保留官方表情键及原始配置；只给自建 Image 应用缩放，退出包时移除。没有修改 `StoryCharacterData` 或 Addressables 资产。

使用现有 UnityPy 环境只读核对当前 `level2`，M/L1/R1 的站位矩形均为 1920×1080，pivot=(0.5,0)、anchor=(0.5,1)。原版按完整矩形配合 Image.preserveAspect 居中等比适配。自定义人物去除旧的 56% 屏高及槽位尺寸过滤，沿同一矩形适配，Canvas 尺寸变化后保留偏移重算。此为接口与资源依据，构建和离线测试不等于游戏实机验收。

同日使用 Host 1.4.3 在实际游戏中定向检查，覆盖 3840×2160 与 1280×720：同一夏灵犀 PNG 在原版 L1 与用户人物 R1 的 100% 显示等高；新增 `special825/tired`、`sleep` 可切换，原版对白姓名保留，切回 `normal` 还原官方立绘。另完成用户人物 280.5%／25.5% 缩放、隐藏后重登，以及全屏切窗口后继续对白检查，结束后恢复原分辨率。这些为本轮实际观察，不扩展为全部人物、分辨率或第三方 MOD 组合的兼容结论。

## 1.4 开发：自定义玩法与独立存档入口

2026-11-03 定向读取当前 Windows 安装的 `Mortal.Core.dll` 与 `Mortal.Combat.dll`，确认以下入口。这里只记录接口依据，游戏执行结果以本轮实际验证为准。

- `Book.Learn` / `DirectLevelUp` 读取 `MartialLevelData` 的条件、点数和升级加成；`ItemDatabase.Save/Load` 保存并还原书籍数量、修炼等级和点数。Host 克隆数据库、集合及其中的 `ItemData/Book`，新功法使用本场实例，不改官方定义。`BookLevelStatConvertData.GetValue`、`BookDataCollection.GetMaxLevelCount` 与 `MartialStatReference.GetValue` 持有序列化原定义引用，需在会话中映射到克隆后的进度。
- `CombatActionController.AddEffect` 对 `CombatStateType.Support` 创建原版援助栏；`CombatSupportStateUI.Setup` 显示图标和次数。`CombatStateEffectUnitScriptable` 提供 `ApplyBeforeAction`、`ApplyAfterAction`、`TriggerAfterAttacked`，由 Host 自有子类实现配置的援助效果，不修改官方效果资产。
- `Consumables` 只有 `ItemEffect[] Effects`，没有原生 `Use`；`ItemStatPanel.UpdateData/OnItemSelected` 只展示书籍、杂物、贵重品。Host 在杂物列表补自有消耗品，并提供使用按钮；属性效果调用 `PlayerStatData.Add`。决斗恢复使用 `CombatActionController.ModifyHealth/ModifyStamina`；以 `CombatManager.Action(無動作)` 完整推进本回合，保留原版休息回气，不捏造行动枚举。
- 当前 `ItemDatabase.Save/Load` 没有序列化 `Consumables`。Host 仅将自有道具数量编码进 `PlayerStatManagerData.StoryKeyList` 的原生 `StorySave` 通路，并在原生加载后恢复数量；不修改 `GameSave` 格式。
- `CombatManager.Action/ActionRoundHandle/ActionCost` 与 `CombatActionButton` 是自创招式的执行和选招入口；基础威力接 `SetCurrentDamageResult`，耗气接 `GetActionCost`，命中接 `GetAttackHitRate/GetWeaponHitRate`，保留原生互拼、克制及防御换算。绝招对嘴炮时会改读 `GetTalkUltimateHitRate`；绝招互拼耗气可能乘原版比例，不能把编辑值宣称为所有情形的最终结果。
- `CombatPlayerController`、`CombatPlayerRootController`、`CombatEnemyController` 的 `Attack/Weapon/Ultimate` 是实际角色演出协程。自有 PNG 序列帧替换本次演出，在指定帧调用 `TakeDamage/HandleDamage`。播放时隐藏角色子树的精灵和模型渲染器，结束逐项恢复原 `Renderer.enabled` 与 Idle；PNG 沿用精灵分支的坐标与图层，不能使用 MaxModel 模型分支的渲染空间。源帧统一朝左，通过 `_sourceAction` 与 `CombatManager.GetActionController(我方)` 比较来决定我方水平翻转，避免取到未激活精灵的错误朝向。`CanUseAnyAction` 原判断为 false 时，仅补充真正可用的自创招式或可操作玩家的恢复药，不绕过死亡、眩晕或自动行动。
- `TitleManager.OpenSlot/NewGame/LoadGame/ContinueGame` 是原版流程入口；MOD 入口直接调用 `SaveSystem` 并选择自己的隔离槽。标题只提供一个开始游戏按钮，再选择纯原版、原版＋MOD、纯 MOD；纯 MOD 随后选择具体作品。
- `MenuPanel.LoadButtonClick`、`SaveButtonClick` 和 `Mortal.Battle.PausePanel.LoadButtonClick` 是游戏内存读档入口。MOD 模式打开当前进度所属的独立页；原版模式不接管。继续使用 `SaveSystem.GetSaveData/GetAutoSaveData/SaveGameData/AutoLoadGameData`，不改变原版 GameSave 格式。

独立 UI 的原版＋MOD 栏位使用 `mod_addon_default`，纯 MOD 保留 `mod_campaign_<campaign_id>`；Universe 最近原版槽保护继续有效。新自定义玩法最低要求 Host 1.4.0。

下方既有矩阵来自 Phase 49 的接口核对；`gameplay_api_contract.json` 保留当时的记录，不能把历史程序集摘要作为现版兼容证明。游戏更新后按根目录 `AGENTS.md` 只读反编译受影响类型与方法，不运行基于程序集哈希的复验。

本页和 `gameplay_api_contract.json` 是我们自己整理的接口摘要，可以进仓库。
用 `ilspycmd` 从游戏 DLL 转出的整份 `.cs` **不要上传**（见根目录 `AGENTS.md`、`.gitignore` 里的 `docs/research/decompiled/`）。

## 2026-11-01 完整属性目录核对

本机现版 `Mortal.Core.GameStatType` 的 54 个 `StringValue` 与 `globalgamemanagers.assets` 中 `PlayerStatData` 的 54 个实际数据引用逐项一致。编辑器原目录只扫描剧情的 `StatModifyManager.Player` 调用，遗漏 23 项属性（包括心上人），另将防御保留为旧键 `defence`；现版防御键为 `combat-defence`。提取器改用已核对的完整键表，显示名仍优先复用本地语言表。

现场只读反编译 `Mortal.Story.LuaManager.GetStatData(string,int)`，确认它按 `GameStatType.StringValue` 查找 `PlayerStatManagerData.Instance.Stats.Get(type).FinalValue`；`Mortal.Story.StatModifyManager.SetPlayer(string,int)` 写对应属性基值。`Mortal.Core.LoverInfoPanel.UpdateStatus()` 直接以该属性值查询 `PlayerInfo/Lover/<编号>`，所以 `lover` 是编号属性，按 `==` 检定；“心上人已变为……”旗标不代替当前值。这里只补目录并复用现有生成入口，不改官方资产或包格式。2026-11-01 在 Windows 游戏 `release_1.0.6001.0` 的隔离试玩中已完成全部 54 项读取，以及 `lover` 成功／失败分支、腿法、儒学与防御的设置及检定；未逐项改写全部属性。

## 2026-10-31 旁白对齐与原地换表情接口核对

2026-11-06 清除画面人物：本机只读核对 `Fungus.Stage.CharactersOnStage` 返回舞台人物列表，`Mortal.Story.StoryStageController.Hide(PortraitOptions)` 先调用 `HideMood` 再交给原版退场。新增 `hide_all` 节点通过 `mod_char_hide_all()` 复用 Host 的 `CustomCharacterRuntime.HideAllOnStage()`，按现有逻辑清除自定义人物并以零淡出时长隐藏官方人物。该入口仅处理人物。此处记录接口依据，未作本次游戏实机验证。

2026-11-07 人物清场时长：再次只读核对本机 `Mortal.Story.StoryStageController.Hide(Fungus.PortraitOptions)` 与 `Fungus.PortraitOptions`。正时长清场传入 `fadeDuration`、`useDefaultSettings=false`、`waitUntilFinished=false`，由生成的 Lua 等待时长；自定义人物复用 Host 现有 `Hide(raw, fade)`。省略参数保留立即清场。已通过 Host 编译，未作本次游戏内淡出验收。

2026-11-07 滤镜预览：只读核对本机 `Mortal.Story.StoryCamera.ActiveVolume(string key, int active)`，它按键独立设置 `PostProcessingVolumeItem.Volume.enabled`。读取本机舞台 VolumeProfile 的 ColorAdjustments 色值：memory RGB 约 (1.059274, 0.948355, 0.643328)，dream (1.189207, 1.189207, 1.189207)，fire (2, 1.026768, 0.971698)，blurdim (0.641509, 0.641509, 0.641509)。编辑器对这四个预设模拟色调并近似绘制模糊、暗角，不执行 URP 景深或 FilmGrain；从中间步骤试玩的前置节点恢复各键开关。未提交原版资源，未作本次游戏画面一致性验收。

2026-11-04 单人气泡：定向只读核对本机 `StoryStageController.CreateMoodObject/Show/Hide/SetDimmed`、`StoryCharacterController.SetupMood/ShowMood` 及 `CharacterMoodPanel.Show(string,float)/Hide()`。原版所有舞台人物都会创建面板，`Show` 只按输入键匹配面板 Mapping，不要求人物自身有这个表情。Host 1.4.1 的 `mod_character_mood(character,icon)` 直接调用该人物实例的 `Mood.Show`，使用已读资源的九种 Mapping；不改 `StoryCharacterData`、表情键或官方资产，继续使用原版动画和朝向。隐藏单人、恢复自动、压暗/解除压暗、离场及清舞台分别沿原版入口处理；本条仅记录接口与实现依据，运行证据另记。

本次 Windows `release_1.0.6001.0` 隔离试玩中，实际观察到只有 `normal` 立绘的 `artist1`（武师）和 `aunt1`（大妈）同时显示独立气泡；叹号、音符、汗滴、害羞、阴影、乱线等可切换且立绘保持不变。已观察单人隐藏不影响另一人、恢复跟随普通表情、转身、压暗/解除压暗、离场重登不残留，以及全场关闭清除单人设置。自定义用户人物未接入，也未做全人物实机覆盖。

本机当前安装的 `Fungus.SayDialog.StoryTextObject` 返回正文 `UnityEngine.UI.Text`。Host 在显式靠左句前保存其 `alignment`，横向扩展正文区域并保留 5% 留白，下句开始前及清舞台时恢复对齐与原矩形布局。

现场只读反编译 `CharacterPlaceholder.LoadCharacterPortrait/GetPortrait`、`StoryCharacterController.LoadPortraitSync`、`Fungus.Character.GetPortrait`、`PortraitState.SetPortraitImageBySprite` 和 `PortraitController.Show/Hide/SetupPortrait`。`LoadPortraitSync` 会给已有 holder 补建表情 Image；`SetPortraitImageBySprite` 切换运行时 `portraitImage` 引用。原版 Show/Hide 的 SetupPortrait 会重置 holder，因此新增 `portrait` 仅包装这些加载及换图入口，对运行时 Image 分两段调用 `LeanTween.alpha`，不重新登场、不改官方资产。RGB 压暗、holder 朝向/偏移/旋转保留；用户人物同样在换图适配后恢复位置。2026-11-01 实机已观察官方与用户人物在无对白时换表情，以及普通／居中旁白的靠左留白。

2026-11-01 补查 `StoryCharacterController.ShowMood/GetPortraitName` 与 `CharacterMoodPanel.Show/Hide`：`GetPortraitName` 保存原表情键，`ShowMood` 使用该键与 holder 朝向显示气泡，图标由面板内官方 Mapping 自动匹配。独立 `mood` 节点复用 `mod_set_mood`，关闭即时隐藏，开启刷新台上未压暗的官方人物；`portrait` 的淡出阶段先隐藏气泡，淡入完成后按当前开关刷新。带 mood 节点的章节由 Host 开关控制，不发射旧的无条件隐藏语句，避免覆盖作者中途启用的选择；分支和回环以实际执行顺序为准。2026-11-01 实机已观察 player/shock 对应的红色叹号气泡随开启／关闭节点显示与隐藏；回环路径另由离线测试覆盖。

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

## 2026-11-04 自定义人物气泡接口

本次重新读取本机 `Mortal.Story.dll` 中的 `StoryStageController`、`StoryCharacterController`、`CharacterMoodPanel` 和 `CharacterPortraitMood`。舞台 `_moodPrefab` 是原版角色 `SetupMood` 实例化的来源，自定义人物同样克隆该预制体到自己的 Holder。`CharacterMoodPanel.SetPosition(float,float)` 设置锚点偏移，`Show(string,float)` 选择 Mapping 和朝向，`Hide()` 关闭当前图标。实例随人物销毁，官方预制体保持原值。

预览提取读取本机 CharacterMoodPanel 映射、子物件 RectTransform，以及 AnimatorController 的无条件结束切换和 AnimationClip 精灵关键帧。单次动画保留末帧，带尾段循环的动画保留循环起点；序列帧补齐 Sprite 原有透明边距以保持定位。

单人气泡的 `offset_x`／`offset_y` 通过同一 `SetPosition` 接口叠加在官方 `StoryCharacterData.MoodPosition` 或用户人物百分比锚点上。官方位置仅在该人物有 Host 临时设置时恢复，用户人物偏移随体型缩放，离场和全场清除都会归零。

本轮 Windows 隔离试玩观察了自定义人物气泡偏移、转身镜像、位移跟随、压暗隐藏与恢复、换表情、单独隐藏、退场重登和全场清除。自定义立绘的尺寸改为读取舞台 Canvas 的本地 RectTransform 高度；复验换表情后保持体型，避免屏幕像素高度随分辨率改变造成重算差异。

### 1.4.2 书页事件图

- 本次实时反编译确认 `Mortal.Story.StoryMainUI._picture` 为 `StoryPicture`，`ShowPicture(string)` 调用其 `Show`，加载 `pic_` 地址。`EventPictureRuntime` 克隆这个实例并禁用克隆的 `StoryPicture`，通过其 `_image` 设置原版或包内图片，使用 `preserveAspect` 适配左侧图框。克隆保留 `DialogUISizePosition`，清除时释放自己加载的 Sprite/Texture/Addressables handle。
- 原版 Story 场景的 `Picture/Mask/Image` 为左侧插图，`Picture/Text` 是右侧“事件”题字。仅在克隆中关闭题字的 `LeanLocalizedText` 后写入作者文字。可选下方文字复用 `saydialogs.narrative`，点击继续。原版书页 `pic_book_1` 仅从用户游戏提取到本地缓存。

### 2026-11-07 旧散装 MOD 格式与本机接口核对

旧格式依据 Binarizer `Plugin-DD2` 的 `Mortal` 分支，固定提交 `075e72253edb83212c132ef551b47e33d25009c6`。`Mods/<名称>/story/**/*.lua` 按文件 basename 取脚本编号，`CustomStory.csv` 为脚本、标题、作者三列，`StringTable.csv` 为完整本地化键与文字两列，同名键保留首项。资源包括 `Portraits/**/*.(png|jpg|gif)`、`Voice/**/*.mp3`、`DataTable/<SO类型>/<对象名>.json` 和 `LuaEquivalent/{Condition,Switch,Position}/**/*.lua`。旧 `SpriteConverter` 也可引用 MOD 根目录下其他相对图片路径。导入保持原目录只读，转换后的包内资源按 MOD 归属解析。[旧目录与资源加载源码](https://github.com/Binarizer/Plugin-DD2/blob/075e72253edb83212c132ef551b47e33d25009c6/ModSupport/Hooks/HookMods.cs)、[旧数据表与人物联动源码](https://github.com/Binarizer/Plugin-DD2/blob/075e72253edb83212c132ef551b47e33d25009c6/ModSupport/Hooks/HookDataTable.cs)

- 当前游戏的剧情立绘入口为 `CharacterPlaceholder.LoadCharacterAsset(string)`、`LoadCharacterPortrait(string,string)`、`GetPortrait(string,string)`，实际实例加载由 `StoryCharacterController.LoadPortrait/LoadPortraitSync/GetPortraitName` 和私有 `AddPortraitObject(Sprite)` 完成。人物编号来自 `StoryCharacterData.Id`，表情来自 `PortraitResourceList[].Mapping.Value`。旧文件名的 `人物_表情` 两段规则接入这些实例入口。切换舞台图像还需调用 `stage.show` 等舞台操作，`setcharacter` 设置发言人物。
- PNG/JPG 的同名 JSON 读取 `pivot`、`pixelsPerUnit`、`extrude`、`spriteType`，复用 `Sprite.Create` 对应参数。旧 GIF 加载使用默认 SpriteDesc，各帧 delay 按毫秒转换，每次 Unity Update 最多推进一帧并循环。Host 保留 Sprite 身份，替换其纹理像素供 Fungus 查找。旧 `normal` 头像联动对应当前 `PlayerAvatarPanel.OnEnable`、`SocialStatPanel.UpdateInfo`、`CombatCharacterStatusUI.Setup` 的 UI Image，按会话记录原值并在结束时恢复。
- 旧 `ext` 读取包括 `GetPlayerStat`、`GetEnemyTeamStat`、`GetRelationship`、`GetSkillLevel`、`GetBookLevel`、`GetDeveloped`、`GetDevelopLevel`、`GetFlag`、`GetCurrentTime`、`GetTimeValue`、`GetItemCount`，并保留旧生成器的 `GetDevelopd` 拼写。Host 在所属 `LuaEnvironment.Interpreter` 注册 `ext`。`LuaEquivalent` 分别接入当前 `CheckPointManager.Condition(string)`、`Switch(string)`、`Position(string)`，返回值依次为 bool、Int32、string。[旧扩展读取源码](https://github.com/Binarizer/Plugin-DD2/blob/075e72253edb83212c132ef551b47e33d25009c6/ModSupport/LuaExt.cs)
- 数据表运行时通过 `LegacyDataTableRuntime.Copy/RemapFields/SessionObject` 维护会话克隆及对象引用。此次反编译确认 `ReadStorySystem._dicStoryIndex` 是未序列化的运行态字典，`ExistKey` 直接访问该字典，`LuaManager.GetStoryText` 在解析本地化文字前先调用它。仅使用 `UnityEngine.Object.Instantiate` 克隆 SO 会遗漏这类状态，克隆需保留当时的托管集合与事件关系。`ReadStorySystem.Load(UniverseSave)` 重建已读字典，`SaveSystem.GetUniverseSaveData()` 只读反序列化文件，`LoadUniverseData()` 还涉及当前槽及缺档时写入，因此这些入口各有不同的生命周期含义。

本轮本机制样例已在游戏显示 GIF 来源的红色替换立绘与钱数 73。实际 Player.log 记录 `GameStat/Player_Money source=500 session=73 cloned=True source_unchanged=True`、`EXT 73`、`CONDITION true`、`SWITCH 2`、`POSITION Free`，确认这些查询与替换入口已执行。该轮首句对白曾因克隆后的已读字典为空而中断，后续状态复制修复已通过本轮旧 MOD 到 scope 样例的热重载及原值恢复实测，结果见下节。后续实机已观察 GIF 红绿帧、PNG/JPG 舞台切换、替换角色姓名与完整对白框，并由运行日志确认导入的 MP3 已转成 WAV 并进入 waveOut 播放。帧间隔以离线解码用例核对，未做人工计时或听觉验收。仅含资源／数据的附加包也通过正常原版＋MOD入口启动官方开篇，连续读取对白，本次日志运行异常为零。第三方实际旧 MOD 样本尚未取得，现有证据覆盖本机按上述公开格式自建的样例。

### 2026-11-07 F5 原版整章的继承角色与立绘

本次重新读取当前安装的 `CharacterPlaceholder`、`StoryCharacterController`、`Fungus.Character`、`PortraitController`、`PortraitState` 和 MoonSharp 转换器。`LoadCharacterAsset(string)` 在人物字典缺少该编号时创建实例，其 `Setup` 等待 `PortraitResourceList` 首项加载。`LoadCharacterPortrait(string,string)` 同步补充立绘资源。`GetPortraitName(string)` 会写入当前表情键，因此入口预载从结构化执行 AST 收集常量引用，再调用两个资源加载方法。

`ch5_6_8_3_4` 的 `original75` 只移动 `girl7_3`，沿用前章人物和当前立绘。Unity Player.log 曾记录“角色 Key girl7_3 不存在”。现场 IL 将第一次异常定位到 `CleanPortraitOptions` 的人物状态读取，预载人物后，第二次异常转到 `Show(PortraitOptions)` 将空立绘设为 `portraitImage` 后的访问。固定 F5 包在实际执行这个入场步骤时，仅对未指定立绘、尚未入场且当前立绘为空的人物，补入资源列表首张默认立绘。

`Character.Portraits` 的 C# 类型为 `List<Sprite>`。本机 MoonSharp 的 `ClrToScriptConversions.ObjectToDynValue` 将未注册为 userdata 的 `IList` 交给 `TableConversions.ConvertIListToTable`，后者按 `table[i + 1]` 写入。因此本轮实际 Lua 读取首张立绘使用 `Portraits[1]`。已直接加载游戏 DLL 中的 MoonSharp，执行当前 helper 并通过默认、显式立绘、已有立绘、已在舞台及空资源列表五个合成场景。相关编译回归共 14 项通过。

本轮 Windows 游戏实测已从另一 F5 样例热重载进入该章节，`original75` 正常登场并继续到 `original112` 的对白，观察到完整对白框、郁宝花、两名乞丐、实际房间背景及气泡。该次运行没有新增运行异常。另一次旧 MOD 样例切回 scope 样例，读数从 `source=500 session=73 source_unchanged=True` 恢复为 `source=500 session=500 cloned=False`，官方立绘和姓名同时恢复。
