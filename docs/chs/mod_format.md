# 活侠传 Mod 包格式（v3 契约）

> 语言：简体中文（本文） · [繁體中文](../cht/mod_format.md) · [日本語](../ja/mod_format.md) · [한국어](../ko/mod_format.md)

**所有组件（编辑器 / 编译器 / 运行时插件）以本文档为准。** 改动需同步更新本文档。
文中规则的官方脚本/反编译实证材料见 `../research/`，正文不重复展开。

## 1. 包结构

`.lommod` 文件 = zip 压缩包，内部结构：

```
manifest.json          # 必填，包元信息
story/<id>.json        # 必填≥1，剧情源文件（编辑器可编辑的源格式）
lua/<id>.lua           # 必填≥1，编译产物（运行时只读这里）；每个 story/<id>.json 对应一个
texts.json             # 必填，已读文本表：{MOD_<modid>_<scriptid>_<nodeid>: 文本}（say 节点文本）
localization.json      # 可选，Story 内容语言元数据（schema/default/fallback/locales）
lua/<locale>/<id>.lua  # 本地化包必填，完整的 locale 专用编译产物
texts/<locale>.json    # 本地化包必填，locale 专用已读文本表
story-lua.sha256       # package_format=3 必填，Story/Lua 成对 SHA-256
package-content.sha256 # 必填，压缩无关的逻辑包内容 SHA-256
assets/                # 可选，自定义资源
                       #   图片：结局插图 / 人物介绍图 PNG/JPG
                       #   用户音频：assets/user/audio/<content_id>/
                       #   自定义角色：assets/user/character/<content_id>/
```

