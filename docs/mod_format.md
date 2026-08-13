# 活侠传 Mod 包格式（v3 契约）

**所有组件（编辑器 / 编译器 / 运行时插件）以本文档为准。** 改动需同步更新本文档。

## 1. 包结构

`.lommod` 文件 = zip 压缩包，内部结构：

```
manifest.json          # 必填，包元信息
story/<id>.json        # 必填≥1，剧情源文件（编辑器可编辑的源格式）
lua/<id>.lua           # 必填≥1，编译产物（运行时只读这里）；每个 story/<id>.json 对应一个
assets/                # 预留（自定义图片/音频），v1 运行时忽略
```

- `<id>` 规则：`[a-zA-Z0-9_\-]+`，包内唯一，即"剧情脚本 id"。
- 导出（打包）时必须重新编译：story/*.json → lua/*.lua，二者同名。
- 运行时插件**只读 manifest.json 和 lua/ 目录**；story/*.json 给编辑器回读/再编辑用。

## 2. manifest.json

```json
{
  "format": 1,
  "id": "demo_mod",
  "name": "示例 Mod",
  "version": "1.0.0",
  "author": "somebody",
  "description": "一句话简介",
  "entry": "main",
  "campaign": {
    "new_game": true,
    "triggers": [
      {"type": "position", "position": "Center", "script": "train", "when_flag_set": "SOME_FLAG"}
    ]
  }
}
```

- `format`：固定 `1`。
- `id`：mod 唯一 id（`[a-z0-9_\-]+`），运行时注册名前缀，防冲突。
- `entry`：入口剧情脚本 id，必须存在。
- `campaign`（可选）：战役模式。
  - `new_game`：true 时本 mod 出现在游戏内 mod 菜单的"开始新战役"区，点击后**隔离存档槽**（`SetSlot("mod_<modid>")`，不覆盖玩家正常存档）开新游戏，首个剧情脚本替换为本 mod 的 `entry`。
  - `triggers`：自由模式触发器数组。`type="position"`：点击地图位置 `position`（PositionType 枚举 id：Mall/Center/Alchemy/Forge/BackMountain/Room1/Door/Study/Kitchen/Room2/Secret）时，该位置的默认活动脚本替换为 `script`（同包脚本 id）。可选条件 `when_flag_set` / `when_flag_clear`：剧情 flag（即 `flag` 节点 AddStory 的 key，存档持久化）已设置/未设置时才生效。无官方主线/支线占用该位置时触发器才生效（官方事件优先）。

## 3. story/*.json — 剧情脚本格式

```json
{
  "id": "main",
  "title": "显示给玩家的标题",
  "start": "n1",
  "nodes": [ ... ]
}
```

- `nodes` 为节点数组；默认按数组顺序依次执行（隐式 goto 下一个节点）。
- 每个节点有唯一 `id`；任何节点可显式写 `"goto": "<nodeId>"` 覆盖顺序流。
- `choice` / `branch` / `dice` 的分支必须用 `goto` 指到目标节点 id。
- 多个前驱汇入同一节点（汇合点）合法。

### 3.1 节点类型（全量 38 种）

**演出类**

| type | 字段 | 说明 |
| --- | --- | --- |
| `music` | `name`；可选 `op`("play"默认/"stop"/"fadeout")，fadeout 时 `seconds`(默认2) | `luamanager.PlayMusic(name)` / `StopMusic()` / `FadeOutMusic(seconds)` |
| `sound` | `name`；可选 `kind`("sound"默认/"env")，`op`("play"默认/"fadeout"仅env，`seconds`默认1) | `luamanager.PlaySound/PlayEnvSound/FadeOutEnvSound` |
| `scene` | `view` | 切场景：`runblock(flowcharts.view,"out")` 后 `ViewName=view; runblock(...,"view")`。`view="out"` 只淡出；`"black"/"white"` 为纯色。非纯色 view 先 `runwait(flowcharts.LoadView(view))` 预加载背景资产（官方 995/1111 个脚本实证；不预载则背景黑屏） |
| `show` | `character`, `position`；可选 `portrait`(默认normal), `facing`(默认right), `fadeDuration`(0), `moveDuration`(0) | 加载并显示人物 |
| `move` | `character`, `from`, `to`；可选 `duration`(默认1) | 移动并 `wait(duration)` |
| `face` | `character`, `facing` | 转向 |
| `hide` | `character`；可选 `fadeDuration`(默认0) | 隐藏人物 |
| `focus` | `character` | `characters.Focus` |
| `offset` | `character`, `x`, `y`, `duration` | 人物偏移演出 `runwait(characters.MoveOffsetCoroutine(id,x,y,t))` |
| `say` | `text`；可选 `character`, `portrait`(默认normal), `mode`("character"默认/"think"/"narrative"/"center") | 对话/内心独白(带os_mask)/旁白/居中旁白。narrative 与 center 忽略 character |
| `choice` | `options`: `[{"text","goto"}]`（2~4 项）；可选 `dialog`(默认"Options"，皮肤见 §3.3) | 选项菜单 `choose()` |
| `shock` | `character`；可选 `duration`(默认0.5) | 人物震动（flowcharts.common "shock"） |
| `mask` | `show`(bool) | 独白遮罩 `os_mask.Show` |
| `intro` | `character` | 人物介绍卡 `runwait(intropanel.Show(...))` |
| `effect` | `name`；可选 `x`,`y`,`a`,`b`,`c`,`d`(数值，默认0/0/1/1/1/1) | 屏幕特效 `effects.SetupEffect(name,x,y,a,b,c,d)`，如 Hit_001/Blood_002/Sword_001 |
| `transition` | `phase`("in"/"out")；可选 `dir`(默认"lr"，lr/rl/tb/bt) | 黑场转场 `runwait(transitionblack.TransitionIn/Out(dir))`。**官方必须成对使用**：TransitionIn 会隐藏剧情 UI 并盖满黑幕，TransitionOut 才恢复；同一脚本有 in 无 out 时编译器给出警告（画面会一直黑屏，ch1_1 实测距离十几行） |
| `camera` | `name`, `active`(bool) | 镜头滤镜 `maincamera.ActiveVolume(name, 0 | 1)`，如 stage-memory/stage-dream/stage-fire/stage-blurdim |
| `block` | `flowchart`("view"/"common"), `name`；可选 `vars`: `[{"name","value"}]` | 通用 flowchart 块调用：`getvar` 逐个赋值后 `runblock(fc, name)`。覆盖 out_white/shake/flash/vshock 等 |
| `cg` | `action`("show"/"hide"), `kind`("picture"/"item"/"big"/"map"/"family"/"title")；可选 `key`, `key2`, `n1`, `n2` | mainui 图片/地图/家谱/标题：`ShowPicture(key)`/`HidePicture`/`ShowItemPicture`/`ShowBigPicture`/`ShowMap(key,key2)`/`ShowFamilyTree(key,key2,n1,n2)`/`DisplayTitle(key)` 等 |

**数值/状态类**

| type | 字段 | 说明 |
| --- | --- | --- |
| `stat` | `key`, `delta`；可选 `waitDisplay`(默认true), `display`(默认1), `mode`(默认"") | 主角属性增减 `statmodifymanager.Player(key, delta, mode, display)` |
| `stat_set` | `key`, `value`；可选 `update`(bool默认false) | 绝对设置 `SetPlayer(key, value)`；update=true 用 `UpdateSetPlayerStat`（title 等用） |
| `affinity` | `character`, `delta` | 人物好感度 `statmodifymanager.Character(character, delta, 1)` |
| `talent` | `talent`, `level`(±1) | 天赋 `statmodifymanager.AddTalent(id, level)` |
| `item` | `kind`("book"/"misc"/"special"), `item`, `count`(默认1)；可选 `remove`(bool默认false) | 物品增减 `AddBook/AddMisc/AddSpecial(id,count)`；remove 时 `RemoveBook/RemoveMisc(id)`（仅 book/misc） |
| `flag` | `flag` | mod 剧情 flag：`statmodifymanager.AddStory(flag)` + `modflags[flag]=true` |
| `game_flag` | `flag`, `value`；可选 `op`("set"默认/"add") | 官方任务 flag：`SetFlag(id, 状态)` / `AddFlag(id, ±增量)`。**id 必须是游戏已有 FlagData**（14_属性与Flag 表），否则游戏静默忽略 |
| `enemy` | `op`("team"/"level"/"people"/"id"), `enemy`, `value`(数值, id 的 op 不需要), `display`(默认1) | 敌方队伍修改 `ModifyEnemyTeam/Level/People/Id` |
| `battle_skill` | `op`("set"/"active"/"reset"), `key`(reset 不需要), `index`(set 用, 默认2), `active`(active 用, 默认1) | 战场技能 `SetPlayerBattleSkill/SetBattleSkillActive/ResetBattleSkill` |
| `mission` | `name`, `key` | 任务操作 `statmodifymanager.Mission(name, key)`：`Mission("Main","M0001")` 推进主线 / `Mission("S2200","clear")` 清支线 |
| `time` | `op`("set"/"round"/"month"/"mission")；set 用 `year,month,stage`；mission 用 `name,year,month,stage` | 时间 `SetGameTime/NextRound/NextMonth/SetMissionTime` |
| `autosave` | 可选 `kind`("story"默认/"free"/"prologue")；可选 `save_button`(0/1，单独控制存档按钮) | `AutoSave()/AutoFreeSave()/PrologueSave(mode)`；`save_button` 单独 emit `ToggleSaveButton(n)` |

**流程类**

| type | 字段 | 说明 |
| --- | --- | --- |
| `branch` | `flag`, `cases`: `[{"value","goto"}]`, 可选 `source`(默认 "mod") | mod：按 modflags 是否已设（value 1=已设置 2=未设置）；game：`checkpointmanager.Switch(flag)` 官方检查点数值分支 |
| `dice` | `check`, `options`: `[{"goto_大成功","goto_成功","goto_失败"}]`（恰好 1 条） | 骰子检定。**check 必须是带官方元数据的检查点**（editor_data 的 dice_meta：骰子范围 max 与结果带 bands，由官方脚本提取；无元数据的检查点会在游戏内骰子菜单 NRE 崩溃）。发射官方五步链 + 按结果带数逐带发射选项（官方文本+条件）；分支按带质量名次映射：最差带→goto_失败，中间带→goto_成功，最优带→3带及以上 goto_大成功 / 2带 goto_成功（2带无独立大成功档）。带质量按条件数值推断（同值 >系优于 <系；官方有 34 个倒序检查点） |
| `goto_scene` | `scene`("Free"/"Title"/"Combat"/"Battle"/"GameOver"/"End"/"Story"/"DemoEnd")；可选 `key`(Combat=战斗id/Battle=战役id/GameOver·End=结局id), `next`(默认"Story") | 场景跳转 `luamanager.ChangeScene(scene, key, next)`。Combat/Battle 后回 Story 重入当前脚本，注意用 game 检查点防重入 |
| `panel` | `panel`("martial"/"weapon"/"poison"/"cg"/"cgvideo"/"shop"/"newshop"/"credit"/"endgame")；可选 `key`(cg/cgvideo/endgame 的 id), `discount`(shop 用, 默认0), `mode`(martial 用, 默认0) | 打开系统面板，除 newshop 外均 `runwait`：`martialpanel.Open(mode)`/`weaponupgradepanel.Open()`/`poisonupgradepanel.Open()`/`cgpanel.Open(key)`/`cgvideopanel.Open(key,0)`/`shoppanel.Open(discount)`/`shoppanel.NewShop()`/`creditpanel.Open()`/`endgamepanel.Open(key)` |
| `wait` | `seconds` | `wait(seconds)` |
| `end` | 可选 `next_script` | 有：`SetNextScript("MOD_<modid>_<id>")`+`Init()` 链到同包脚本；无：`ChangeScene("Free","","")` 回自由模式 |
| `raw` | `code` | 原生 Lua 逃逸口：原样插入代码（多行合法）。**机制兜底**：任何节点表达不了的官方机制用它 |

### 3.2 常用取值（以 data/editor_data.json 为权威清单，schema 2 起带中文名）

- 站位 position：`SL L1 L2 M R1 R2 RM2 SR …`（共 36 个，S=屏外 L=左 M=中 R=右 B=后 C=央）
- 表情 portrait：`normal nervous1..3 angry1 angry2 laugh1 gloomy2 …`（按人物配置，缺失时游戏回退第一张立绘）
- say mode：`character` 对话 / `think` 内心独白 / `narrative` 旁白 / `center` 居中旁白
- stat key：`mental(心相) money(银两) disposition behaviour karma fame talking team …`（31 个）

### 3.3 选项菜单皮肤（choice.dialog）

**仅 `Options` 可用**（默认，纯文本选项）。官方脚本实证：全部 589 处 story 场景 choose 都用 Options（Dice 为骰子节点内部专用）。其余皮肤（Talk/Meet/Door/Section_*/Kitchen/Alchemy/Forge/Center 等）是自由场景的 break 格式菜单，选项文本格式为 `类型+key+行动点+贡献` 四段 `+` 分隔，纯文本选项会触发 `BreakOptionButton.UpdateContent` 的 IndexOutOfRange 崩溃（菜单冻结无法点击）——编译器直接报错拒绝。发射：`setmenudialog(menudialogs.Options)` → `choose()` → `menudialogs.Options.SetActive(false)`。

