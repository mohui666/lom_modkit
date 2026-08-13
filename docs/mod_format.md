# 活侠传 Mod 包格式（v3 契约）

**所有组件（编辑器 / 编译器 / 运行时插件）以本文档为准。** 改动需同步更新本文档。

## 1. 包结构

`.lommod` 文件 = zip 压缩包，内部结构：

```
manifest.json          # 必填，包元信息
story/<id>.json        # 必填≥1，剧情源文件（编辑器可编辑的源格式）
lua/<id>.lua           # 必填≥1，编译产物（运行时只读这里）；每个 story/<id>.json 对应一个
texts.json             # 必填，已读文本表：{MOD_<modid>_<scriptid>_<nodeid>: 文本}（say/death 节点文本）
assets/                # 可选，自定义资源（结局插图/人物介绍图 PNG/JPG）
```

- `<id>` 规则：`[a-zA-Z0-9_\-]+`，包内唯一，即"剧情脚本 id"。
- 导出（打包）时必须重新编译：story/*.json → lua/*.lua，二者同名。
- 运行时插件**只读 manifest.json、lua/ 目录与 assets/ 下的图片**；story/*.json 给编辑器回读/再编辑用。编译器只打入剧情明确引用的 PNG/JPG（单张 ≤8MB），不会把目录中的未使用文件意外分发。
- texts.json 由打包时自动生成：收集每个 story 的全部 **say** 节点文本，key 与 lua 里 `GetStoryText` 的 key 一一对应；运行时插件把它注册进 LeanLocalization（已读系统按 key 查文本，见 §4/§6）。**death 文本不进 texts.json**：由 codegen 发射 `mod_set_death_text(<标题>, <文本>)` 两参 lua_str 字面量（官方死亡画面中央两段式显示，见 §3.1/§6）。

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
    "disable_official_events": true,
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
  - `disable_official_events`（可选，bool，缺省 false）：true 时本战役**禁用原版剧情事件**——返回 Free 时不自动启动无地点主线/支线，地图各位置也只保留本 mod 的位置触发器（`triggers` 命中才替换，否则该位置默认活动不可用，需 mod 自带兜底触发器）。
  - `triggers`：自由模式触发器数组。`type="position"`：点击地图位置 `position`（PositionType 枚举 id：Mall/Center/Alchemy/Forge/BackMountain/Room1/Door/Study/Kitchen/Room2/Secret）时，该位置的默认活动脚本替换为 `script`（同包脚本 id）。可选条件（全部命中才生效，多个条件之间是 AND；**数组顺序=优先级**，运行时取第一个全部命中的触发器）：
    - `when_flag_set` / `when_flag_clear`：剧情 flag（即 `flag` 节点 AddStory 的 key，存档持久化）已设置/未设置时才生效。
    - `when_month`：整数 1~12，仅该月份生效。
    - `when_stage`：整数 1~3（旬：上/中/下），仅该旬生效。
    - `when_affinity`：`{"character": <人物 id>, "min": <整数>}`，该人物好感度 ≥ min 才生效。
  - 默认情况下官方主线/支线优先；当 `disable_official_events` 或 F7 临时开关生效时，跳过官方任务判定并优先匹配 mod 触发器。
  - **触发器按战役隔离**：有活跃 mod 战役时，位置触发器只匹配当前战役 mod（其他已安装 mod 的触发器不参与匹配，避免跨 mod 抢占）；无战役时全部 mod 参与匹配、先加载者优先（加载顺序=文件名序）。
  - 触发器示例（练武场：好感事件 > 下旬晚练 > 默认闲逛）：

```json
"campaign": {
  "new_game": true,
  "disable_official_events": true,
  "triggers": [
    {"type": "position", "position": "Center", "script": "train_affinity", "when_affinity": {"character": "brother4", "min": 3}},
    {"type": "position", "position": "Center", "script": "train_dusk", "when_stage": 3},
    {"type": "position", "position": "Center", "script": "train_any"}
  ]
}
```

## 3. story/*.json — 剧情脚本格式

```json
{
  "id": "main",
  "title": "显示给玩家的标题",
  "mood": false,
  "start": "n1",
  "nodes": [ ... ]
}
```

- `mood`（可选，bool，默认 false）：心情气泡开关。false=每次 show 节点末尾与每次 say 节点前后发射 `mod_hide_mood()`（隐藏官方圆形情绪面板）；true=保留官方心情气泡。
- `nodes` 为节点数组；默认按数组顺序依次执行（隐式 goto 下一个节点）。
- 每个节点有唯一 `id`；任何节点可显式写 `"goto": "<nodeId>"` 覆盖顺序流。
- `choice` / `branch` / `dice` 的分支必须用 `goto` 指到目标节点 id。
- 多个前驱汇入同一节点（汇合点）合法。

### 3.1 节点类型（全量 43 种）

**演出类**

| type | 字段 | 说明 |
| --- | --- | --- |
| `music` | `name`；可选 `op`("play"默认/"stop"/"fadeout")，fadeout 时 `seconds`(默认2) | `luamanager.PlayMusic(name)` / `StopMusic()` / `FadeOutMusic(seconds)` |
| `sound` | `name`；可选 `kind`("sound"默认/"env")，`op`("play"默认/"fadeout"仅env，`seconds`默认1) | `luamanager.PlaySound/PlayEnvSound/FadeOutEnvSound` |
| `scene` | `view` | 切场景：`runblock(flowcharts.view,"out")` 后 `ViewName=view; runblock(...,"view")`。`view="out"` 只淡出；`"black"/"white"` 为纯色。非纯色 view 先 `runwait(flowcharts.LoadView(view))` 预加载背景资产（官方 995/1111 个脚本实证；不预载则背景黑屏） |
| `show` | `character`, `position`；可选 `portrait`(默认normal), `facing`(默认right), `fadeDuration`(0), `moveDuration`(0) | 加载并显示人物。**心情气泡**：story.mood 为 false 时末尾（Focus 后）追加 `mod_hide_mood()` |
| `move` | `character`, `from`, `to`；可选 `duration`(默认1) | 移动并 `wait(duration)` |
| `face` | `character`, `facing` | 转向 |
| `hide` | `character`；可选 `fadeDuration`(默认0) | 隐藏人物 |
| `focus` | `character` | `characters.Focus` |
| `offset` | `character`, `x`, `y`, `duration` | 人物偏移演出 `runwait(characters.MoveOffsetCoroutine(id,x,y,t))` |
| `say` | `text`；可选 `character`, `portrait`(默认normal), `mode`("character"默认/"think"/"narrative"/"center") | 对话/内心独白(带os_mask)/旁白/居中旁白。narrative 与 center 忽略 character。**已读机制**：文本不再裸字面量进 Lua，改发射 `say(luamanager.GetStoryText("MOD_<modid>_<scriptid>_<nodeid>"))`（独立 build/编辑器预览无 modid 时用 "MOD" 兜底），文本本体进包内 texts.json 由运行时注册 |
| `choice` | `options`: `[{"text","goto"}]`（2~4 项）；可选 `dialog`(默认"Options"，皮肤见 §3.3) | 选项菜单 `choose()` |
| `shock` | `character`；可选 `duration`(默认0.5) | 人物震动（flowcharts.common "shock"） |
| `mask` | `show`(bool) | 独白遮罩 `os_mask.Show` |
| `intro` | 可选 `intro_source`(`official` 默认/`custom`)。official 必填 `character`；custom 必填 `name`,`text`，可选 `title`,`image`（包内 `assets/` PNG/JPG，≤8MB）、`image_scale`(40~160，默认100)、`image_x`/`image_y`(-30~30，默认0) | official 直接调用原版 `runwait(intropanel.Show(character))`；custom 调用 `mod_prepare_character_intro(title,name,text,image,scale,x,y)`，复用同一个 CharacterIntroPanel。图片按屏幕安全区独立布局并保持比例；x 正数向右、y 正数向上，无图时隐藏头像区域 |
| `effect` | `name`；可选 `x`,`y`,`a`,`b`,`c`(数值，默认0/0/1/1/1)，`play`(bool，默认true) | 屏幕特效 `effects.SetupEffect(name,x,y,a,b,c,play)`，如 Hit_001/Blood_002/Sword_001。`play=false` 发射停止调用（末参 0）：**循环类特效播放后不会自动销毁**（官方实证 EventBubble_001/Glow_001 均有成对的 play=0 停止调用），如 EventBubble/Glow 必须后接 play=false 的同参节点停止，否则常驻画面（旧数据的 `d` 字段仍兼容：无 play 时用 d） |
| `transition` | `phase`("in"/"out")；可选 `dir`(默认"lr"，lr/rl/tb/bt) | 黑场转场 `runwait(transitionblack.TransitionIn/Out(dir))`。**官方必须成对使用**：TransitionIn 会隐藏剧情 UI 并盖满黑幕，TransitionOut 才恢复；同一脚本有 in 无 out 时编译器给出警告（画面会一直黑屏，ch1_1 实测距离十几行） |
| `camera` | `name`, `active`(bool) | 镜头滤镜 `maincamera.ActiveVolume(name, 0 | 1)`，如 stage-memory/stage-dream/stage-fire/stage-blurdim |
| `block` | `flowchart`("view"/"common"), `name`；可选 `vars`: `[{"name","value"}]` | 通用 flowchart 块调用：`getvar` 逐个赋值后 `runblock(fc, name)`。覆盖 out_white/shake/flash/vshock 等 |
| `cg` | `action`("show"/"hide"), `kind`("picture"/"item"/"big"/"map"/"family"/"title")；可选 `key`, `key2`, `n1`, `n2` | mainui 图片/地图/家谱/标题：`ShowPicture(key)`/`HidePicture`/`ShowItemPicture`/`ShowBigPicture`/`ShowMap(key,key2)`/`ShowFamilyTree(key,key2,n1,n2)`/`DisplayTitle(key)` 等 |
| `dim` | `character`, `dimmed`(bool 必填，默认 true) | 人物压暗 `stage.SetDimmed(character, dimmedState)`（反编译实证：实参 character 在前、bool 在后；dimmed=true 时官方实现还会隐藏该角色心情气泡） |
| `message` | `text`（必填非空，多行合法） | 系统提示 `mainui.DisplayMessageText(text)` 显示**原文**（DisplayMessage 走本地化 key 解析，用 Text 版避免自定文本被当 key 查空） |
| `rotate` | `character`, `angle`(int 必填，默认 180), `duration`(float 必填，默认 1，>0) | 人物旋转 `characters.Rotate(key, angle, duration)`——**注意官方参数序 angle 在前、duration 在后**（StoryCharacterController.Rotate(duration, angle) 内部交换；raw_scripts 调用点实证，如 `characters.Rotate("player", 90, 0.5)`） |
| `dayenv` | `day_type`（int 必填，1=白天 / 2=晚上） | 日夜环境 `luamanager.SetGameDayEnvironment(day_type)`。DayEnvironmentType 枚举实证（raw_scripts 调用点，ch1_1 等）：白天=1、晚上=2。**字段名 day_type**：字段名 "type" 与节点通用键 "type"（节点类型）冲突（dict 无法同名共存） |

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
| `branch` | `cases`(≥1)；可选 `source`("mod"默认/"game"/"stat"/"flag_value"/"condition")。键字段：source=stat 时用 `stat`（属性 id，editor_data stats 清单），其余来源用 `flag`（非空） | 条件分支，五来源：mod=按 modflags 是否已设；game=官方检查点 `checkpointmanager.Switch(flag)`；stat=主角属性 `luamanager.GetStatData(stat, 1)`；flag_value=官方任务旗标 `tonumber(luamanager.GetFlagData(flag))`；condition=官方条件检查点 `checkpointmanager.Condition(flag)`（bool）。case 结构按来源：mod/condition 用 `[{"value","goto"}]`（value 仅 1/2：mod=已设/未设，condition=真/假）；game 用 `[{"value","goto"}]`（任意整数）；stat/flag_value 用 `[{"op","value","goto"}]`（op 缺省 ">="，允许 >=/>/<=/</==）。未命中一律 else 落顺序下一节点（末节点且未覆盖全部取值 → LomcError；mod/condition 两 case 齐则覆盖） |
| `dice` | `check`, `options`: `[{"goto_大成功","goto_成功","goto_失败","band_texts"?}]`（恰好 1 条） | 骰子检定。**check 必须是带官方元数据的检查点**（editor_data 的 dice_meta：骰子范围 max 与结果带 bands，由官方脚本提取；无元数据的检查点会在游戏内骰子菜单 NRE 崩溃）。发射官方五步链 + 按结果带数逐带发射选项（文本+条件）；分支按带质量名次映射：最差带→goto_失败，中间带→goto_成功，最优带→3带及以上 goto_大成功 / 2带 goto_成功（2带无独立大成功档）。带质量按条件数值推断（同值 >系优于 <系；官方有 34 个倒序检查点）。**band_texts**（可选）：逐带覆写骰子菜单选项文本（条数必须等于结果带数，每项非空字符串，否则 LomcError）；发射 `<作者文本> | <官方cond>`（作者文本为字面量，游戏 GetStoryText 查不到时原样显示，不进 texts.json；文本内 ASCII \| 净化为全角｜；cond 永远用官方元数据）。缺省时用官方结果带文本 |
| `goto_scene` | `scene`("Free"/"Title"/"Combat"/"Battle"/"GameOver"/"End"/"Story"/"DemoEnd")；可选 `key`(Combat=战斗id/Battle=战役id/GameOver=死亡画面id/End=结局标识), `next`, `title`, `desc`(均为 str，仅 End/GameOver 用), `image`(str，**仅 End 用**：包内图片相对路径，如 `assets/ending.png`) | 普通场景仍为 `luamanager.ChangeScene(scene,key,next)`。**End 特例按原版汗青书流程**：缓存自定义标题/正文/插图 → `runwait(endgamepanel.Open("__MORTAL_MOD_END__"))` → 玩家确认 → 黑幕 → Title。运行时 patch 真正的 `EndGamePanel`，完整复用官方书卷版式、渐显和输入；`image` 写入官方左页 `_picImage`，留空时暂借原版结局 20047 的 Picture 占位。图片缺失/损坏只警告并回退占位，不中断。旧文件中的 End next 非 Title 值会被忽略并给出警告。GameOver 的 next 同样无效，因为原版按钮固定为读档/标题。只有不带任何自定义内容且给官方 key 时才直接打开官方结局条目（会按原版解锁/记录并给警告）。mod 专属 End key/空 key 若无 title/desc/image、mod 专属 GameOver key 若无 title/desc，均直接校验失败，避免空白卡。 |
| `panel` | `panel`("martial"/"weapon"/"poison"/"cg"/"cgvideo"/"shop"/"newshop"/"credit"/"endgame")；可选 `key`(cg/cgvideo/endgame 的 id), `discount`(shop 用, 默认0), `mode`(martial 用, 默认0) | 打开系统面板，除 newshop 外均 `runwait`：`martialpanel.Open(mode)`/`weaponupgradepanel.Open()`/`poisonupgradepanel.Open()`/`cgpanel.Open(key)`/`cgvideopanel.Open(key,0)`/`shoppanel.Open(discount)`/`shoppanel.NewShop()`/`creditpanel.Open()`/`endgamepanel.Open(key)` |
| `wait` | `seconds` | `wait(seconds)` |
| `end` | 可选 `next_script` | 有：`SetNextScript("MOD_<modid>_<id>")`+`Init()` 链到同包脚本；无：`ChangeScene("Free","","")` 回自由模式 |
| `death` | `text`（必填非空，多行合法）、`death_id`（必填）；可选 `title`（str，缺省「勝敗乃兵家常事」）、旧字段 `next` | **死亡文本**：黑屏过渡（view="black"）→ `mod_set_death_text(title, text)`（两参 lua_str 字面量，短标题 + 多行描述，**不进 texts.json / 已读系统**）→ `luamanager.ChangeScene("GameOver", death_id, "Title")` 进**官方 GameOver 死亡画面**（黑底红字 + 读档/标题按钮）；原版不读取自定义 next，旧值会被忽略并警告。运行时插件 patch GameOverController 把两段文本显示在死亡画面中央（官方布局，见 §6）。`death_id` 必须是 ≥900000 的 mod 专属数字 id（否则 LomcError，见下方「死亡/结局 id 约定」）。终止节点（自带流转，不允许显式 goto，可作末节点收尾） |
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
- 每个脚本开头 emit `modflags = modflags or {}`（全局表，Story 场景会话内持续，链式脚本共享；不存档），紧跟一行 `mod_set_mood(true|false)`（story 顶层 mood 声明，默认 false；见 §6）。
- `flag` 节点双 emit：`AddStory` + `modflags[flag]=true`。
- **分支兜底**：choice 外任何多路结构不允许静默落空——未命中 case 时 else 落顺序下一节点；无法兜底（branch 为末节点且未覆盖全部返回值）视为校验错误。
- 节点 id 字符集 `[a-zA-Z0-9_]+`（脚本 id 允许 `-`）。
- story 顶层 `title` 可选。
- **已读 key 规则**：所有 say（character/think/narrative/center）节点的文本一律发射 `say(luamanager.GetStoryText(key))`，key = `MOD_<modid>_<scriptid>_<nodeid>`；modid 来自 manifest（打包时），独立 build/编辑器预览缺省时用 "MOD" 兜底（预览显示兜底 key 可接受）。key 与文本本体（包内 texts.json）由打包器同步生成。**death 文本不走已读 key**：发射 `mod_set_death_text(<标题字面量>, <文本字面量>)`（均 lua_str 转义；标题缺省/空串时用「勝敗乃兵家常事」），文本不进 texts.json。
- **结局/死亡卡片规则**：goto_scene scene=End 且带 title/desc/image 时先发射 `mod_set_ending_text(...)`，再按原版 `EndGamePanel.Open` 汗青书流程显示；image 是左页插图，不是全屏背景。scene=GameOver 带 title/desc 时改走语义正确的 `mod_set_death_text(<title>, <desc>)`。death 节点同样发射 `mod_set_death_text(<title>, <text>)`（两参；单参旧包兼容仍由运行时支持）。两个全局调用由运行时插件注册（§6）。
- **mood 规则**：story.mood（可选 bool，默认 false）。每个脚本头部（modflags 行之后）必发射 `mod_set_mood(false)` 或 `mod_set_mood(true)`（运行时插件注册的全局声明，硬控心情面板，见 §6）；mood=false 时另在 show 节点末尾（Focus 之后）与 say 节点 say(...) 前后各发射一次 `mod_hide_mood()`（隐藏官方圆形情绪面板）；true 时不发射 mod_hide_mood。
- **death 发射**：见 §3.1 death 行（runblock out → ViewName="black" → runblock view → `mod_set_death_text(title, text)` → ChangeScene("GameOver", death_id, next)）。
- 最后一个节点不是 `end`/`death`/`goto_scene`/`raw` 且无 goto → 校验错误。
- `choice`/`branch`/`dice`/`end`/`death`/`goto_scene` 写显式 `goto` → 校验错误。
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
mod_hide_mood()
say(luamanager.GetStoryText("MOD_demo_mod_main_n7"))
mod_hide_mood()
-- say (think)：setsaydialog(saydialogs.think)，say 前 os_mask.SetLastPosition(); os_mask.Show(true)，后 os_mask.Show(false)
-- say (narrative/center)：setsaydialog(saydialogs.narrative|saydialogs.center); sayoptions 两行同上; setcharacter(narrative); say(GetStoryText(...))（官方 ch1_1 实证：任何 say 前都设 sayoptions）
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
-- effect / transition / camera（循环特效必须成对 play=0 停止，如 EventBubble/Glow）
effects.SetupEffect("Hit_001", 10, -5, 1, 1, 1, 1)
effects.SetupEffect("Glow_001", -4.5, 0, 1, 1, 1, 0)
runwait(transitionblack.TransitionIn("lr"))
maincamera.ActiveVolume("stage-memory", 1)
-- dim / message / rotate / dayenv（四个高价值节点，官方 API 实证）
stage.SetDimmed(characters.Get("trainee1"), true)
mainui.DisplayMessageText("【系统提示】欢迎来到全功能展示！")
characters.Rotate("player", 180, 1)  -- 参数序：angle 在前、duration 在后
luamanager.SetGameDayEnvironment(1)  -- 1=白天 / 2=晚上
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
-- branch（source="stat"：主角属性数值比较，op 缺省 >=）
local branch1 = luamanager.GetStatData("mental", 1)
if branch1 >= 50 then return node_a() else return node_next() end
-- branch（source="flag_value"：官方任务旗标数值比较）
local branch1 = tonumber(luamanager.GetFlagData("50019"))
if branch1 >= 1 then return node_a() else return node_next() end
-- branch（source="condition"：官方条件检查点，value 1=真 2=假）
if checkpointmanager.Condition("S0030_01_001") then return node_a() else return node_b() end
-- dice（check 必须是带官方元数据的检查点；范围/带数/带文本来自 dice_meta，仅故事场景检查点；band_texts 可选逐带覆写）
setmenudialog(menudialogs.Dice)
local dice_rand1 = math.random(99)
dicemenudialog.SetRandom(99, dice_rand1)
local dice_result1 = checkpointmanager.Dice("S0205_01_001", dice_rand1)
local dice_opts1 = {}
dice_opts1[1] = "O_S0205_01_001|<40"        -- 缺省：官方结果带文本
dice_opts1[2] = "O_S0205_01_002|>=40"
-- band_texts 存在时：dice_opts1[i] = "<作者文本>|<官方cond>"（作者文本为字面量，GetStoryText 查不到时原样显示）
dicemenudialog.Setup(dice_result1.ResultCount, dice_result1.Result, dice_result1.Header, dice_result1.Additions)
runwait(dicemenudialog.ExecuteRoll(dice_opts1, 1, "S0205_01_001"))
local dice_sel1 = dicemenudialog.ResultSelection
-- 分支按带质量：最差带→失败，最优带→大成功（2带→成功）
if dice_sel1 == 1 then return node_fail() else return node_ok() end
-- goto_scene（GameOver 的 key 用 mod 专属 id：9+官方 id，如 910021）
luamanager.ChangeScene("Combat", "5102_01", "Story")
-- 自定义 End：官方汗青书 EndGamePanel；留空 image 时游戏内借用原版 20047 插图占位
mod_set_ending_text("武林传奇", "你的名字，从今往后便是传说。")
luamanager.SetEnableActions(0)
runwait(endgamepanel.Open("__MORTAL_MOD_END__"))
luamanager.SetEnableActions(1)
runwait(transitionblack.TransitionIn("lr"))
luamanager.ChangeScene("Title", "", "")
-- 自定义左页插图（scene=End 且带 image 时发射三参）
mod_set_ending_text("武林传奇", "你的名字，从今往后便是传说。", "assets/ending.png")
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
-- death（死亡文本：黑屏过渡 → mod_set_death_text(title, text) 两段式 → 官方 GameOver 死亡画面）
runblock(flowcharts.view, "out")
getvar(flowcharts.view, "ViewName").value = "black"
runblock(flowcharts.view, "view")
mod_set_death_text("勝敗乃兵家常事", "你坠入山崖，万事休矣。")
luamanager.ChangeScene("GameOver", "910021", "Title")
```

## 5. data/editor_data.json — 编辑器数据契约（schema 3）

由 `tools/extract_editor_data.py` 生成。schema 2 起 `characters`/`stats`/`positions`/`views`/`music`/`free_positions` 均为 `{id, name}` 对象数组（characters 另有 portraits）；schema 3 新增 `dice_meta`（骰子检查点元数据，从官方脚本调用点提取：`{check: {max, bands: [{text, cond}]}}`，bands 按官方展示顺序）与 `death_ids`/`ending_ids` 富化对象数组（`{id, name}`，name 取自权威参考文件 `data/ref/death_ending_ids.json`，见下方「死亡/结局 id 约定」）。**dice_meta 仅含故事场景检查点**：旅行系统检查点（Travel_*，只存在于旅行系统配置，故事场景的 CheckPointManager 查不到会崩）已剔除——提取时排除旅行脚本（文件名含 travel_ 前缀/_travel 子串，或被 SetCurrentTravelScript/SetTempScript 引用的脚本）。`dice_checks` 是全名清单，保留全部调用点（含旅行）：

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
  "dice_checks": ["S0205_01_001", "Travel_601_101_001"],
  "dice_meta": {"S0205_01_001": {"max": 99, "bands": [{"text": "O_S0205_01_001", "cond": "<40"}, {"text": "O_S0205_01_002", "cond": ">=40"}]}},
  "combat_ids": ["5102_01"],
  "battle_ids": ["A0001_01"],
  "death_ids": [{"id": "10021", "name": "乱战中被践踏而死"}],
  "ending_ids": [{"id": "20003", "name": "唐门叛徒"}],
  "game_flags": [{"id": "M0001_00", "name": "…"}],
  "talents": [{"id": "1010", "name": "…"}],
  "items_book": [{"id": "6002", "name": "…"}],
  "items_misc": [{"id": "2001", "name": "…"}],
  "items_special": [{"id": "2001", "name": "…"}],
  "messages": [{"id": "M_Add_Misc_2002", "name": "…"}],
  "affinity_characters": ["brother4"]
}
```

### 死亡/结局 id 约定（mod 专属区间）

官方 GameOver/EndGamePanel 会用 id 查 LibrarySystem 并可能执行 LibraryItemData.Add()（解锁/记录官方结局）。自定义 End 现在固定用不存在的内部 key，不会查询或写入官方结局；只有“无自定义内容、直接打开官方 End key”会按原版记录。

- **mod 死亡/结局 id = `9<官方id>`**（900000 区段）：官方死亡 10021 → mod 910021；官方结局 20003 → mod 920003。与官方 1xxxx（死亡）/2xxxx（结局）/4xxxx（后日谈）全区间不撞。
- 自造 GameOver id 查不到官方条目 → 无副作用，文本由插件注入；自造 End id 只作 mod 内标识，真正显示走固定内部 key。
- `death` 节点的 `death_id` 校验 ≥900000 整数。空白的自造 GameOver/End 卡会被编译器拒绝；无自定义内容直接使用官方 key 时给出非致命存档污染警告。
- 权威参考：`data/ref/death_ending_ids.json`（由 lom-save-analyzer 仓库 mappings.js 提取）：`death` 106 个（10000~10104、11000，带标题+描述）、`ending` 54 个（20000~20053）、`epilogue` 4 个（40000~40003）。提取器用其标题富化 editor_data 的 death_ids/ending_ids；编辑器 death_id 输入框列出前 5 个官方参考。

## 6. 运行时插件行为（MortalModHost）

1. 启动扫描 `BepInEx/plugins/MortalModHost/mods/*.lommod`，注册 `MOD_<modid>_<scriptid>` → lua 文本。
2. Harmony prefix `LuaManager.ExecuteLuaScript()`：注册名命中时用 mod lua 执行并跳过原方法。
3. 入口：Free 自由场景与 Title 标题画面左下角"活侠MOD"按钮 + F8（可配）打开菜单。Free 菜单分"演出 mod 剧情"与"开始新战役"两区；Title 菜单仅"开始新战役"区（演出剧情需要已加载的存档玩家状态，只在 Free 提供）。即开新战役可直接从标题画面独立开启，无需先进自由模式。
4. **战役**：在 Free 或 Title 点击"开始新战役"→ `SetSlot("mod_<modid>")`（隔离存档槽）→ 官方 `NewGameData()` → postfix 把首个剧情脚本替换为该 mod 的 entry → LoadStory。
5. **原版剧情抑制与位置触发器**：`disable_official_events` 或 F7 生效时，`UpdateCheckMissions` 内暂时隐藏主线触发状态，防止未播放的无地点主线被推进；`HasAnyMissionTrigger` 返回 false，避免返回 Free 时自动启动官方主线/支线。地点点击再跳过官方主线/支线，postfix `FreePositionData.GetExecuteScript` 优先匹配 manifest.triggers；无 mod 命中时抑制官方地点默认脚本。
6. **兜底**：Story 场景请求的 MOD_ 脚本未注册（mod 被删）时，不执行并 `ChangeScene("Free","","")` 防软锁。
7. mod 不修改官方脚本与文本表；mod 的 flag 进 StoryKeyList，存档兼容。
8. **texts.json 注册**：加载 .lommod 时把包内 texts.json 的 key→文本注册进 LeanLocalization，key 解析为 `Story/`+key 的本地化文本；`GetStoryText` 按 key 查已读系统：已读→黄色+可快进，未读→正常色+记入已读，查不到返回 key 本身。
9. **mod_hide_mood**：注册全局 Lua 函数 `mod_hide_mood()`（无参），隐藏全场角色圆形情绪面板（CharacterMoodPanel）；编译器按 story.mood 开关在 show/say 处发射（见 §4）。
10. **mod_set_mood**：注册全局 Lua 函数 `mod_set_mood(bool)`，按脚本头部声明（story 顶层 mood，默认 false）硬控官方心情面板开关（ShowMood）——每个 mod 脚本入口处发射一次（见 §4），链式脚本逐脚本切换生效；与 mod_hide_mood 双保险防官方情绪面板干扰剧情演出。
11. **UpdateTranslations 防 wipe**：官方文本刷新（UpdateTranslations / LeanLocalization 重建）会清掉插件注册的 mod 文本，必须 hook 并在刷新后重放 texts.json 的 key→文本注册（加载时缓存全部注册项），保证 GetStoryText 的 mod key 永不失效。
12. **人物介绍卡**：官方人物保持原始 `CharacterIntroPanel.Show(key)` 行为（仅首次激活关系时显示）；自定义人物只在特殊 key 上由 Harmony 接管，使用官方面板、关闭按钮和版式，写入自定义称号/姓名/正文。可选 `image` 从当前 `.lommod` 的 `assets/` 解码后放入独立安全布局：默认中心为屏幕 `(31%,50%)`，最大宽/高为屏幕 `(30%,62%)`，保持比例；`image_scale` 在自动适配尺寸上缩放，`image_x/image_y` 以屏幕百分比微调。关闭时销毁临时纹理并完整恢复原版 Image 与 RectTransform；无图时隐藏头像区域，不修改官方本地化表或关系数据。
13. **结局/死亡卡片绘制**：注册两个全局 Lua 函数，供编译产物调用（见 §3.1/§4）：
    - `mod_set_death_text(title, desc)`：缓存自定死亡标题/描述（两段式：短标题 + 多行描述）；Harmony postfix `GameOverController`（官方死亡画面已有 `_titleText`/`_descTextPrefab` 文本控制器）把两段文本写入官方标题/描述控制器，用官方布局显示在死亡画面中央（黑底红字 + 返回按钮不变）。单参调用按旧契约当 desc、标题留空（旧 mod 包兼容）。
    - `mod_set_ending_text(title, desc[, image])`：缓存结局标题/描述与可选包内图片。Harmony postfix 包装真正的 `EndGamePanel.Open`，在官方第一次画布 fade 前把标题/正文写入 `_titleText/_descText`，把图片写入左页 `_picImage`；未给图片时直接借用游戏内官方结局 20047 的 Picture 作占位（不复制游戏资源）。官方渐显、等待确认与淡出全保留；为避免 mod key 进入无法解析的传奇存档槽，本次显示临时关闭 `_saveLibrary`，结束后恢复。
    - 新编译器的自定义 End 不再进入简化 `EndGameController` 场景；旧包仍保留原 End 场景覆盖兼容。GameOver 自造 id 无文字与 End 自造 id 无内容均在编译期阻止，解决空屏回归。
14. **编辑器单次试玩协议**：编辑器把入口章节的 `start` 临时改为当前选中节点，安装为固定包 `__lom_modkit_preview.lommod`（manifest id `lom_modkit_preview`），随后原子写入插件目录 `preview-request.json`。运行时每 0.35 秒检查一次：Free 场景直接演出，Title 场景用 `mod_lom_modkit_preview` 隔离槽开局，其它场景等待到安全场景；消费后删除请求与固定临时包。请求只接受 format=1 及 `[A-Za-z0-9_-]+` 的 mod/script/node id，正式 Mod 包不在自动删除范围内。

## 7. AI 工具接口（story_api）

editor/story_api.py 是 AI/编辑器共用的受控写入口。规则：**AI 不直接手写 story JSON 或 Lua**，
一切剧情构建经 story_api（models 契约默认值 + lomc 校验/警告），防止骰子菜单崩溃、
transition 黑幕、choice 皮肤崩溃、背景黑屏等已知坑。

- Python API：
  - `load_editor_data()`：读取编辑器数据（含 dice_meta 等清单），返回 (editor_data, is_fallback)
  - `new_story(story_id="main", title="新剧情", mood=False)`：新建剧情脚本（含 1 个起始节点）；mood 为心情气泡开关（false=自动隐藏官方心情气泡，见 §3）
  - `add_node(story, node_type, fields=None, after=None)`：按 models 默认值新增节点（43 种类型），未知类型/字段/类型不符→ValueError，节点 id 自动生成，after 指定插入位置（节点 id 或 None=末尾）
  - `update_node(story, node_id, fields)`：更新节点字段（同 add 的字段校验），节点不存在→ValueError
  - `get_node(story, node_id)`：读取节点，不存在→ValueError
  - `list_nodes(story)`：返回 [{"id","type","summary"}] 清单
  - `delete_node(story, node_id)`：删除节点，不存在→ValueError
  - `move_node(story, node_id, delta)`：按相对位移调整节点顺序
  - `set_start(story, node_id)`：设置起始节点
  - `add_choice(story, options, after=None)`：新增选项分支（2~4 项，dialog 固定 Options）
  - `add_dice(story, check, goto_成功, goto_失败, goto_大成功="", band_texts=None, after=None)`：新增骰子检定（check 必须有官方元数据，按结果带数校验 goto；band_texts 可选逐带覆写选项文本，条数必须等于结果带数且每项非空）
  - `add_say(story, text, character=None, mode="character", portrait="normal", after=None)`：新增对白（character 模式必填 character；narrative/center 不写 character）
  - `add_death(story, text, death_id, next="Title", title=None, after=None)`：新增死亡文本节点（text 必填非空多行；death_id 必填 ≥900000 的 mod 专属数字 id，约定 9+官方 id；原版只支持读档/标题按钮，因此 next 仅接受 Title；title 可选短标题，缺省/空串用「勝敗乃兵家常事」）
  - `add_scene(story, view, after=None)`：新增场景切换
  - `check_story(story)`：只校验，返回 (errors: list[str], warnings: list[str])
  - `compile_story(story)`：校验+编译，返回 (lua|None, errors, warnings)，失败时 lua 为 None
  - `load_story_json(path)` / `save_story_json(story, path)`：story.json 读写（UTF-8）
  - `pack_mod(mod_dir, output=None)`：校验 manifest + 全部编译 + 打 .lommod，返回产物路径
- CLI：python editor/story_api.py check|compile|pack|new-story（AI 子进程友好，退出码 0/1，中文错误）
- 关键不变量（编译器强制，API 透传）：choice.dialog 仅 Options；dice.check 必须有官方元数据
  （骰子范围+结果带）；transition in/out 成对；scene 自动预载背景；say/show 自动加载表情差分；
  **show/say 的 (character, portrait) 必须落在 data/editor_data.json 的角色表情表内**
  （表不可用/角色不在表 → 放行；角色在表但表情不在其列表 → LomcError/ValueError——
  游戏 LoadCharacterPortrait 对无效表情 key 抛 KeyNotFoundException → Lua 协程死 → 对话冻结，
  练武场卡死即此因）。另注意：say/show 引用的人物必须先 show 上台（未上台同样抛
  KeyNotFoundException），showcase 构建脚本已带防回归自检。