- `<id>` 规则：`[a-zA-Z0-9_\-]{1,64}`，包内唯一，即"剧情脚本 id"。
- 导出（打包）时必须重新编译：story/*.json → lua/*.lua，二者同名。
- `story-lua.sha256` 使用 `lom-story-lua-sha256-v1`：每行以 TAB 记录 Story 路径/原始字节 SHA-256 与对应默认或 locale Lua 路径/原始字节 SHA-256。Runtime 对 v3 包强制逐项核对；v1/v2 包缺少稳定战役身份，1.0.1 不再导入或加载，必须明确设置 `campaign_id` 后重新导出。
- 运行时插件**只读 manifest.json、lua/、texts、可选 localization.json 与 assets/**；story/*.json 给编辑器回读/再编辑用。编译器只打入剧情明确引用的 PNG/JPG（单张 ≤8MB）、明确引用的 `user:` 音频，以及明确引用的自定义角色立绘。导出的 `.lommod` 自包含，玩家机器不需要编辑器仓库。
- texts.json 由打包时自动生成：收集每个 story 的全部 **say** 节点文本，key 与 lua 里 `GetStoryText` 的 key 一一对应；运行时注册进 LeanLocalization（见 §4/§6）。**death 文本不进 texts.json**：由 codegen 发射 `mod_set_death_text(<标题>, <文本>)` 两参 lua_str 字面量（见 §3.1/§6）。
- 运行时先拒绝物理文件超过 160 MiB 的包，再从读取该包的同一个文件句柄计算最终 `.lommod` **全部原始字节**的 SHA-256，保存完整 64 个十六进制字符，并在强制披露中显示前 16 个字符。重新压缩、修改任一字节都会改变指纹；改文件名或逐字节复制不会改变。该指纹用于核对具体包，不是作者签名或官方认证。编辑器安装器同样以 160 MiB / 4 MiB 分别限制包文件与 `manifest.json`。
- 打包器按包内规范路径排序条目，JSON 使用稳定键顺序，Lua 编译顺序固定，并把 ZIP 时间戳固定为 1980-01-01、权限固定为普通只读元数据；同一 lom_modkit/Python/zlib 工具链下，相同项目连续导出的 `.lommod` 应逐字节一致。不同 Python 或 zlib 实现的压缩字节可能不同，因此**不宣称跨工具链 reproducible build**。
- `package-content.sha256` 是 `lom-entry-sha256-v1` 记录：对除它自身外的全部条目按名称排序，以「名称长度 + UTF-8 名称 + 内容长度 + 原始内容」计算 SHA-256。它不依赖 ZIP 时间戳、权限或压缩结果，可由 `lomc.package_content_hash(path)` 复算；它是构建一致性校验，不是签名或官方认证，也不替代 Runtime 对整包原始字节计算的 Host 指纹。
- 编辑器「文件 → 检查 Mod 包」以只读方式展示 Manifest、Story、Lua、Texts、资源、用户内容、大小和逐条目 SHA-256，同时检查版本兼容、格式错误、逻辑内容哈希及资源引用/打包差异。检查器不解包到磁盘、不执行 Lua，也不会像「导入 Mod」那样登记用户内容。

### 1.1 Story 内容本地化（可选）

Story 本地化与编辑器界面语言是两套独立机制。支持 `chs`、`cht`、`ja`、`ko`。未启用本地化的旧 Story 及其 `lua/<id>.lua`、`texts.json` 契约保持不变，不要求迁移。

启用后，节点字段中的作者文本仍是默认语言原文；每个 Story 可增加：

```json
"localization": {
  "default_locale": "chs",
  "fallback_locale": "cht",
  "translations": {
    "cht": {"story.title": "章節標題", "say1.text": "你好呀"},
    "ja": {"story.title": "章タイトル", "say1.text": "こんにちは"}
  }
}
```

翻译键是编译器生成的稳定路径：`story.title`、`<node>.text/title/name/desc`、`<choice>.options.<index>.text`、`<dice>.options.<index>.band_texts.<index>`（人物介绍正文使用 `<intro>.text`）。ID、跳转、资源引用与 raw Lua 不可翻译。未知 locale、未知路径、空译文会在校验时拒绝。

打包器为四种受支持语言生成完整 Lua 与 texts 变体。解析顺序是「当前游戏语言 → fallback_locale → 默认语言原文」。运行时在 LeanLocalization 刷新时重新选择缓存的脚本与文本，不重新扫描或解压 Mod。`localization.json`/语言资源损坏时整组本地化资源被忽略并告警，旧的默认 Lua 仍可运行。

编辑器的「剧情本地化」窗口按目标语言显示互斥覆盖率：总文本数 = 已翻译 + 使用回退 + 缺失；可只显示缺失项。这里的“缺失”表示目标语言和指定回退语言都没有译文，运行时最终会显示默认语言原文。统计完全来自当前 Story 数据，不调用或暗中接入机器翻译。

## 2. manifest.json

```json
{
  "format": 3,
  "package_format": 3,
  "story_schema": 2,
  "content_schema": 1,
  "min_host_version": "1.0.0",
  "tested_host_version": "1.1.0",
  "tested_game_version": "1.2.3",
  "id": "showcase3",
  "campaign_id": "demo_campaign",
  "name": "示例 Mod",
  "version": "1.1.0",
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

- `package_format`：`.lommod` 容器与 manifest 契约版本，当前固定为 `3`。v3 强制 `story-lua.sha256`、`package-content.sha256`、稳定 `campaign_id` 和 `campaign.new_game=true`；v1/v2 一律拒绝。
- `story_schema`：包内 `story/*.json` 源格式版本，当前固定 `2`。
- `content_schema`：包内 `assets/user/*/*/content.json` 格式版本，当前固定 `1`。
- `format`：兼容拼写，值必须与 `package_format` 同为 `3`。未知、旧版或互相冲突的声明都会被拒绝。
- v1/v2 manifest 和 Story 的 Combat/Battle 语义无法安全推断，也没有稳定 `campaign_id`，Editor 不会自动迁移或从 `id` 猜测。旧项目须由作者选择长期不变的战役 ID、重新配置相关节点并用 1.0.1 或更高版本重新导出。
- `id`：mod 包唯一 id（`[a-z0-9_\-]{1,64}`），作为 Lua 注册名前缀；它不是存档身份。
- `campaign_id`：战役唯一且跨包版本稳定的 id（同样匹配 `[a-z0-9_\-]{1,64}`），唯一用于生成 `mod_campaign_<campaign_id>` 存档命名空间。同一战役更新时不得改变，也不会回退为 `id`。
- `entry`：入口剧情脚本 id（`[A-Za-z0-9_\-]{1,64}`），必须存在；`lua/` 下所有脚本 id 同样由运行时复验。
- `min_host_version`（可选 SemVer）：硬门槛。当前 MortalModHost 低于它时，在注册脚本前明确拒载。
- `tested_host_version`（可选 SemVer）：作者最后测试的 Host 版本；当前 Host 更高时警告但继续加载。编辑器新导出默认填入随附 Runtime 版本。
- `game_version`（可选版本标识）：硬门槛，必须与 Unity 运行时真实 `Application.version` 精确一致，否则拒载。
- `tested_game_version`（可选版本标识）：作者测试的游戏版本；与 `Application.version` 不同时警告但继续加载。它与 Steam build id 不是同一字段，当前值可从 Host 启动日志查看。
- 四个兼容性字段仍可选；`min_host_version` 不得高于 `tested_host_version`，同时填写 `game_version` 与 `tested_game_version` 时二者不得矛盾。
- `name`、`version`、`author`、`description` 都是**作者自报元数据**，不能声明官方身份。运行时展示前会单行化、限长，并移除控制字符、双向覆盖/零宽格式字符与富文本尖括号。
- 未定义 `official`、`verified`、`signature`、`sha256` 等信任字段；手工向 manifest 添加这些字段不会影响 Host 计算的包指纹，也不会显示为官方内容。
- `campaign`（可选）：战役模式。
  - `new_game`：v3 固定为 true。选择 MOD 只切换到该战役的存档页，不直接开局；只有明确点击“开始新战役”才使用当前选中的 `mod_campaign_<campaign_id>` / `_sNNN` 隔离槽开新游戏，并把首个脚本替换为本包 `entry`。
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
  "story_schema": 2,
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

### 3.1 节点类型（全量 62 种）

此表是当前全部合法节点。`combat` / `battle` 使用稳定的运行时内部基线，不向作者暴露场景预设；战斗能力只调用已反编译核验的原版接口，`mod_quest` 则是明确不接触原版 Mission ID 的宿主状态机。

**演出类**

| type | 字段 | 说明 |
| --- | --- | --- |
| `music` | `name`；可选 `op`("play"默认/"stop"/"fadeout")，fadeout 时 `seconds`(默认2) | 官方名继续 `PlayMusic` / `StopMusic` / `FadeOutMusic(seconds)` 后 **`wait(seconds)`**。`name` 以 `user:` 开头时是用户内容引用（§8），由运行时从**当前包** `assets/user/` 解析播放。禁止本机绝对路径 |
| `sound` | `name`；可选 `kind`("sound"默认/"env")，`op`("play"默认/"fadeout"仅env，`seconds`默认1) | 官方名继续 `PlaySound` / `PlayEnvSound` / `FadeOutEnvSound(seconds)` 后同样 **`wait(seconds)`**。`user:` 引用规则同 music，且 `audio_kind` 必须与节点匹配 |
| `scene` | `view` | 切换**原版官方背景**：先清除自定义背景，再 `runblock(flowcharts.view,"out")`；其余原版预载与切换行为不变。`view="out"` 只淡出；`"black"/"white"` 为纯色 |
| `background` | `action`(`set`/`show`/`replace`/`fadein`/`fadeout`/`clear`)；显示类必填 `image`(`user:` 图片)，可选 `fade`(默认0.5) | 使用当前包 `assets/user/image/` 的图片覆盖为舞台背景。`set/clear` 立即执行；其余动作按 `fade` 淡入/淡出并等待。换章、`goto_scene`、官方 `scene`、场景切换与热重载都会清理，不修改原版 View 资源 |
| `custom_cg` | `action`(`show`/`hide`)；show 必填 `image`(`user:` 图片)；可选 `fade`(默认0.5)、`scale`(10~300，默认100)、`x/y`(-100~100，默认0) | 在人物层前显示全屏 CG，保持图片比例；`scale` 为自动适配后的百分比，`x` 正向右、`y` 正向上。show 替换上一张，hide 淡出并销毁；换章、换场景、换包自动清理。官方 `cg` 节点保持原样 |
| `overlay` | 必填 `action`(`show`/`hide`) 与 `slot`；show 必填 `image`；可选 `position`(九宫格)、`scale`(10~300)、`opacity`(0~100)、`layer`(`back`/`front`)、`fade` | 多槽位前景/道具/插图/遮罩；同槽 show 替换、hide 独立清理。前后层相对人物分离，F5 可恢复，换章/换场景/换包统一清理 |
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
| `enemy` | `op`("team"=向心力/"level"=门派规模/"people"=门派人数/"id"=选择当前敌对门派), `enemy`(战役门派 id), `value`(变化量，id 操作不需要), `display`(仅 team/people 使用，默认1) | 修改 **Battle 多人战役**使用的门派状态 `ModifyEnemyTeam/Level/People/Id`；不配置 Combat 一对一决斗敌人 |
| `battle_skill` | `op`("set"/"active"/"reset"), `key`(reset 不需要), `index`(set 用, 默认2), `active`(active 用, 默认1) | 战场技能 `SetPlayerBattleSkill/SetBattleSkillActive/ResetBattleSkill` |
| `combat` | 必填 `character`, `background`, `win`, `lose`；可选对手 HP/气力/六维/评语/`talents`/行动概率；可选 `player_*` 与 `player_talents` 覆盖本场赵活 | 对手与赵活都只写本场 `CombatStat`。对手只填 `max_health` / `max_stamina` 时会以该最大值满血/满气开场；填写 `health` / `stamina` 才覆盖初始值。赵活未填的字段仍走官方 `SetPlayerStat` 从 `GameStat` 抄值。`player_*` 不写 `SaveSystem`/`PlayerStatManagerData`，战后赵活存档属性自然恢复。`talents`/`player_talents` 为官方 CombatSkill |
| `battle` | 必填 `win`, `lose`；可选 `title`、`friend_health` / `enemy_health`、`friend_factions` / `enemy_factions`（`{id,people}`）、`friend_characters` / `enemy_characters` | 各方总人数 = 各阵营 people + 具名角色，至少 1。阵营必须是已有 `BattleLevel.NameKey` 的 id。具名角色仅限可生成的官方人物。标题写 `ReadyPanel._enemyNameText`；血量克隆 `HealthData`，不改官方资产。旧 `friend_people` / 单数 `friend_faction` 拒绝 |
| `battle_result` | `win`, `lose`；可选 `kind`("any"/"combat"/"battle"，默认 any) | 读取 Host 按完整包指纹和剧情 id 绑定的最后真实结果并分支。当前仅支持反编译确认的 win/lose；无结果、类型不符或伪造 draw/escape 都会 fail-closed。终止节点，不允许额外 goto |
| `reward` | `entries`(1~32)：`kind`=stat/affinity/talent/item/flag，`key`，非 flag 用 `amount`，item 另用 `category`=book/misc/special | 编译期展开为现有 `Player` / `Character` / `AddTalent` / `AddBook|Misc|Special` / `AddStory` 与 `modflags`，不发明奖励存档系统 |
| `result_screen` | `title`（非空）、`entries`（同 reward）；可选 `text` | 先用原版 `mainui.DisplayMessageText` 显示作者填写的标题与说明，再按顺序执行 `reward` 的现有原子接口；这是作者友好的组合节点，不创建自定义结算 UI |
| `custom_shop` | `items`(1~64)：`category`=book/misc/special、`item`(原版物品 id)、`count`(1~9999，默认1)，可选 `condition:{source,key,invert?}`；节点可选 `discount`(0/1，默认0) | Host 临时替换 `ShopDatabase` 三类库存并打开原版 `ShopPanel`，关闭或故障时恢复原库存。source=mod 读取本章 `modflags`，source=condition 调原版 `checkpointmanager.Condition`。discount=1 使用原版统一 50% 折扣；原版没有公开逐商品价格接口，`price` 字段会被拒绝 |
| `stat_check` | `key`, `op`(>=/>/<=/</==), `value`, `success`, `failure` | 读取 `LuaManager.GetStatData(key,1)` 后二分 |
| `affinity_check` | `character`, `op`, `value`, `success`, `failure` | Host 只读解析原版 RelationshipStatType 并读取 `Relationships.Get(type).Value` |
| `item_check` | `category`=book/misc/special, `item`, `success`, `failure`；可选 `invert` | Host 只读调用原版 `ItemDatabase.HasItem`；未知 id 运行时 fail-closed |
| `talent_check` | `talent`, `op`, `value`, `success`, `failure` | Host 只读读取原版 `PlayerStatManagerData.Talents.Get(id).Level` |
| `flag_check` | `source`=mod/condition/flag_value, `flag`, `success`, `failure`；布尔来源可选 `invert`，flag_value 必填 `op/value` | 分别读取 `modflags`、`checkpointmanager.Condition` 或 `tonumber(luamanager.GetFlagData)`；二分节点不允许额外 goto |
| `activity` | `kind`=training/study/forge/alchemy/custom，`stat`,`op`,`value`,`success`,`failure`；可选 `message`,`time`=none/round/month、成功/失败奖励表 | 编译期组合现有系统提示、`GetStatData` 检定、`NextRound/NextMonth` 与 `reward` 原子接口，不引入新活动引擎 |
| `mod_quest` | `quest`(安全 id), `op`=start/update/complete/fail；可选 `message` | 操作 Host 自有任务状态机；按包 id + 完整 SHA-256 隔离，不调用 `MissionManager`，不占用官方 Mission ID。当前是战役会话态，跨 Story/Free、但不跨重启/新战役 |
| `quest_check` | `quest`,`state`=inactive/active/completed/failed,`success`,`failure` | 读取同包任务状态并二分；未知任务为 inactive，非法状态迁移在运行时拒绝 |
| `persistent_var` | `key`(安全 id), `op`=set/add, `value`(Int32) | 设置或增减当前 MOD 隔离手动槽的 Host sidecar 整数变量。只允许 `SaveSystem.CurrentSlot` 属于当前 `campaign_id` 的 `mod_campaign_<campaign_id>` / `_sNNN` 命名空间；修改先留在内存，原版手动/自动存档成功返回后原子写入，不修改 GameSave schema |
| `persistent_check` | `key`,`op`(>=/>/<=/</==),`value`(Int32),`success`,`failure` | 读取同一隔离手动槽 sidecar 后二分；缺失变量为 0。普通存档槽、F5 试玩槽、其他 MOD 槽一律拒绝访问 |
| `mission` | `name`, `key` | 任务操作 `statmodifymanager.Mission(name, key)`：`Mission("Main","M0001")` 推进主线 / `Mission("S2200","clear")` 清支线 |
| `time` | `op`("set"/"round"/"month"/"mission")；set 用 `year,month,stage`；mission 用 `name,year,month,stage` | 时间 `SetGameTime/NextRound/NextMonth/SetMissionTime` |
| `autosave` | 可选 `kind`("story"默认/"free"/"prologue")；可选 `save_button`(0/1，单独控制存档按钮) | `AutoSave()/AutoFreeSave()/PrologueSave(mode)`；`save_button` 单独 emit `ToggleSaveButton(n)` |

**流程类**

> 原版术语：`Combat` 是一对一决斗，`Battle` 是带门派人数、阵型与战场技能的多人战役；两套关卡编号不能混用。

| type | 字段 | 说明 |
| --- | --- | --- |
| `branch` | `cases`(≥1)；可选 `source`("mod"默认/"game"/"stat"/"flag_value"/"condition")。键字段：source=stat 时用 `stat`（属性 id，editor_data stats 清单），其余来源用 `flag`（非空） | 条件分支，五来源：mod=按 modflags 是否已设；game=官方检查点 `checkpointmanager.Switch(flag)`；stat=主角属性 `luamanager.GetStatData(stat, 1)`；flag_value=官方任务旗标 `tonumber(luamanager.GetFlagData(flag))`；condition=官方条件检查点 `checkpointmanager.Condition(flag)`（bool）。case 结构按来源：mod/condition 用 `[{"value","goto"}]`（value 仅 1/2：mod=已设/未设，condition=真/假）；game 用 `[{"value","goto"}]`（任意整数）；stat/flag_value 用 `[{"op","value","goto"}]`（op 缺省 ">="，允许 >=/>/<=/</==）。未命中一律 else 落顺序下一节点（末节点且未覆盖全部取值 → LomcError；mod/condition 两 case 齐则覆盖） |
| `dice` | 必填 `max`(1～9999)、`header`、`bands`(2～4 条)；可选 `bonus`、`bonus_name`、`bonus_status`。除最后一档外每档必填递增的 `upper`，且须位于 bonus～max+bonus-1 的可达总点数内；每档必填 `text`,`goto` | 不再选择 `CH_*` 官方检查点。Host 生成 0～max 的随机点数并加固定 bonus，按从低到高的 upper 匹配结果；最后一档接收剩余点数。界面继续复用原版 `DiceMenuDialog`，各档文字和跳转均由作者直接设置 |
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
say(luamanager.GetStoryText("MOD_showcase3_main_n7"))
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

由 `tools/extract_editor_data.py` 生成。schema 2 起 `characters`/`stats`/`positions`/`views`/`music`/`sounds`/`env_sounds`/`free_positions` 均为 `{id, name}` 对象数组（characters 另有 portraits，并在原版有资料时带 `title` 与 `intro`）。人物介绍字段严格对应原版 `CharacterIntroPanel` 读取的 `CharacterTitle/<id>` 与 `CharacterIntro0/<id>`；`sounds` 与 `env_sounds` 分别从原版脚本的 `PlaySound` / `PlayEnvSound` 调用提取。schema 3 新增 `dice_meta`（骰子检查点元数据：`{check: {max, bands: [{text, cond}]}}`，bands 按官方展示顺序）与 `death_ids`/`ending_ids` 富化对象数组（name 取自 `data/ref/death_ending_ids.json`，见下方「死亡/结局 id 约定」）。**dice_meta 仅含故事场景检查点**：旅行系统检查点（Travel_*）在故事场景的 CheckPointManager 查不到会崩，提取时已剔除；`dice_checks` 是全名清单，保留全部调用点（含旅行）：

```json
{
  "schema": 3,
  "characters": [{"id": "brother4", "name": "唐惟元", "title": "四师兄", "intro": "本名惟元……", "portraits": ["normal", "nervous1"]}],
  "views": [{"id": "center", "name": "校場_白天"}],
  "music": [{"id": "普通_001", "name": "普通_001"}],
  "sounds": [{"id": "巴掌_001", "name": "巴掌_001"}],
  "env_sounds": [{"id": "雨天_001", "name": "雨天_001"}],
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

1. 启动扫描 `BepInEx/plugins/MortalModHost/mods/*.lommod`，限制物理包大小、ZIP 条目数/单项/总解压大小，复验 id 与脚本 id，并从同一文件句柄计算 SHA-256；随后注册 `MOD_<modid>_<scriptid>` → lua 文本。重复 mod id 或任一注册名碰撞时保留先加载者并按整包拒绝后加载者，菜单不会展示未完整注册的包。
2. Harmony prefix `LuaManager.ExecuteLuaScript()`：注册名命中时用 mod lua 执行并跳过原方法。
3. 入口：Free 自由场景保留“活侠MOD”按钮与 F8 菜单；Title 标题场景在原版“开始游戏”上方显示同风格“开始 MOD 战役”。点击后临时复用原版读档面板：先选择战役，再显示该战役的 001～020 手动栏位和三类自动档；选择不会直接启动。关闭后重建原版 001～020 槽。
   - **完整存档隔离**：每个 `campaign_id` 独占原版风格的 001～020 手动栏位，001 为 `mod_campaign_<campaign_id>`，002～020 为 `_sNNN`；空栏位可新建，已有栏位可覆盖和读取。Story/Free/Battle 自动槽分别追加 `_auto`、`_auto_free`、`_auto_battle`。旧 `mod_<id>` 不探测，原版继续游戏和最近存档不会指向 MOD。
4. **战役**：选择战役只显示其存档页；明确点击"开始新战役"→ 使用当前 MOD 的 001 栏位 `SetSlot("mod_campaign_<campaign_id>")` 或 002～020 的 `_sNNN` 槽（全新 v3 命名空间，不探测旧 `mod_<id>`）→ 官方 `NewGameData()` → postfix 把首个剧情脚本替换为该 mod 的 entry → LoadStory。三类自动槽依次为 `_auto`、`_auto_free`、`_auto_battle`。
5. **原版剧情抑制与位置触发器**：`disable_official_events` 或 F7 生效时，`UpdateCheckMissions` 内暂时隐藏主线触发状态，`HasAnyMissionTrigger` 返回 false，避免返回 Free 时自动启动官方主线/支线；地点点击 postfix `FreePositionData.GetExecuteScript` 优先匹配 manifest.triggers，无 mod 命中时抑制官方地点默认脚本。
6. **兜底**：Story 场景请求的 MOD_ 脚本未注册、包身份/指纹缺失、Lua 编译/运行失败时，不执行或立即停止 `LuaEnvironment` 协程，绕过任务判定直接 `SceneController.LoadFree()` 防软锁。若场景正在切换则保持安全遮罩，待可安全转场时重试，禁止并发启动第二条转场协程。
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
21. **游戏内 Mod 菜单多语言**：菜单文案（`src/I18n.cs` 内嵌 chs/cht/ja/ko 四语言目录）跟随游戏当前语言——反射读 LeanLocalization `CurrentLanguage` 并模糊匹配语言名；官方游戏本身没有日语选项，日语目录实际不会触发；检测失败一律回退 chs。详见 `i18n.md`。
22. **强制玩家内容披露与失败停播**：注册表命中的 Mod 首次开演前，Host 必须同步创建固定的「玩家制作 MOD｜非官方内容 / UNOFFICIAL」主标签，并以独立次级 Text 显示清洗后的作品名、作者自报信息和 SHA-256 前 16 字符。屏幕右上角常驻标签、独立 IMGUI 固定章、对白框内标签、GameOver/EndGamePanel/End/人物介绍卡标签同时覆盖全屏录像、裁对白截图和关键单卡；标签无 Lua/cfg 关闭接口。来源按整段会话污染：Mod 链入官方脚本仍保持原包标签；活动期间嵌套切到另一包会被拒绝；只有实际抵达 Title/Free 才解除。独立于 BepInEx 宿主对象的 guardian 在 Update/LateUpdate、`Canvas.willRenderCanvases` 与 `Camera.onPreCull` 复验对象父级、几何、字体、文案、alpha、材质、显示器与最高排序，被删除/禁用/移出屏幕后自动重建；Lua 篡改 `ActiveSayDialog` 时仍扫描并维护所有可见对白宿主。任何必须标签无法创建/恢复时，立即停止由 Host 推进的 LuaEnvironment 协程、用不依赖 Canvas 的 IMGUI 黑屏显示固定警示并直接返回 Free；异步 Lua 异常不会被 Fungus 吞掉。演出中关闭 Host 总开关只关闭菜单/热键，补丁与披露延迟到 Title/Free 后卸载。
23. **F5 Runtime Trace v1**：只对编辑器固定的 `lom_modkit_preview` / `__lom_modkit_preview.lommod` 开发包启用。记录 `mod_enter`、`story_enter`、`node_enter`、`choice`、`condition_result`、`goto`、`end`、`death`、`runtime_error`；普通玩家加载的正式 Mod 默认不记录。Trace 使用 256 条内存 ring buffer，满后丢弃最旧条目，不写入存档且不会无限增长。新编译器只增加 `if mod_trace_node then ... end` 可选钩子，旧 Runtime 没有该函数时仍按原流程运行。
24. **F5 Runtime Debugger v1**：开发 trace 激活后显示独立 IMGUI 调试窗，列出当前 Mod/Story/Node、`modvars`、`modflags`、可见自定义角色、当前自定义音乐/语音及最近 24 条 trace；F10 可隐藏/重新显示。正式 `.lommod` 不激活该窗口。变量与 Flag 由节点入口处的真实 Lua table 快照取得；尚未使用 `modvars` 的旧剧情会明确显示为空，不伪造状态。
25. **Pause / Step / Continue**：F5 调试窗的「暂停」只设置“下一节点前暂停”，不会把正在显示的官方对话/面板冻结在半个 API 调用中。节点第一行的可选 trace 回调通过 MoonSharp `YieldRequest` 在节点体执行前挂起；「单步」放行当前节点，并在再下一节点体之前重新挂起；「继续」清除请求。宿主协程在暂停期间不调用 `Resume()`。该控制器仅在固定 F5 包激活，正式 Mod 即使包含相同可选节点钩子也始终直接返回，不改变执行路径。
26. **F5 Hot Reload v1**：开发演出仍在 Story 场景且固定试玩包披露仍活动时再次按 F5，宿主会停止旧 `LuaEnvironment` 与 `LuaManager` 协程，丢弃旧 MoonSharp Interpreter（含 `modvars` / `modflags` / 注册回调），并释放人物介绍暂存、死亡/结局覆盖、角色立绘与纹理、背景/CG/Overlay、自定义音乐/环境音/音效/语音及旧包引用。强制披露在重载期间保持，且仅允许 Host 将固定 F5 试玩包的身份原子更新为新 SHA-256；随后重新扫描固定试玩包、卸载并重载 Story，从编辑器本次选中的节点重新开始，不恢复 Lua 指令指针。Trace 保留 256 条有界历史并插入 `hot_reload` 分隔事件，但清空旧变量、Flag 和暂停状态。抵达 `Title`/`Free` 后开发 Trace 与披露一同结束；此时再次 F5 是一次新的试玩，不会拿残留 Trace 误走热重载。正式 Mod 与普通场景请求行为不变。

27. **结构化 Runtime 错误**：所有导致 Mod 演出 fail-closed 中止的故障写入单条 `[mod-runtime-error]` JSON 日志，字段固定为 `mod_id`、`mod_name`、`version`、`story`、`node`、`category`、`error`、`recent_trace`（另含 UTC 时间）。正式 Mod 只保留最多 32 条节点/跳转级轻量 breadcrumb，不记录变量值；错误快照最多附 16 条，每条和错误正文均有长度上限。F5 的 256 条完整开发 trace 规则不变。异常格式化、trace 快照、JSON 序列化或日志 sink 自身再次失败时逐层吞掉并生成最小兜底报告，不能遮蔽原始错误或阻止安全返回 Free；最后一份报告保留在内存中供诊断包读取。

## 7. AI 工具接口（story_api）

editor/story_api.py 是 AI/编辑器共用的受控写入口。规则：**AI 不直接手写 story JSON 或 Lua**，
一切剧情构建经 story_api（models 契约默认值 + lomc 校验/警告），防止骰子菜单崩溃、
transition 黑幕、choice 皮肤崩溃、背景黑屏、人物未登场就做动作等已知坑。

- Python API：
  - `load_editor_data()`：读取编辑器数据（含 dice_meta 等清单），返回 (editor_data, is_fallback)
  - `new_story(story_id="main", title="新剧情", mood=False)`：新建剧情脚本（show 登场 + 空 say 双节点开场，先登场再动作）
  - `add_node(story, node_type, fields=None, after=None)`：按 models 默认值新增节点（62 种类型），未知类型/字段/类型不符→ValueError，节点 id 自动生成，after 指定插入位置（节点 id 或 None=末尾）。登场防线：动作类节点的目标人物在前面未登场/已退场时，自动在它前面插入 show
  - `update_node(story, node_id, fields)`：更新节点字段（同 add 的字段校验），节点不存在→ValueError。登场防线：更新后若动作人物未登场/已退场，自动在该节点前插入 show 并把指向它的 goto/选项/分支跳转改指新节点
  - `get_node(story, node_id)`：读取节点，不存在→ValueError
  - `list_nodes(story)`：返回 [{"id","type","summary"}] 清单
  - `delete_node(story, node_id)`：删除节点，不存在→ValueError
  - `rename_node(story, node_id, new_id)`：重命名节点 id 并同步 start 与全部跳转引用（goto/选项/分支/骰子去向），返回改名后的节点；新 id 限 `[A-Za-z0-9_-]+`，与现有节点冲突→ValueError
  - `move_node(story, node_id, delta)`：按相对位移调整节点顺序
  - `set_start(story, node_id)`：设置起始节点
  - `add_choice(story, options, after=None)`：新增选项分支（2~4 项，dialog 固定 Options）
  - `add_dice(story, maximum, header, bands, bonus=0, bonus_name="", bonus_status="", after=None)`：新增直接配置的骰子检定；bands 为 2～4 档，非末档有递增 upper，每档有 text 与 goto
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
  "content_schema": 1,
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