## 4. story.json → Lua 编译约定（lomc 实现）

- 每个节点编译为一个 Lua 函数；文件头前向声明 `local node_n1, node_n2, ...`，然后 `node_nX = function() ... end`；流转尾调用 `return node_<goto>()`；顶层 `return node_<start>()`。
- 文本转义：`\`→`\\`，`"`→`\"`，换行→`\n`，`\r`→`\r`。
- 每个脚本开头 emit `modflags = modflags or {}`（全局表，Story 场景会话内持续，链式脚本共享；不存档）。
- `flag` 节点双 emit：`AddStory` + `modflags[flag]=true`。
- **分支兜底**：choice 外任何多路结构不允许静默落空——未命中 case 时 else 落顺序下一节点；无法兜底（branch 为末节点且未覆盖全部返回值）视为校验错误。
- 节点 id 字符集 `[a-zA-Z0-9_]+`（脚本 id 允许 `-`）。
- story 顶层 `title` 可选。
- 最后一个节点不是 `end`/`goto_scene`/`raw` 且无 goto → 校验错误。
- `choice`/`branch`/`dice`/`end`/`goto_scene` 写显式 `goto` → 校验错误。
- `say` 的 narrative/center 模式给 character 允许但忽略。
- `raw` 节点内容原样插入（编译器不做语法检查）；其后流转照常（顺序/goto）。
- **非致命警告**：以 `-- lomc 警告：` 注释形式插在 Lua 头部（如 transition 有 in 无 out），编辑器 Lua 预览与导出产物都能看到；`lomc check` 同步打印到 stderr。

