# 活侠传 Mod 包格式（v3 契约）

> 语言：简体中文（本文） · [繁體中文](../zh_TW/mod_format.md) · [日本語](../ja/mod_format.md) · [한국어](../ko/mod_format.md)

**所有组件（编辑器 / 编译器 / 运行时插件）以本文档为准。** 改动需同步更新本文档。
文中规则的官方脚本/反编译实证材料见 `../research/`，正文不重复展开。

## 1. 包结构

`.lommod` 文件 = zip 压缩包，内部结构：

```
manifest.json          # 必填，包元信息
story/<id>.json        # 必填≥1，剧情源文件（编辑器可编辑的源格式）
lua/<id>.lua           # 必填≥1，编译产物（运行时只读这里）；每个 story/<id>.json 对应一个
texts.json             # 必填，已读文本表：{MOD_<modid>_<scriptid>_<nodeid>: 文本}（say 节点文本）
assets/                # 可选，自定义资源
                       #   图片：结局插图 / 人物介绍图 PNG/JPG
                       #   用户音频：assets/user/audio/<content_id>/
                       #   自定义角色：assets/user/character/<content_id>/
```

- `<id>` 规则：`[a-zA-Z0-9_\-]+`，包内唯一，即"剧情脚本 id"。
- 导出（打包）时必须重新编译：story/*.json → lua/*.lua，二者同名。
- 运行时插件**只读 manifest.json、lua/ 目录与 assets/**；story/*.json 给编辑器回读/再编辑用。编译器只打入剧情明确引用的 PNG/JPG（单张 ≤8MB）、明确引用的 `user:` 音频，以及明确引用的自定义角色立绘。导出的 `.lommod` 自包含，玩家机器不需要编辑器仓库。
- texts.json 由打包时自动生成：收集每个 story 的全部 **say** 节点文本，key 与 lua 里 `GetStoryText` 的 key 一一对应；运行时注册进 LeanLocalization（见 §4/§6）。**death 文本不进 texts.json**：由 codegen 发射 `mod_set_death_text(<标题>, <文本>)` 两参 lua_str 字面量（见 §3.1/§6）。

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
  - `disable_official_events`（可选，bool，缺省 false）：true 时本战役**禁用原版剧情事件**——返回 Free 时不自动启动无地点主线/支线，地图位置只保留本 mod 触发器（未命中则该位置默认活动不可用，需 mod 自带兜底触发器）。
  - `triggers`：自由模式触发器数组。`type="position"`：点击地图位置 `position`（PositionType 枚举 id：Mall/Center/Alchemy/Forge/BackMountain/Room1/Door/Study/Kitchen/Room2/Secret）时，该位置默认活动脚本替换为 `script`（同包脚本 id）。可选条件全部命中才生效（多条件 AND；**数组顺序=优先级**，运行时取第一个全部命中的触发器）：
    - `when_flag_set` / `when_flag_clear`：剧情 flag（即 `flag` 节点 AddStory 的 key，存档持久化）已设置/未设置。
    - `when_month`：整数 1~12，仅该月份生效。
    - `when_stage`：整数 1~3（旬：上/中/下），仅该旬生效。
    - `when_affinity`：`{"character": <人物 id>, "min": <整数>}`，好感度 ≥ min。
  - 默认官方主线/支线优先；`disable_official_events` 或 F7 临时开关生效时跳过官方任务判定，优先匹配 mod 触发器。
  - **触发器按战役隔离**：有活跃 mod 战役时只匹配当前战役 mod 的触发器；无战役时全部 mod 参与匹配、先加载者优先（加载顺序=文件名序）。
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

### 3.1 节点类型（全量 44 种）

**演出类**

| type | 字段 | 说明 |
| --- | --- | --- |
| `music` | `name`；可选 `op`("play"默认/"stop"/"fadeout")，fadeout 时 `seconds`(默认2) | 官方名继续 `PlayMusic` / `StopMusic` / `FadeOutMusic(seconds)` 后 **`wait(seconds)`**。`name` 以 `user:` 开头时是用户内容引用（§8），由运行时从**当前包** `assets/user/` 解析播放。禁止本机绝对路径 |
| `sound` | `name`；可选 `kind`("sound"默认/"env")，`op`("play"默认/"fadeout"仅env，`seconds`默认1) | 官方名继续 `PlaySound` / `PlayEnvSound` / `FadeOutEnvSound(seconds)` 后同样 **`wait(seconds)`**。`user:` 引用规则同 music，且 `audio_kind` 必须与节点匹配 |
| `scene` | `view` | 切换**原版官方背景**：先清除自定义背景，再 `runblock(flowcharts.view,"out")`；其余原版预载与切换行为不变。`view="out"` 只淡出；`"black"/"white"` 为纯色 |
| `background` | `action`(`set`/`show`/`replace`/`fadein`/`fadeout`/`clear`)；显示类必填 `image`(`user:` 图片)，可选 `fade`(默认0.5) | 使用当前包 `assets/user/image/` 的图片覆盖为舞台背景。`set/clear` 立即执行；其余动作按 `fade` 淡入/淡出并等待。换章、`goto_scene`、官方 `scene`、场景切换与热重载都会清理，不修改原版 View 资源 |
| `show` | `character`, `position`；可选 `portrait`(默认normal), `facing`(默认right), `fadeDuration`(0), `moveDuration`(0) | 加载并显示人物。story.mood 为 false 时末尾（Focus 后）追加 `mod_hide_mood()` |
| `move` | `character`, `from`, `to`；可选 `duration`(默认1) | 移动并 `wait(duration)` |
| `face` | `character`, `facing` | 转向 |
| `hide` | `character`；可选 `fadeDuration`(默认0) | 隐藏人物 |
| `focus` | `character` | `characters.Focus` |
| `offset` | `character`, `x`, `y`, `duration` | 人物偏移演出。官方角色走 `runwait(characters.MoveOffsetCoroutine(id,x,y,t))`；`user:` 角色走 `mod_char_offset` 后等待相同时长 |
| `say` | `text`；可选 `character`, `portrait`(默认normal), `mode`("character"默认/"think"/"narrative"/"center")，可选 `voice` | 对话/内心独白(带os_mask)/旁白/居中旁白。narrative 与 center 忽略 character。**已读机制**：文本不裸进 Lua，发射 `say(luamanager.GetStoryText("MOD_<modid>_<scriptid>_<nodeid>"))`（无 modid 时兜底 "MOD"），文本本体进 texts.json 由运行时注册。**`voice`**（可选）：用户音频引用，如 `user:mohui.line_01`；进入本句前 `mod_play_voice`（先停上一句），`say()` 返回后 `mod_stop_voice`。语音走独立通道，`sound` / `StopMusic` 不停它。禁止绝对路径与官方音效名 |
| `choice` | `options`: `[{"text","goto"}]`（2~4 项）；可选 `dialog`(默认"Options"，皮肤见 §3.3) | 选项菜单 `choose()` |
| `shock` | `character`；可选 `duration`(默认0.5) | 人物震动。官方角色走 flowcharts.common `shock`；`user:` 角色走独立立绘抖动并在结束后恢复位置 |
| `mask` | `show`(bool) | 独白遮罩 `os_mask.Show` |
| `intro` | 可选 `intro_source`(`official` 默认/`custom`)。official 必填 `character`；custom 必填 `name`,`text`，可选 `title`,`image`（包内 `assets/` PNG/JPG，≤8MB）、`image_scale`(40~160，默认100)、`image_x`/`image_y`(-30~30，默认0) | official 调用原版 `runwait(intropanel.Show(character))`；custom 调用 `mod_prepare_character_intro(title,name,text,image,scale,x,y)`，复用同一 CharacterIntroPanel。图片按屏幕安全区独立布局并保持比例；x 正数向右、y 正数向上，无图时隐藏头像区域 |
| `effect` | `name`；可选 `x`,`y`,`a`,`b`,`c`(数值，默认0/0/1/1/1)，`play`(bool，默认true) | 屏幕特效 `effects.SetupEffect(name,x,y,a,b,c,play)`，如 Hit_001/Blood_002/Sword_001。`play=false` 发射停止调用（末参 0）：**循环类特效不会自动销毁**（如 EventBubble/Glow），必须后接 play=false 的同参节点停止，否则常驻画面（旧数据的 `d` 字段仍兼容：无 play 时用 d） |
| `transition` | `phase`("in"/"out")；可选 `dir`(默认"lr"，lr/rl/tb/bt) | 黑场转场 `runwait(transitionblack.TransitionIn/Out(dir))`。**必须成对使用**：TransitionIn 隐藏剧情 UI 并盖满黑幕，TransitionOut 才恢复；有 in 无 out 时编译器警告（画面会一直黑屏） |
| `camera` | `name`, `active`(bool) | 镜头滤镜 `maincamera.ActiveVolume(name, 0 | 1)`，如 stage-memory/stage-dream/stage-fire/stage-blurdim |
| `block` | `flowchart`("view"/"common"), `name`；可选 `vars`: `[{"name","value"}]` | 通用 flowchart 块调用：`getvar` 逐个赋值后 `runblock(fc, name)`。覆盖 out_white/shake/flash/vshock 等 |
| `cg` | `action`("show"/"hide"), `kind`("picture"/"item"/"big"/"map"/"family"/"title")；可选 `key`, `key2`, `n1`, `n2` | mainui 图片/地图/家谱/标题：`ShowPicture(key)`/`HidePicture`/`ShowItemPicture`/`ShowBigPicture`/`ShowMap(key,key2)`/`ShowFamilyTree(key,key2,n1,n2)`/`DisplayTitle(key)` 等 |
| `dim` | `character`, `dimmed`(bool 必填，默认 true) | 人物压暗。官方角色走 `stage.SetDimmed(character, dimmedState)`；`user:` 角色复用当前舞台的 `DimColor` / `FadeDuration` |
| `message` | `text`（必填非空，多行合法） | 系统提示 `mainui.DisplayMessageText(text)` 显示**原文**（DisplayMessage 走本地化 key 解析，用 Text 版避免自定文本被当 key 查空） |
| `rotate` | `character`, `angle`(int 必填，默认 180), `duration`(float 必填，默认 1，>0) | 人物旋转到绝对 Z 角度。官方角色走 `characters.Rotate(key, angle, duration)`；`user:` 角色走 `mod_char_rotate` 后等待相同时长 |
| `dayenv` | `day_type`（int 必填，1=白天 / 2=晚上） | 日夜环境 `luamanager.SetGameDayEnvironment(day_type)`。**字段名 day_type**：避免与节点通用键 "type" 冲突 |

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
| `dice` | `check`, `options`: `[{"goto_大成功","goto_成功","goto_失败","band_texts"?}]`（恰好 1 条） | 骰子检定。**check 必须是带官方元数据的检查点**（editor_data 的 dice_meta：骰子范围 max 与结果带 bands；无元数据的检查点会让游戏内骰子菜单 NRE 崩溃）。发射官方五步链 + 按结果带数逐带发射选项（文本+条件）；分支按带质量名次映射：最差带→goto_失败，中间带→goto_成功，最优带→3带及以上 goto_大成功 / 2带 goto_成功。带质量按条件数值推断（同值 >系优于 <系）。**band_texts**（可选）：逐带覆写骰子菜单选项文本（条数=结果带数，每项非空，否则 LomcError）；发射 `<作者文本> \| <官方cond>`（作者文本为字面量，不进 texts.json；ASCII \| 净化为全角｜；cond 永远用官方元数据）。缺省用官方结果带文本 |
| `goto_scene` | `scene`("Free"/"Title"/"Combat"/"Battle"/"GameOver"/"End"/"Story"/"DemoEnd")；可选 `key`(Combat=战斗id/Battle=战役id/GameOver=死亡画面id/End=结局标识), `next`, `title`, `desc`(均为 str，仅 End/GameOver 用), `image`(str，**仅 End 用**：包内图片相对路径，如 `assets/ending.png`) | 普通场景仍为 `luamanager.ChangeScene(scene,key,next)`。**End 特例按原版汗青书流程**：缓存自定义标题/正文/插图 → `runwait(endgamepanel.Open("__MORTAL_MOD_END__"))` → 玩家确认 → 黑幕 → Title；运行时 patch 真正的 `EndGamePanel`，完整复用官方版式；`image` 写入左页 `_picImage`，留空时借原版结局 20047 的 Picture 占位；图片缺失/损坏只警告并回退占位。End/GameOver 的 next 无效（原版按钮固定为读档/标题），旧值忽略并警告（旧兼容值 Story 按 Title 处理、不警告）。只有不带自定义内容且给官方 key 时才直接打开官方结局条目（按原版解锁/记录并给警告）。mod 专属 End key 若无 title/desc/image、mod 专属 GameOver key 若无 title/desc，直接校验失败，避免空白卡 |
| `panel` | `panel`("martial"/"weapon"/"poison"/"cg"/"cgvideo"/"shop"/"newshop"/"credit"/"endgame")；可选 `key`(cg/cgvideo/endgame 的 id), `discount`(shop 用, 默认0), `mode`(martial 用, 默认0) | 打开系统面板，除 newshop 外均 `runwait`：`martialpanel.Open(mode)`/`weaponupgradepanel.Open()`/`poisonupgradepanel.Open()`/`cgpanel.Open(key)`/`cgvideopanel.Open(key,0)`/`shoppanel.Open(discount)`/`shoppanel.NewShop()`/`creditpanel.Open()`/`endgamepanel.Open(key)` |
| `wait` | `seconds` | `wait(seconds)` |
| `end` | 可选 `next_script` | 有：`SetNextScript("MOD_<modid>_<id>")`+`Init()` 链到同包脚本；无：`ChangeScene("Free","","")` 回自由模式 |
| `death` | `text`（必填非空，多行合法）、`death_id`（必填）；可选 `title`（str，缺省「勝敗乃兵家常事」）、旧字段 `next` | **死亡文本**：黑屏过渡（view="black"）→ `mod_set_death_text(title, text)`（两参 lua_str 字面量，**不进 texts.json / 已读系统**）→ `luamanager.ChangeScene("GameOver", death_id, "Title")` 进**官方 GameOver 死亡画面**（黑底红字 + 读档/标题按钮，见 §6）；原版不读取自定义 next，旧值忽略并警告。`death_id` 必须是 ≥900000 的 mod 专属数字 id（否则 LomcError，见「死亡/结局 id 约定」）。终止节点（自带流转，不允许显式 goto，可作末节点收尾） |
| `raw` | `code` | 原生 Lua 逃逸口：原样插入代码（多行合法）。**机制兜底**：任何节点表达不了的官方机制用它 |

### 3.2 常用取值（以 data/editor_data.json 为权威清单，schema 2 起带中文名）

- 站位 position：`SL L1 L2 M R1 R2 RM2 SR …`（共 36 个，S=屏外 L=左 M=中 R=右 B=后 C=央）
- 表情 portrait：`normal nervous1..3 angry1 angry2 laugh1 gloomy2 …`（按人物配置，缺失时游戏回退第一张立绘）
- say mode：`character` 对话 / `think` 内心独白 / `narrative` 旁白 / `center` 居中旁白
- stat key：`mental(心相) money(银两) disposition behaviour karma fame talking team …`（31 个）

### 3.3 选项菜单皮肤（choice.dialog）

**仅 `Options` 可用**（默认，纯文本选项；Dice 为骰子节点内部专用）。其余皮肤（Talk/Meet/Door/Section_* 等）是自由场景的 break 格式菜单（选项文本为 `类型+key+行动点+贡献` 四段 `+` 分隔），纯文本选项会触发 `BreakOptionButton.UpdateContent` 的 IndexOutOfRange 崩溃（菜单冻结）——编译器直接报错拒绝。发射：`setmenudialog(menudialogs.Options)` → `choose()` → `menudialogs.Options.SetActive(false)`。

## 4. story.json → Lua 编译约定（lomc 实现）

- 每个节点编译为一个 Lua 函数；文件头前向声明 `local node_n1, node_n2, ...`，然后 `node_nX = function() ... end`；流转尾调用 `return node_<goto>()`；顶层 `return node_<start>()`。
- 文本转义：`\`→`\\`，`"`→`\"`，换行→`\n`，`\r`→`\r`。
- 每个脚本开头 emit `modflags = modflags or {}`（全局表，Story 场景会话内持续，链式脚本共享；不存档），紧跟一行 `mod_set_mood(true|false)`（story 顶层 mood 声明，默认 false；见 §6）。
- `flag` 节点双 emit：`AddStory` + `modflags[flag]=true`。
- **分支兜底**：choice 外任何多路结构不允许静默落空——未命中 case 时 else 落顺序下一节点；无法兜底（branch 为末节点且未覆盖全部返回值）视为校验错误。
- 节点 id 字符集 `[a-zA-Z0-9_]+`（脚本 id 允许 `-`）。
- story 顶层 `title` 可选。
- **已读 key 规则**：所有 say（character/think/narrative/center）节点的文本一律发射 `say(luamanager.GetStoryText(key))`，key = `MOD_<modid>_<scriptid>_<nodeid>`；modid 来自 manifest（打包时），独立 build/编辑器预览缺省时用 "MOD" 兜底。**death 文本不走已读 key**：发射 `mod_set_death_text(<标题字面量>, <文本字面量>)`（标题缺省/空串用「勝敗乃兵家常事」），文本不进 texts.json。
- **结局/死亡卡片规则**：goto_scene scene=End 且带 title/desc/image 时先发射 `mod_set_ending_text(...)`，再按原版汗青书流程显示；image 是左页插图，不是全屏背景。scene=GameOver 带 title/desc 时改走 `mod_set_death_text(<title>, <desc>)`。death 节点同样发射 `mod_set_death_text(<title>, <text>)`（两参；单参旧包兼容仍由运行时支持）。两个全局调用由运行时插件注册（§6）。
- **mood 规则**：story.mood=false 时另在 show 节点末尾（Focus 后）与 say 节点 say(...) 前后各发射一次 `mod_hide_mood()`；true 时不发射。
- **death 发射**：见 §3.1 death 行（runblock out → ViewName="black" → runblock view → `mod_set_death_text(title, text)` → ChangeScene("GameOver", death_id, next)）。
- 最后一个节点不是 `end`/`death`/`goto_scene`/`raw` 且无 goto → 校验错误。
- `choice`/`branch`/`dice`/`end`/`death`/`goto_scene` 写显式 `goto` → 校验错误。
- `say` 的 narrative/center 模式给 character 允许但忽略。
- `raw` 节点内容原样插入（编译器不做语法检查）；其后流转照常（顺序/goto）。
- **非致命警告**：以 `-- lomc 警告：` 注释形式插在 Lua 头部（如 transition 有 in 无 out）；`lomc check` 同步打印到 stderr。

关键 API 范式：

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
-- say (narrative/center)：setsaydialog(saydialogs.narrative|saydialogs.center); sayoptions 两行同上; setcharacter(narrative); say(GetStoryText(...))（任何 say 前都设 sayoptions）
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
-- dim / message / rotate / dayenv
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
-- dice（check 必须带官方元数据；band_texts 可选逐带覆写）
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
-- death（黑屏过渡 → mod_set_death_text(title, text) → 官方 GameOver 死亡画面）
runblock(flowcharts.view, "out")
getvar(flowcharts.view, "ViewName").value = "black"
runblock(flowcharts.view, "view")
mod_set_death_text("勝敗乃兵家常事", "你坠入山崖，万事休矣。")
luamanager.ChangeScene("GameOver", "910021", "Title")
```

## 5. data/editor_data.json — 编辑器数据契约（schema 3）

由 `tools/extract_editor_data.py` 生成。schema 2 起 `characters`/`stats`/`positions`/`views`/`music`/`free_positions` 均为 `{id, name}` 对象数组（characters 另有 portraits）；schema 3 新增 `dice_meta`（骰子检查点元数据：`{check: {max, bands: [{text, cond}]}}`，bands 按官方展示顺序）与 `death_ids`/`ending_ids` 富化对象数组（name 取自 `data/ref/death_ending_ids.json`，见下方「死亡/结局 id 约定」）。**dice_meta 仅含故事场景检查点**：旅行系统检查点（Travel_*）在故事场景的 CheckPointManager 查不到会崩，提取时已剔除；`dice_checks` 是全名清单，保留全部调用点（含旅行）：

```json
{
  "schema": 3,
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

官方 GameOver/EndGamePanel 会用 id 查 LibrarySystem 并可能执行 LibraryItemData.Add()（解锁/记录官方结局）。自定义 End 固定用不存在的内部 key，不查询/写入官方结局；只有"无自定义内容、直接打开官方 End key"会按原版记录。

- **mod 死亡/结局 id = `9<官方id>`**（900000 区段）：官方死亡 10021 → mod 910021；官方结局 20003 → mod 920003。与官方 1xxxx（死亡）/2xxxx（结局）/4xxxx（后日谈）全区间不撞。
- 自造 GameOver id 查不到官方条目 → 无副作用，文本由插件注入；自造 End id 只作 mod 内标识，真正显示走固定内部 key。
- `death` 节点的 `death_id` 校验 ≥900000 整数。空白的自造 GameOver/End 卡会被编译器拒绝；无自定义内容直接使用官方 key 时给出非致命存档污染警告。
- 权威参考：`data/ref/death_ending_ids.json`：`death` 106 个（10000~10104、11000）、`ending` 54 个（20000~20053）、`epilogue` 4 个（40000~40003）。提取器用其标题富化 editor_data 的 death_ids/ending_ids；编辑器 death_id 输入框列出前 5 个官方参考。

## 6. 运行时插件行为（MortalModHost）

1. 启动扫描 `BepInEx/plugins/MortalModHost/mods/*.lommod`，注册 `MOD_<modid>_<scriptid>` → lua 文本。
2. Harmony prefix `LuaManager.ExecuteLuaScript()`：注册名命中时用 mod lua 执行并跳过原方法。
3. 入口：Free 自由场景与 Title 标题画面左下角"活侠MOD"按钮 + F8（可配）打开菜单。Free 菜单分"演出 mod 剧情"与"开始新战役"两区；Title 菜单仅"开始新战役"区（演出剧情需要已加载的存档玩家状态，只在 Free 提供）。
4. **战役**：点击"开始新战役"→ `SetSlot("mod_<modid>")`（隔离存档槽）→ 官方 `NewGameData()` → postfix 把首个剧情脚本替换为该 mod 的 entry → LoadStory。
5. **原版剧情抑制与位置触发器**：`disable_official_events` 或 F7 生效时，`UpdateCheckMissions` 内暂时隐藏主线触发状态，`HasAnyMissionTrigger` 返回 false，避免返回 Free 时自动启动官方主线/支线；地点点击 postfix `FreePositionData.GetExecuteScript` 优先匹配 manifest.triggers，无 mod 命中时抑制官方地点默认脚本。
6. **兜底**：Story 场景请求的 MOD_ 脚本未注册（mod 被删）时，不执行并 `ChangeScene("Free","","")` 防软锁。
7. mod 不修改官方脚本与文本表；mod 的 flag 进 StoryKeyList，存档兼容。
8. **texts.json 注册**：加载 .lommod 时把 texts.json 的 key→文本注册进 LeanLocalization（`Story/`+key）；`GetStoryText` 按 key 查已读系统：已读→黄色+可快进，未读→正常色+记入已读，查不到返回 key 本身。
9. **mod_hide_mood**：注册全局 Lua 函数 `mod_hide_mood()`（无参），隐藏全场角色圆形情绪面板（CharacterMoodPanel）；编译器按 story.mood 开关在 show/say 处发射（见 §4）。
10. **mod_set_mood**：注册全局 Lua 函数 `mod_set_mood(bool)`，按脚本头部声明硬控官方心情面板开关（ShowMood），每个 mod 脚本入口发射一次，链式脚本逐脚本切换生效。
11. **UpdateTranslations 防 wipe**：官方文本刷新会清掉插件注册的 mod 文本，必须 hook 并在刷新后重放 texts.json 注册（加载时缓存全部注册项），保证 mod key 永不失效。
12. **人物介绍卡**：官方人物保持原始 `CharacterIntroPanel.Show(key)` 行为；自定义人物在特殊 key 上由 Harmony 接管，复用官方面板版式，写入自定义称号/姓名/正文。可选 `image` 从当前 `.lommod` 的 `assets/` 解码放入独立安全布局：默认中心屏幕 `(31%,50%)`，最大宽/高屏幕 `(30%,62%)`，保持比例；`image_scale` 在自动适配尺寸上缩放，`image_x/image_y` 按屏幕百分比微调。关闭时销毁临时纹理并完整恢复原版控件；无图时隐藏头像区域，不修改官方本地化表或关系数据。
13. **结局/死亡卡片绘制**：注册两个全局 Lua 函数（见 §3.1/§4）：
    - `mod_set_death_text(title, desc)`：缓存死亡标题/描述；Harmony postfix `GameOverController` 把两段文本写入官方 `_titleText`/`_descTextPrefab` 控制器，官方布局显示在死亡画面中央。单参调用按旧契约当 desc、标题留空（旧包兼容）。
    - `mod_set_ending_text(title, desc[, image])`：缓存结局标题/描述与可选包内图片；Harmony postfix 包装 `EndGamePanel.Open`，在官方第一次画布 fade 前写入 `_titleText/_descText` 与左页 `_picImage`；未给图片时借用官方结局 20047 的 Picture 占位。官方渐显、等待确认与淡出全保留；显示期间临时关闭 `_saveLibrary` 避免 mod key 进入传奇存档槽，结束后恢复。
    - 新编译器的自定义 End 不再进入简化 `EndGameController` 场景；旧包仍保留原 End 场景覆盖兼容。GameOver 自造 id 无文字与 End 自造 id 无内容均在编译期阻止。
14. **编辑器单次试玩协议**：编辑器把入口章节的 `start` 临时改为当前选中节点，安装为固定包 `__lom_modkit_preview.lommod`（manifest id `lom_modkit_preview`），随后原子写入插件目录 `preview-request.json`。运行时每 0.35 秒检查一次：Free 场景直接演出，Title 场景用 `mod_lom_modkit_preview` 隔离槽开局，其它场景等待到安全场景；消费后删除请求与临时包。请求只接受 format=1 及 `[A-Za-z0-9_-]+` 的 mod/script/node id，正式 Mod 包不在自动删除范围内。
15. **mod 新战役发放 2 点命运**：官方新游戏初始带命运点，mod 隔离存档初始为 0，骰子「逆天」流程（`DiceMenuDialog.CheckRevolution` 要求 命运>0）在 mod 战役中不可用；NewGameData postfix 在替换首脚本后给 mod 战役 `GameStatType.命運` 加 2 点。官方新游戏不受影响。
16. **mod 剧情放开骰子范围修改**：官方「修改范围」按钮要求二周目且持有成就 30016；mod 剧情中（`CurrentStoryScript` 以 `MOD_` 开头）`get_NewGamePlus` prefix 返 true，且 `CheckRevolution` 原返 true 时直接激活 `_rangeButton`（不在 mod 里解锁官方成就 30016，避免污染官方存档）。官方剧情完全不受影响。
17. **用户音频**：`LuaManager.PlayMusic/PlaySound/PlayEnvSound` 参数以 `user:` 开头时由插件接管，从**当前演出 Mod 包**的 `UserContents` 解析（`assets/user/audio/<id>/content.json` + 主文件），解码后用 Windows `waveOut` 播放（本游戏主混音是 Wwise，Unity `AudioSource` 经常无声）。官方名字一律放行给原版 Wwise。运行时禁止读取 `%APPDATA%/lom_modkit/repository`。两个 Mod 即使 ID 相同也只解析自身包。支持格式仅 `.ogg` / `.wav`，单条 ≤20MB。自定义 fadeout 是输出音量淡出（随后仍有编译器发射的 `wait`）；切到自定义音乐会先停官方 Wwise 音乐（官方 `StopMusic` 会同时清环境音）。
18. **对白语音**：注册 `mod_play_voice(ref)` / `mod_stop_voice()`。`mod_play_voice` 先停当前语音再播（不循环，走独立 `_voice` 通道）。`sound` 节点、自定义音效、`StopMusic` 都不碰这条通道。剧情中断、切官方脚本、重载 Mod 时 `StopEverything()` 会停语音。无 `voice` 的旧 Lua 不会调用这两个函数，行为不变。
19. **离场清台**：脚本开头、`end` / `goto_scene` / `death` 发射 `mod_hide_all()`，立刻隐藏官方台上人物并清掉自定义立绘。换背景 `scene` 不自动退场。切到下一章（`end.next_script`）时下一章开场也会再清一次，避免上一幕角色带到下一章。
20. **自定义角色立绘朝向与体型**：原版立绘朝左。自定义角色默认 `art_facing=left`，节点 `facing=left` 不翻、`right` 才水平翻转；原图朝右时把 `art_facing` 标成 `right`。`scale` 是 50–130 的体型百分比（默认 100），从脚底缩放，大约 80 接近小师妹。
21. **游戏内 Mod 菜单多语言**：菜单文案（`src/I18n.cs` 内嵌 zh_CN/zh_TW/ja/ko 四语言目录）跟随游戏当前语言——反射读 LeanLocalization `CurrentLanguage` 并模糊匹配语言名；官方游戏本身没有日语选项，日语目录实际不会触发；检测失败一律回退 zh_CN。详见 `i18n.md`。

## 7. AI 工具接口（story_api）

editor/story_api.py 是 AI/编辑器共用的受控写入口。规则：**AI 不直接手写 story JSON 或 Lua**，
一切剧情构建经 story_api（models 契约默认值 + lomc 校验/警告），防止骰子菜单崩溃、
transition 黑幕、choice 皮肤崩溃、背景黑屏、人物未登场就做动作等已知坑。

- Python API：
  - `load_editor_data()`：读取编辑器数据（含 dice_meta 等清单），返回 (editor_data, is_fallback)
  - `new_story(story_id="main", title="新剧情", mood=False)`：新建剧情脚本（show 登场 + 空 say 双节点开场，先登场再动作）
  - `add_node(story, node_type, fields=None, after=None)`：按 models 默认值新增节点（44 种类型），未知类型/字段/类型不符→ValueError，节点 id 自动生成，after 指定插入位置（节点 id 或 None=末尾）。登场防线：动作类节点的目标人物在前面未登场/已退场时，自动在它前面插入 show
  - `update_node(story, node_id, fields)`：更新节点字段（同 add 的字段校验），节点不存在→ValueError。登场防线：更新后若动作人物未登场/已退场，自动在该节点前插入 show 并把指向它的 goto/选项/分支跳转改指新节点
  - `get_node(story, node_id)`：读取节点，不存在→ValueError
  - `list_nodes(story)`：返回 [{"id","type","summary"}] 清单
  - `delete_node(story, node_id)`：删除节点，不存在→ValueError
  - `rename_node(story, node_id, new_id)`：重命名节点 id 并同步 start 与全部跳转引用（goto/选项/分支/骰子去向），返回改名后的节点；新 id 限 `[A-Za-z0-9_-]+`，与现有节点冲突→ValueError
  - `move_node(story, node_id, delta)`：按相对位移调整节点顺序
  - `set_start(story, node_id)`：设置起始节点
  - `add_choice(story, options, after=None)`：新增选项分支（2~4 项，dialog 固定 Options）
  - `add_dice(story, check, goto_成功, goto_失败, goto_大成功="", band_texts=None, after=None)`：新增骰子检定（check 必须有官方元数据，按结果带数校验 goto；band_texts 条数必须等于结果带数且每项非空）
  - `add_say(story, text, character=None, mode="character", portrait="normal", voice=None, after=None)`：新增对白（character 模式必填 character；narrative/center 不写 character；voice 可选 user: 音频引用）
  - `add_death(story, text, death_id, next="Title", title=None, after=None)`：新增死亡文本节点（text 必填非空多行；death_id 必填 ≥900000 的 mod 专属数字 id；next 仅接受 Title；title 可选短标题，缺省/空串用「勝敗乃兵家常事」）
  - `add_scene(story, view, after=None)`：新增场景切换
  - `check_story(story)`：只校验，返回 (errors: list[str], warnings: list[str])
  - `compile_story(story)`：校验+编译，返回 (lua|None, errors, warnings)，失败时 lua 为 None
  - `load_story_json(path)` / `save_story_json(story, path)`：story.json 读写（UTF-8）
  - `pack_mod(mod_dir, output=None)`：校验 manifest + 全部编译 + 打 .lommod，返回产物路径
- CLI：python editor/story_api.py check|compile|pack|new-story（AI 子进程友好，退出码 0/1，中文错误）
- 关键不变量（编译器强制，API 透传）：choice.dialog 仅 Options；dice.check 必须有官方元数据
  （骰子范围+结果带）；transition in/out 成对；scene 自动预载背景；
  **show/say 的 (character, portrait) 必须落在 data/editor_data.json 的角色表情表内**
  （表不可用/角色不在表 → 放行；角色在表但表情不在其列表 → LomcError/ValueError——
  游戏 LoadCharacterPortrait 对无效表情 key 抛 KeyNotFoundException → Lua 协程死 → 对话冻结）。
  say/show 引用的人物必须先 show 上台（未上台同样抛 KeyNotFoundException），
  写入口的登场防线会自动补 show（见 add_node/update_node），编辑器体检对多路径汇合做图级兜底。

## 8. 用户内容（User Content）

开发环境仓库在 `%APPDATA%/lom_modkit/repository/`，**不是**运行时依赖。剧情只保存稳定引用：

```text
user:<namespace>.<content_id>     例如 user:mohui.boss_theme
```

官方 ID（`普通_001`、`brother4`）保持原样，不改成 `official:`。

包内结构（仅打包实际引用）：

```text
assets/user/audio/mohui.boss_theme/content.json
assets/user/audio/mohui.boss_theme/boss_theme.ogg
assets/user/image/mohui.moon_bg/content.json
assets/user/image/mohui.moon_bg/moon.jpg
```

`content.json` schema 1：

```json
{
  "schema": 1,
  "id": "mohui.boss_theme",
  "type": "audio",
  "name": "决战曲",
  "audio_kind": "music",
  "files": { "main": "boss_theme.ogg" }
}
```

对白语音仍是 `type=audio`，可另加可选管理字段 `character`（`user:mohui.luoxue` 或官方人物 id，如 `player`）。没有该字段的旧音频继续合法。`character` 不改变 `say.voice` 播放协议，也不导致未引用音频被打包。

自定义角色 `content.json` 还可选：`title`（对话短称号）、`scale`（体型 50–130，默认 100，脚底对齐）、`art_facing`（原图朝向 `left` 默认 / `right`）。缺省与旧包按 100 / 朝左处理。

- `type`：`audio` / `character` / `image`。`image` 是背景、CG、Overlay 共用的统一图片内容，不建立三个独立仓库。
- `background.image` 只保存 `user:` 稳定引用；编辑器提供缩略图、缺失诊断与 F5 舞台重建，发布包只收集实际引用的图片。
- `audio_kind`：`music` / `sound` / `env`。
- `character`（仅音频、可选）：用户角色引用或官方人物 id；省略表示旁白/系统/未关联。
- 内容 ID：`[a-z][a-z0-9_]{0,31}.[a-z0-9][a-z0-9_]{0,47}`，禁止 `..`、`/`、`\`、`:`。
- 缺失、类型不匹配、metadata 损坏、文件不存在、扩展名不支持、音频超过 20MB 或图片超过 8MB：pack 直接失败，不得 silently skip。
- Python 侧唯一解析入口：`compiler/lomc/content.py`。C# 侧契约实现：`ContentRef.cs` + `ModLoader`。

使用说明见 `user_content.md`。