关键 API 范式（官方脚本实证，raw_scripts/ 下有 1814 个可参考）：

```lua
-- show
runwait(characters.LoadCharacterAsset("player"))
stage.show{character=characters.Get("player"), portrait=characters.GetPortrait("player", "normal"), fromPosition="RM2", toPosition="RM2", facing="right", fadeDuration=0, moveDuration=0, useDefaultSettings=false}
characters.Focus("player")
-- say (character 模式)
setsaydialog(saydialogs.character)
sayoptions.waitforinput = true
sayoptions.fadewhendone  = true
stage.showPortrait(characters.Get("player"), characters.GetPortrait("player", "nervous1"))
setcharacter(characters.Get("player"), characters.GetPortrait("player", "nervous1"))
characters.Focus("player")
say("对话文本")
-- say (think)：setsaydialog(saydialogs.think)，say 前 os_mask.SetLastPosition(); os_mask.Show(true)，后 os_mask.Show(false)
-- say (narrative/center)：setsaydialog(saydialogs.narrative|saydialogs.center); sayoptions 两行同上; setcharacter(narrative); say("...")（官方 ch1_1 实证：任何 say 前都设 sayoptions）
-- choice（含皮肤）
setmenudialog(menudialogs.Options)
local option1 = {}
option1[1] = "选项一"
option1[2] = "选项二"
local choice1 = choose(option1)
menudialogs.Options.SetActive(false)
if choice1 == 1 then return node_a() elseif choice1 == 2 then return node_b() end
-- scene
runblock(flowcharts.view, "out")
getvar(flowcharts.view, "ViewName").value = "center"
runblock(flowcharts.view, "view")
-- shock
getvar(flowcharts.common, "ShockPosition").value = characters.Get("brother4").State.holder.gameObject
getvar(flowcharts.common, "ShockDuration").value = 0.5
runblock(flowcharts.common, "shock")
-- effect / transition / camera
effects.SetupEffect("Hit_001", 10, -5, 1, 1, 1, 1)
runwait(transitionblack.TransitionIn("lr"))
maincamera.ActiveVolume("stage-memory", 1)
-- stat / affinity / talent / item / flag / game_flag / enemy / mission
statmodifymanager.Player("mental", -5, "", 1)
wait(statmodifymanager.GetDisplayTime("mental"))
statmodifymanager.Character("brother4", 1, 1)
statmodifymanager.AddTalent("1010", 1)
statmodifymanager.AddMisc("2001", 50)
statmodifymanager.AddStory("SOME_FLAG_ID")
modflags["SOME_FLAG_ID"] = true
statmodifymanager.SetFlag("M0001_00", 1)
statmodifymanager.ModifyEnemyTeam("400", -10, 1)
statmodifymanager.Mission("Main", "M0001")
-- branch（source="mod"）
if modflags["SOME_FLAG_ID"] then return node_a() else return node_b() end
-- branch（source="game"，else 兜底）
local branch1 = checkpointmanager.Switch("S0003_01_001")
if branch1 == 1 then return node_a() elseif branch1 == 2 then return node_b() else return node_next() end
-- dice（check 必须是带官方元数据的检查点；范围/带数/带文本来自 dice_meta）
setmenudialog(menudialogs.Dice)
local dice_rand1 = math.random(60)
dicemenudialog.SetRandom(60, dice_rand1)
local dice_result1 = checkpointmanager.Dice("Travel_601_101_001", dice_rand1)
local dice_opts1 = {}
dice_opts1[1] = "O_Travel_601_101_004|<60"
dice_opts1[2] = "O_Travel_601_101_005|>=60"
dicemenudialog.Setup(dice_result1.ResultCount, dice_result1.Result, dice_result1.Header, dice_result1.Additions)
runwait(dicemenudialog.ExecuteRoll(dice_opts1, 1, "Travel_601_101_001"))
local dice_sel1 = dicemenudialog.ResultSelection
-- 分支按带质量：最差带→失败，最优带→大成功（2带→成功）
if dice_sel1 == 1 then return node_fail() else return node_ok() end
-- goto_scene
luamanager.ChangeScene("Combat", "5102_01", "Story")
-- panel（除 newshop 外 runwait）
runwait(martialpanel.Open(0))
runwait(shoppanel.Open(10))
shoppanel.NewShop()
runwait(endgamepanel.Open("20003"))
-- autosave / time / battle_skill / block
luamanager.AutoSave()
luamanager.SetGameTime(1, 4, 1)
luamanager.SetPlayerBattleSkill("special3", 2)
runblock(flowcharts.common, "flash")
-- end（链式 / 回自由模式）
luamanager.SetNextScript("MOD_<modid>_<scriptid>")
luamanager.Init()
luamanager.ChangeScene("Free", "", "")
```

## 5. data/editor_data.json — 编辑器数据契约（schema 3）

由 `tools/extract_editor_data.py` 生成。schema 2 起 `characters`/`stats`/`positions`/`views`/`music`/`free_positions` 均为 `{id, name}` 对象数组（characters 另有 portraits）；schema 3 新增 `dice_meta`（骰子检查点元数据，从官方脚本调用点提取：`{check: {max, bands: [{text, cond}]}}`，bands 按官方展示顺序）：

```json
{
  "schema": 2,
  "characters": [{"id": "brother4", "name": "唐惟元", "portraits": ["normal", "nervous1"]}],
  "views": [{"id": "center", "name": "校場_白天"}],
  "music": [{"id": "普通_001", "name": "普通_001"}],
  "positions": [{"id": "RM2", "name": "右中2"}],
  "stats": [{"id": "mental", "name": "心相"}],
  "free_positions": [{"id": "Center", "name": "练功场"}],
  "modes": ["character", "think", "narrative", "center"],
  "menu_dialogs": ["Options", "Talk", "Meet", "Center", "..."],
  "effects": [{"id": "Hit_001", "name": "Hit_001"}],
  "dice_checks": ["Travel_601_101_001"],
  "dice_meta": {"Travel_601_101_001": {"max": 60, "bands": [{"text": "O_Travel_601_101_004", "cond": "<60"}, {"text": "O_Travel_601_101_005", "cond": ">=60"}]}},
  "combat_ids": ["5102_01"],
  "battle_ids": ["A0001_01"],
  "ending_ids": ["20003"],
  "game_flags": [{"id": "M0001_00", "name": "…"}],
  "talents": [{"id": "1010", "name": "…"}],
  "items_book": [{"id": "6002", "name": "…"}],
  "items_misc": [{"id": "2001", "name": "…"}],
  "items_special": [{"id": "2001", "name": "…"}],
  "messages": [{"id": "M_Add_Misc_2002", "name": "…"}],
  "affinity_characters": ["brother4"]
}
```

## 6. 运行时插件行为（MortalModHost）

1. 启动扫描 `BepInEx/plugins/MortalModHost/mods/*.lommod`，注册 `MOD_<modid>_<scriptid>` → lua 文本。
2. Harmony prefix `LuaManager.ExecuteLuaScript()`：注册名命中时用 mod lua 执行并跳过原方法。
3. 入口：Free 自由场景与 Title 标题画面左下角"活侠MOD"按钮 + F8（可配）打开菜单。Free 菜单分"演出 mod 剧情"与"开始新战役"两区；Title 菜单仅"开始新战役"区（演出剧情需要已加载的存档玩家状态，只在 Free 提供）。即开新战役可直接从标题画面独立开启，无需先进自由模式。
4. **战役**：在 Free 或 Title 点击"开始新战役"→ `SetSlot("mod_<modid>")`（隔离存档槽）→ 官方 `NewGameData()` → postfix 把首个剧情脚本替换为该 mod 的 entry → LoadStory。
5. **位置触发器**：postfix `FreePositionData.GetExecuteScript`，manifest.triggers 命中且 flag 条件满足（查 StoryKeyList）时返回 mod 脚本注册名；官方主线/支线优先。
6. **兜底**：Story 场景请求的 MOD_ 脚本未注册（mod 被删）时，不执行并 `ChangeScene("Free","","")` 防软锁。
7. mod 不修改官方脚本与文本表；mod 的 flag 进 StoryKeyList，存档兼容。

## 7. AI 工具接口（story_api）

editor/story_api.py 是 AI/编辑器共用的受控写入口。规则：**AI 不直接手写 story JSON 或 Lua**，
一切剧情构建经 story_api（models 契约默认值 + lomc 校验/警告），防止骰子菜单崩溃、
transition 黑幕、choice 皮肤崩溃、背景黑屏等已知坑。

- Python API：
  - `load_editor_data()`：读取编辑器数据（含 dice_meta 等清单），返回 (editor_data, is_fallback)
  - `new_story(story_id="main", title="新剧情")`：新建剧情脚本（含 1 个起始节点）
  - `add_node(story, node_type, fields=None, after=None)`：按 models 默认值新增节点（38 种类型），未知类型/字段/类型不符→ValueError，节点 id 自动生成，after 指定插入位置（节点 id 或 None=末尾）
  - `update_node(story, node_id, fields)`：更新节点字段（同 add 的字段校验），节点不存在→ValueError
  - `get_node(story, node_id)`：读取节点，不存在→ValueError
  - `list_nodes(story)`：返回 [{"id","type","summary"}] 清单
  - `delete_node(story, node_id)`：删除节点，不存在→ValueError
  - `move_node(story, node_id, delta)`：按相对位移调整节点顺序
  - `set_start(story, node_id)`：设置起始节点
  - `add_choice(story, options, after=None)`：新增选项分支（2~4 项，dialog 固定 Options）
  - `add_dice(story, check, goto_成功, goto_失败, goto_大成功="", after=None)`：新增骰子检定（check 必须有官方元数据，按结果带数校验 goto）
  - `add_say(story, text, character=None, mode="character", portrait="normal", after=None)`：新增对白（character 模式必填 character；narrative/center 不写 character）
  - `add_scene(story, view, after=None)`：新增场景切换
  - `check_story(story)`：只校验，返回 (errors: list[str], warnings: list[str])
  - `compile_story(story)`：校验+编译，返回 (lua|None, errors, warnings)，失败时 lua 为 None
  - `load_story_json(path)` / `save_story_json(story, path)`：story.json 读写（UTF-8）
  - `pack_mod(mod_dir, output=None)`：校验 manifest + 全部编译 + 打 .lommod，返回产物路径
- CLI：python editor/story_api.py check|compile|pack|new-story（AI 子进程友好，退出码 0/1，中文错误）
- 关键不变量（编译器强制，API 透传）：choice.dialog 仅 Options；dice.check 必须有官方元数据
  （骰子范围+结果带）；transition in/out 成对；scene 自动预载背景；say/show 自动加载表情差分。
