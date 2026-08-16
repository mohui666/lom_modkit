# 活俠傳 Mod 包格式（v3 契約）

> 語言：[简体中文](../zh_CN/mod_format.md) · 繁體中文（本文） · [日本語](../ja/mod_format.md) · [한국어](../ko/mod_format.md)

**所有元件（編輯器 / 編譯器 / 執行階段外掛）以本文件為準。** 改動需同步更新本文件。
文中規則的官方腳本/反編譯實證材料見 `../research/`，正文不重複展開。

## 1. 包結構

`.lommod` 檔案 = zip 壓縮檔，內部結構：

```
manifest.json          # 必填，包元信息
story/<id>.json        # 必填≥1，剧情源文件（编辑器可编辑的源格式）
lua/<id>.lua           # 必填≥1，编译产物（运行时只读这里）；每个 story/<id>.json 对应一个
texts.json             # 必填，已读文本表：{MOD_<modid>_<scriptid>_<nodeid>: 文本}（say 节点文本）
assets/                # 可选，自定义资源
                       #   图片：结局插图 / 人物介绍图 PNG/JPG
                       #   用户音频：assets/user/audio/<content_id>/
```

- `<id>` 規則：`[a-zA-Z0-9_\-]+`，包內唯一，即「劇情腳本 id」。
- 匯出（打包）時必須重新編譯：story/*.json → lua/*.lua，二者同名。
- 執行階段外掛**只讀 manifest.json、lua/ 目錄與 assets/**；story/*.json 給編輯器回讀/再編輯用。編譯器只打入劇情明確引用的 PNG/JPG（單張 ≤8MB）與明確引用的 `user:` 音訊。匯出的 `.lommod` 自包含，玩家機器不需要編輯器倉庫。
- texts.json 由打包時自動產生：收集每個 story 的全部 **say** 節點文字，key 與 lua 裡 `GetStoryText` 的 key 一一對應；執行階段註冊進 LeanLocalization（見 §4/§6）。**death 文字不進 texts.json**：由 codegen 發射 `mod_set_death_text(<標題>, <文字>)` 兩參 lua_str 字面量（見 §3.1/§6）。

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
- `id`：mod 唯一 id（`[a-z0-9_\-]+`），執行階段註冊名前綴，防衝突。
- `entry`：入口劇情腳本 id，必須存在。
- `campaign`（可選）：戰役模式。
  - `new_game`：true 時本 mod 出現在遊戲內 mod 選單的「開始新戰役」區，點擊後**隔離存檔槽**（`SetSlot("mod_<modid>")`，不覆蓋玩家正常存檔）開新遊戲，首個劇情腳本替換為本 mod 的 `entry`。
  - `disable_official_events`（可選，bool，預設 false）：true 時本戰役**停用原版劇情事件**——返回 Free 時不自動啟動無地點主線/支線，地圖位置只保留本 mod 觸發器（未命中則該位置預設活動不可用，需 mod 自帶保底觸發器）。
  - `triggers`：自由模式觸發器陣列。`type="position"`：點擊地圖位置 `position`（PositionType 列舉 id：Mall/Center/Alchemy/Forge/BackMountain/Room1/Door/Study/Kitchen/Room2/Secret）時，該位置預設活動腳本替換為 `script`（同包腳本 id）。可選條件全部命中才生效（多條件 AND；**陣列順序=優先級**，執行階段取第一個全部命中的觸發器）：
    - `when_flag_set` / `when_flag_clear`：劇情 flag（即 `flag` 節點 AddStory 的 key，存檔持久化）已設定/未設定。
    - `when_month`：整數 1~12，僅該月份生效。
    - `when_stage`：整數 1~3（旬：上/中/下），僅該旬生效。
    - `when_affinity`：`{"character": <人物 id>, "min": <整數>}`，好感度 ≥ min。
  - 預設官方主線/支線優先；`disable_official_events` 或 F7 臨時開關生效時跳過官方任務判定，優先匹配 mod 觸發器。
  - **觸發器按戰役隔離**：有活躍 mod 戰役時只匹配當前戰役 mod 的觸發器；無戰役時全部 mod 參與匹配、先載入者優先（載入順序=檔名序）。
  - 觸發器範例（練武場：好感事件 > 下旬晚練 > 預設閒逛）：

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

## 3. story/*.json — 劇情腳本格式

```json
{
  "id": "main",
  "title": "显示给玩家的标题",
  "mood": false,
  "start": "n1",
  "nodes": [ ... ]
}
```

- `mood`（可選，bool，預設 false）：心情氣泡開關。false=每次 show 節點末尾與每次 say 節點前後發射 `mod_hide_mood()`（隱藏官方圓形情緒面板）；true=保留官方心情氣泡。
- `nodes` 為節點陣列；預設按陣列順序依序執行（隱式 goto 下一個節點）。
- 每個節點有唯一 `id`；任何節點可顯式寫 `"goto": "<nodeId>"` 覆蓋順序流。
- `choice` / `branch` / `dice` 的分支必須用 `goto` 指到目標節點 id。
- 多個前驅匯入同一節點（匯合點）合法。

### 3.1 節點類型（全量 43 種）

**演出類**

| type | 欄位 | 說明 |
| --- | --- | --- |
| `music` | `name`；可選 `op`("play"預設/"stop"/"fadeout")，fadeout 時 `seconds`(預設2) | 官方名繼續 `PlayMusic` / `StopMusic` / `FadeOutMusic(seconds)` 後 **`wait(seconds)`**。`name` 以 `user:` 開頭時是使用者內容引用（§8），由執行階段從**當前包** `assets/user/` 解析播放。禁止本機絕對路徑 |
| `sound` | `name`；可選 `kind`("sound"預設/"env")，`op`("play"預設/"fadeout"僅env，`seconds`預設1) | 官方名繼續 `PlaySound` / `PlayEnvSound` / `FadeOutEnvSound(seconds)` 後同樣 **`wait(seconds)`**。`user:` 引用規則同 music，且 `audio_kind` 必須與節點匹配 |
| `scene` | `view` | 切場景：`runblock(flowcharts.view,"out")` 後 `ViewName=view; runblock(...,"view")`。`view="out"` 只淡出；`"black"/"white"` 為純色。非純色 view 先 `runwait(flowcharts.LoadView(view))` 預先載入背景資產（不預載則背景黑畫面） |
| `show` | `character`, `position`；可選 `portrait`(預設normal), `facing`(預設right), `fadeDuration`(0), `moveDuration`(0) | 載入並顯示人物。story.mood 為 false 時末尾（Focus 後）追加 `mod_hide_mood()` |
| `move` | `character`, `from`, `to`；可選 `duration`(預設1) | 移動並 `wait(duration)` |
| `face` | `character`, `facing` | 轉向 |
| `hide` | `character`；可選 `fadeDuration`(預設0) | 隱藏人物 |
| `focus` | `character` | `characters.Focus` |
| `offset` | `character`, `x`, `y`, `duration` | 人物偏移演出 `runwait(characters.MoveOffsetCoroutine(id,x,y,t))` |
| `say` | `text`；可選 `character`, `portrait`(預設normal), `mode`("character"預設/"think"/"narrative"/"center")，可選 `voice` | 對話/內心獨白(帶os_mask)/旁白/居中旁白。narrative 與 center 忽略 character。**已讀機制**：文字不裸進 Lua，發射 `say(luamanager.GetStoryText("MOD_<modid>_<scriptid>_<nodeid>"))`（無 modid 時保底 "MOD"），文字本體進 texts.json 由執行階段註冊。**`voice`**（可選）：使用者音訊引用，如 `user:mohui.line_01`；進入本句前 `mod_play_voice`（先停上一句），`say()` 返回後 `mod_stop_voice`。語音走獨立通道，`sound` / `StopMusic` 不停它。禁止絕對路徑與官方音效名 |
| `choice` | `options`: `[{"text","goto"}]`（2~4 項）；可選 `dialog`(預設"Options"，外觀見 §3.3) | 選項選單 `choose()` |
| `shock` | `character`；可選 `duration`(預設0.5) | 人物震動（flowcharts.common "shock"） |
| `mask` | `show`(bool) | 獨白遮罩 `os_mask.Show` |
| `intro` | 可選 `intro_source`(`official` 預設/`custom`)。official 必填 `character`；custom 必填 `name`,`text`，可選 `title`,`image`（包內 `assets/` PNG/JPG，≤8MB）、`image_scale`(40~160，預設100)、`image_x`/`image_y`(-30~30，預設0) | official 呼叫原版 `runwait(intropanel.Show(character))`；custom 呼叫 `mod_prepare_character_intro(title,name,text,image,scale,x,y)`，重用同一 CharacterIntroPanel。圖片按螢幕安全區獨立佈局並保持比例；x 正數向右、y 正數向上，無圖時隱藏頭像區域 |
| `effect` | `name`；可選 `x`,`y`,`a`,`b`,`c`(數值，預設0/0/1/1/1)，`play`(bool，預設true) | 螢幕特效 `effects.SetupEffect(name,x,y,a,b,c,play)`，如 Hit_001/Blood_002/Sword_001。`play=false` 發射停止呼叫（末參 0）：**循環類特效不會自動銷毀**（如 EventBubble/Glow），必須後接 play=false 的同參節點停止，否則常駐畫面（舊資料的 `d` 欄位仍相容：無 play 時用 d） |
| `transition` | `phase`("in"/"out")；可選 `dir`(預設"lr"，lr/rl/tb/bt) | 黑場轉場 `runwait(transitionblack.TransitionIn/Out(dir))`。**必須成對使用**：TransitionIn 隱藏劇情 UI 並蓋滿黑幕，TransitionOut 才恢復；有 in 無 out 時編譯器警告（畫面會一直黑屏） |
| `camera` | `name`, `active`(bool) | 鏡頭濾鏡 `maincamera.ActiveVolume(name, 0 | 1)`，如 stage-memory/stage-dream/stage-fire/stage-blurdim |
| `block` | `flowchart`("view"/"common"), `name`；可選 `vars`: `[{"name","value"}]` | 通用 flowchart 區塊呼叫：`getvar` 逐個賦值後 `runblock(fc, name)`。覆蓋 out_white/shake/flash/vshock 等 |
| `cg` | `action`("show"/"hide"), `kind`("picture"/"item"/"big"/"map"/"family"/"title")；可選 `key`, `key2`, `n1`, `n2` | mainui 圖片/地圖/家譜/標題：`ShowPicture(key)`/`HidePicture`/`ShowItemPicture`/`ShowBigPicture`/`ShowMap(key,key2)`/`ShowFamilyTree(key,key2,n1,n2)`/`DisplayTitle(key)` 等 |
| `dim` | `character`, `dimmed`(bool 必填，預設 true) | 人物壓暗 `stage.SetDimmed(character, dimmedState)`（實參 character 在前、bool 在後；dimmed=true 時官方實作還會隱藏該角色心情氣泡） |
| `message` | `text`（必填非空，多行合法） | 系統提示 `mainui.DisplayMessageText(text)` 顯示**原文**（DisplayMessage 走本地化 key 解析，用 Text 版避免自訂文字被當 key 查空） |
| `rotate` | `character`, `angle`(int 必填，預設 180), `duration`(float 必填，預設 1，>0) | 人物旋轉 `characters.Rotate(key, angle, duration)`——**官方參數序 angle 在前、duration 在後** |
| `dayenv` | `day_type`（int 必填，1=白天 / 2=晚上） | 日夜環境 `luamanager.SetGameDayEnvironment(day_type)`。**欄位名 day_type**：避免與節點通用鍵 "type" 衝突 |

**數值/狀態類**

| type | 欄位 | 說明 |
| --- | --- | --- |
| `stat` | `key`, `delta`；可選 `waitDisplay`(預設true), `display`(預設1), `mode`(預設"") | 主角屬性增減 `statmodifymanager.Player(key, delta, mode, display)` |
| `stat_set` | `key`, `value`；可選 `update`(bool預設false) | 絕對設定 `SetPlayer(key, value)`；update=true 用 `UpdateSetPlayerStat`（title 等用） |
| `affinity` | `character`, `delta` | 人物好感度 `statmodifymanager.Character(character, delta, 1)` |
| `talent` | `talent`, `level`(±1) | 天賦 `statmodifymanager.AddTalent(id, level)` |
| `item` | `kind`("book"/"misc"/"special"), `item`, `count`(預設1)；可選 `remove`(bool預設false) | 物品增減 `AddBook/AddMisc/AddSpecial(id,count)`；remove 時 `RemoveBook/RemoveMisc(id)`（僅 book/misc） |
| `flag` | `flag` | mod 劇情 flag：`statmodifymanager.AddStory(flag)` + `modflags[flag]=true` |
| `game_flag` | `flag`, `value`；可選 `op`("set"預設/"add") | 官方任務 flag：`SetFlag(id, 狀態)` / `AddFlag(id, ±增量)`。**id 必須是遊戲已有 FlagData**（14_屬性與Flag 表），否則遊戲靜默忽略 |
| `enemy` | `op`("team"=向心力/"level"=門派規模/"people"=門派人數/"id"=選擇目前敵對門派), `enemy`(戰役門派 id), `value`(變化量，id 操作不需要), `display`(僅 team/people 使用，預設1) | 修改 **Battle 多人戰役**使用的門派狀態 `ModifyEnemyTeam/Level/People/Id`；不設定 Combat 一對一決鬥敵人 |
| `battle_skill` | `op`("set"/"active"/"reset"), `key`(reset 不需要), `index`(set 用, 預設2), `active`(active 用, 預設1) | 戰場技能 `SetPlayerBattleSkill/SetBattleSkillActive/ResetBattleSkill` |
| `mission` | `name`, `key` | 任務操作 `statmodifymanager.Mission(name, key)`：`Mission("Main","M0001")` 推進主線 / `Mission("S2200","clear")` 清支線 |
| `time` | `op`("set"/"round"/"month"/"mission")；set 用 `year,month,stage`；mission 用 `name,year,month,stage` | 時間 `SetGameTime/NextRound/NextMonth/SetMissionTime` |
| `autosave` | 可選 `kind`("story"預設/"free"/"prologue")；可選 `save_button`(0/1，單獨控制存檔按鈕) | `AutoSave()/AutoFreeSave()/PrologueSave(mode)`；`save_button` 單獨 emit `ToggleSaveButton(n)` |

**流程類**

> 原版術語：`Combat` 是一對一決鬥，`Battle` 是帶門派人數、陣型與戰場技能的多人戰役；兩套關卡編號不能混用。

| type | 欄位 | 說明 |
| --- | --- | --- |
| `branch` | `cases`(≥1)；可選 `source`("mod"預設/"game"/"stat"/"flag_value"/"condition")。鍵欄位：source=stat 時用 `stat`（屬性 id，editor_data stats 清單），其餘來源用 `flag`（非空） | 條件分支，五來源：mod=按 modflags 是否已設；game=官方檢查點 `checkpointmanager.Switch(flag)`；stat=主角屬性 `luamanager.GetStatData(stat, 1)`；flag_value=官方任務旗標 `tonumber(luamanager.GetFlagData(flag))`；condition=官方條件檢查點 `checkpointmanager.Condition(flag)`（bool）。case 結構按來源：mod/condition 用 `[{"value","goto"}]`（value 僅 1/2：mod=已設/未設，condition=真/假）；game 用 `[{"value","goto"}]`（任意整數）；stat/flag_value 用 `[{"op","value","goto"}]`（op 預設 ">="，允許 >=/>/<=/</==）。未命中一律 else 落順序下一節點（末節點且未覆蓋全部取值 → LomcError；mod/condition 兩 case 齊則覆蓋） |
| `dice` | `check`, `options`: `[{"goto_大成功","goto_成功","goto_失敗","band_texts"?}]`（恰好 1 條） | 骰子檢定。**check 必須是帶官方元資料的檢查點**（editor_data 的 dice_meta：骰子範圍 max 與結果帶 bands；無元資料的檢查點會讓遊戲內骰子選單 NRE 崩潰）。發射官方五步鏈 + 按結果帶數逐帶發射選項（文字+條件）；分支按帶品質名次映射：最差帶→goto_失敗，中間帶→goto_成功，最優帶→3帶及以上 goto_大成功 / 2帶 goto_成功。帶品質按條件數值推斷（同值 >系優於 <系）。**band_texts**（可選）：逐帶覆寫骰子選單選項文字（條數=結果帶數，每項非空，否則 LomcError）；發射 `<作者文字> \| <官方cond>`（作者文字為字面量，不進 texts.json；ASCII \| 淨化為全形｜；cond 永遠用官方元資料）。預設用官方結果帶文字 |
| `goto_scene` | `scene`("Free"/"Title"/"Combat"/"Battle"/"GameOver"/"End"/"Story"/"DemoEnd")；可選 `key`(Combat=戰鬥id/Battle=戰役id/GameOver=死亡畫面id/End=結局標識), `next`, `title`, `desc`(均為 str，僅 End/GameOver 用), `image`(str，**僅 End 用**：包內圖片相對路徑，如 `assets/ending.png`) | 普通場景仍為 `luamanager.ChangeScene(scene,key,next)`。**End 特例按原版汗青書流程**：快取自訂標題/正文/插圖 → `runwait(endgamepanel.Open("__MORTAL_MOD_END__"))` → 玩家確認 → 黑幕 → Title；執行階段 patch 真正的 `EndGamePanel`，完整重用官方版式；`image` 寫入左頁 `_picImage`，留空時借原版結局 20047 的 Picture 佔位；圖片缺失/損壞只警告並退回佔位。End/GameOver 的 next 無效（原版按鈕固定為讀檔/標題），舊值忽略並警告（舊相容值 Story 按 Title 處理、不警告）。只有不帶自訂內容且給官方 key 時才直接開啟官方結局條目（按原版解鎖/記錄並給警告）。mod 專屬 End key 若無 title/desc/image、mod 專屬 GameOver key 若無 title/desc，直接驗證失敗，避免空白卡 |
| `panel` | `panel`("martial"/"weapon"/"poison"/"cg"/"cgvideo"/"shop"/"newshop"/"credit"/"endgame")；可選 `key`(cg/cgvideo/endgame 的 id), `discount`(shop 用, 預設0), `mode`(martial 用, 預設0) | 開啟系統面板，除 newshop 外均 `runwait`：`martialpanel.Open(mode)`/`weaponupgradepanel.Open()`/`poisonupgradepanel.Open()`/`cgpanel.Open(key)`/`cgvideopanel.Open(key,0)`/`shoppanel.Open(discount)`/`shoppanel.NewShop()`/`creditpanel.Open()`/`endgamepanel.Open(key)` |
| `wait` | `seconds` | `wait(seconds)` |
| `end` | 可選 `next_script` | 有：`SetNextScript("MOD_<modid>_<id>")`+`Init()` 鏈到同包腳本；無：`ChangeScene("Free","","")` 回自由模式 |
| `death` | `text`（必填非空，多行合法）、`death_id`（必填）；可選 `title`（str，預設「勝敗乃兵家常事」）、舊欄位 `next` | **死亡文字**：黑屏過渡（view="black"）→ `mod_set_death_text(title, text)`（兩參 lua_str 字面量，**不進 texts.json / 已讀系統**）→ `luamanager.ChangeScene("GameOver", death_id, "Title")` 進**官方 GameOver 死亡畫面**（黑底紅字 + 讀檔/標題按鈕，見 §6）；原版不讀取自訂 next，舊值忽略並警告。`death_id` 必須是 ≥900000 的 mod 專屬數字 id（否則 LomcError，見「死亡/結局 id 約定」）。終止節點（自帶流轉，不允許顯式 goto，可作末節點收尾） |
| `raw` | `code` | 原生 Lua 逃逸口：原樣插入程式碼（多行合法）。**機制保底**：任何節點表達不了的官方機制用它 |

### 3.2 常用取值（以 data/editor_data.json 為權威清單，schema 2 起帶中文名）

- 站位 position：`SL L1 L2 M R1 R2 RM2 SR …`（共 36 個，S=屏外 L=左 M=中 R=右 B=後 C=央）
- 表情 portrait：`normal nervous1..3 angry1 angry2 laugh1 gloomy2 …`（按人物配置，缺失時遊戲退回第一張立繪）
- say mode：`character` 對話 / `think` 內心獨白 / `narrative` 旁白 / `center` 居中旁白
- stat key：`mental(心相) money(銀兩) disposition behaviour karma fame talking team …`（31 個）

### 3.3 選項選單外觀（choice.dialog）

**僅 `Options` 可用**（預設，純文字選項；Dice 為骰子節點內部專用）。其餘外觀（Talk/Meet/Door/Section_* 等）是自由場景的 break 格式選單（選項文字為 `類型+key+行動點+貢獻` 四段 `+` 分隔），純文字選項會觸發 `BreakOptionButton.UpdateContent` 的 IndexOutOfRange 崩潰（選單凍結）——編譯器直接報錯拒絕。發射：`setmenudialog(menudialogs.Options)` → `choose()` → `menudialogs.Options.SetActive(false)`。

## 4. story.json → Lua 編譯約定（lomc 實作）

- 每個節點編譯為一個 Lua 函式；檔頭前向宣告 `local node_n1, node_n2, ...`，然後 `node_nX = function() ... end`；流轉尾呼叫 `return node_<goto>()`；頂層 `return node_<start>()`。
- 文字跳脫：`\`→`\\`，`"`→`\"`，換行→`\n`，`\r`→`\r`。
- 每個腳本開頭 emit `modflags = modflags or {}`（全域表，Story 場景工作階段內持續，鏈式腳本共享；不存檔），緊跟一行 `mod_set_mood(true|false)`（story 頂層 mood 宣告，預設 false；見 §6）。
- `flag` 節點雙 emit：`AddStory` + `modflags[flag]=true`。
- **分支保底**：choice 外任何多路結構不允許靜默落空——未命中 case 時 else 落順序下一節點；無法保底（branch 為末節點且未覆蓋全部返回值）視為驗證錯誤。
- 節點 id 字元集 `[a-zA-Z0-9_]+`（腳本 id 允許 `-`）。
- story 頂層 `title` 可選。
- **已讀 key 規則**：所有 say（character/think/narrative/center）節點的文字一律發射 `say(luamanager.GetStoryText(key))`，key = `MOD_<modid>_<scriptid>_<nodeid>`；modid 來自 manifest（打包時），獨立 build/編輯器預覽預設時用 "MOD" 保底。**death 文字不走已讀 key**：發射 `mod_set_death_text(<標題字面量>, <文字字面量>)`（標題預設/空字串用「勝敗乃兵家常事」），文字不進 texts.json。
- **結局/死亡卡片規則**：goto_scene scene=End 且帶 title/desc/image 時先發射 `mod_set_ending_text(...)`，再按原版汗青書流程顯示；image 是左頁插圖，不是全螢幕背景。scene=GameOver 帶 title/desc 時改走 `mod_set_death_text(<title>, <desc>)`。death 節點同樣發射 `mod_set_death_text(<title>, <text>)`（兩參；單參舊包相容仍由執行階段支援）。兩個全域呼叫由執行階段外掛註冊（§6）。
- **mood 規則**：story.mood=false 時另在 show 節點末尾（Focus 後）與 say 節點 say(...) 前後各發射一次 `mod_hide_mood()`；true 時不發射。
- **death 發射**：見 §3.1 death 行（runblock out → ViewName="black" → runblock view → `mod_set_death_text(title, text)` → ChangeScene("GameOver", death_id, next)）。
- 最後一個節點不是 `end`/`death`/`goto_scene`/`raw` 且無 goto → 驗證錯誤。
- `choice`/`branch`/`dice`/`end`/`death`/`goto_scene` 寫顯式 `goto` → 驗證錯誤。
- `say` 的 narrative/center 模式給 character 允許但忽略。
- `raw` 節點內容原樣插入（編譯器不做語法檢查）；其後流轉照常（順序/goto）。
- **非致命警告**：以 `-- lomc 警告：` 註解形式插在 Lua 頭部（如 transition 有 in 無 out）；`lomc check` 同步列印到 stderr。

關鍵 API 範式：

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

## 5. data/editor_data.json — 編輯器資料契約（schema 3）

由 `tools/extract_editor_data.py` 產生。schema 2 起 `characters`/`stats`/`positions`/`views`/`music`/`free_positions` 均為 `{id, name}` 物件陣列（characters 另有 portraits）；schema 3 新增 `dice_meta`（骰子檢查點元資料：`{check: {max, bands: [{text, cond}]}}`，bands 按官方展示順序）與 `death_ids`/`ending_ids` 富化物件陣列（name 取自 `data/ref/death_ending_ids.json`，見下方「死亡/結局 id 約定」）。**dice_meta 僅含故事場景檢查點**：旅行系統檢查點（Travel_*）在故事場景的 CheckPointManager 查不到會崩，提取時已剔除；`dice_checks` 是全名清單，保留全部呼叫點（含旅行）：

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

### 死亡/結局 id 約定（mod 專屬區間）

官方 GameOver/EndGamePanel 會用 id 查 LibrarySystem 並可能執行 LibraryItemData.Add()（解鎖/記錄官方結局）。自訂 End 固定用不存在的內部 key，不查詢/寫入官方結局；只有「無自訂內容、直接開啟官方 End key」會按原版記錄。

- **mod 死亡/結局 id = `9<官方id>`**（900000 區段）：官方死亡 10021 → mod 910021；官方結局 20003 → mod 920003。與官方 1xxxx（死亡）/2xxxx（結局）/4xxxx（後日談）全區間不撞。
- 自造 GameOver id 查不到官方條目 → 無副作用，文字由外掛注入；自造 End id 只作 mod 內標識，真正顯示走固定內部 key。
- `death` 節點的 `death_id` 驗證 ≥900000 整數。空白的自造 GameOver/End 卡會被編譯器拒絕；無自訂內容直接使用官方 key 時給出非致命存檔污染警告。
- 權威參考：`data/ref/death_ending_ids.json`：`death` 106 個（10000~10104、11000）、`ending` 54 個（20000~20053）、`epilogue` 4 個（40000~40003）。提取器用其標題富化 editor_data 的 death_ids/ending_ids；編輯器 death_id 輸入框列出前 5 個官方參考。

## 6. 執行階段外掛行為（MortalModHost）

1. 啟動掃描 `BepInEx/plugins/MortalModHost/mods/*.lommod`，註冊 `MOD_<modid>_<scriptid>` → lua 文字。
2. Harmony prefix `LuaManager.ExecuteLuaScript()`：註冊名命中時用 mod lua 執行並跳過原方法。
3. 入口：Free 自由場景與 Title 標題畫面左下角「活俠MOD」按鈕 + F8（可配置）開啟選單。Free 選單分「演出 mod 劇情」與「開始新戰役」兩區；Title 選單僅「開始新戰役」區（演出劇情需要已載入的存檔玩家狀態，只在 Free 提供）。
4. **戰役**：點擊「開始新戰役」→ `SetSlot("mod_<modid>")`（隔離存檔槽）→ 官方 `NewGameData()` → postfix 把首個劇情腳本替換為該 mod 的 entry → LoadStory。
5. **原版劇情抑制與位置觸發器**：`disable_official_events` 或 F7 生效時，`UpdateCheckMissions` 內暫時隱藏主線觸發狀態，`HasAnyMissionTrigger` 返回 false，避免返回 Free 時自動啟動官方主線/支線；地點點擊 postfix `FreePositionData.GetExecuteScript` 優先匹配 manifest.triggers，無 mod 命中時抑制官方地點預設腳本。
6. **保底**：Story 場景請求的 MOD_ 腳本未註冊（mod 被刪）時，不執行並 `ChangeScene("Free","","")` 防軟鎖。
7. mod 不修改官方腳本與文字表；mod 的 flag 進 StoryKeyList，存檔相容。
8. **texts.json 註冊**：載入 .lommod 時把 texts.json 的 key→文字註冊進 LeanLocalization（`Story/`+key）；`GetStoryText` 按 key 查已讀系統：已讀→黃色+可快進，未讀→正常色+記入已讀，查不到返回 key 本身。
9. **mod_hide_mood**：註冊全域 Lua 函式 `mod_hide_mood()`（無參），隱藏全場角色圓形情緒面板（CharacterMoodPanel）；編譯器按 story.mood 開關在 show/say 處發射（見 §4）。
10. **mod_set_mood**：註冊全域 Lua 函式 `mod_set_mood(bool)`，按腳本頭部宣告硬控官方心情面板開關（ShowMood），每個 mod 腳本入口發射一次，鏈式腳本逐腳本切換生效。
11. **UpdateTranslations 防 wipe**：官方文字刷新會清掉外掛註冊的 mod 文字，必須 hook 並在刷新後重播 texts.json 註冊（載入時快取全部註冊項），保證 mod key 永不失效。
12. **人物介紹卡**：官方人物保持原始 `CharacterIntroPanel.Show(key)` 行為；自訂人物在特殊 key 上由 Harmony 接管，重用官方面板版式，寫入自訂稱號/姓名/正文。可選 `image` 從當前 `.lommod` 的 `assets/` 解碼放入獨立安全佈局：預設中心螢幕 `(31%,50%)`，最大寬/高螢幕 `(30%,62%)`，保持比例；`image_scale` 在自動適配尺寸上縮放，`image_x/image_y` 按螢幕百分比微調。關閉時銷毀暫存紋理並完整恢復原版控制項；無圖時隱藏頭像區域，不修改官方本地化表或關係資料。
13. **結局/死亡卡片繪製**：註冊兩個全域 Lua 函式（見 §3.1/§4）：
    - `mod_set_death_text(title, desc)`：快取死亡標題/描述；Harmony postfix `GameOverController` 把兩段文字寫入官方 `_titleText`/`_descTextPrefab` 控制器，官方佈局顯示在死亡畫面中央。單參呼叫按舊契約當 desc、標題留空（舊包相容）。
    - `mod_set_ending_text(title, desc[, image])`：快取結局標題/描述與可選包內圖片；Harmony postfix 包裝 `EndGamePanel.Open`，在官方第一次畫布 fade 前寫入 `_titleText/_descText` 與左頁 `_picImage`；未給圖片時借用官方結局 20047 的 Picture 佔位。官方漸顯、等待確認與淡出全保留；顯示期間暫時關閉 `_saveLibrary` 避免 mod key 進入傳奇存檔槽，結束後恢復。
    - 新編譯器的自訂 End 不再進入簡化 `EndGameController` 場景；舊包仍保留原 End 場景覆蓋相容。GameOver 自造 id 無文字與 End 自造 id 無內容均在編譯期阻止。
14. **編輯器單次試玩協定**：編輯器把入口章節的 `start` 暫時改為當前選中節點，安裝為固定包 `__lom_modkit_preview.lommod`（manifest id `lom_modkit_preview`），隨後原子寫入外掛目錄 `preview-request.json`。執行階段每 0.35 秒檢查一次：Free 場景直接演出，Title 場景用 `mod_lom_modkit_preview` 隔離槽開局，其它場景等待到安全場景；消費後刪除請求與暫存包。請求只接受 format=1 及 `[A-Za-z0-9_-]+` 的 mod/script/node id，正式 Mod 包不在自動刪除範圍內。
15. **mod 新戰役發放 2 點命運**：官方新遊戲初始帶命運點，mod 隔離存檔初始為 0，骰子「逆天」流程（`DiceMenuDialog.CheckRevolution` 要求 命運>0）在 mod 戰役中不可用；NewGameData postfix 在替換首腳本後給 mod 戰役 `GameStatType.命運` 加 2 點。官方新遊戲不受影響。
16. **mod 劇情放開骰子範圍修改**：官方「修改範圍」按鈕要求二周目且持有成就 30016；mod 劇情中（`CurrentStoryScript` 以 `MOD_` 開頭）`get_NewGamePlus` prefix 返 true，且 `CheckRevolution` 原返 true 時直接啟用 `_rangeButton`（不在 mod 裡解鎖官方成就 30016，避免污染官方存檔）。官方劇情完全不受影響。
17. **使用者音訊**：`LuaManager.PlayMusic/PlaySound/PlayEnvSound` 參數以 `user:` 開頭時由外掛接管，從**當前演出 Mod 包**的 `UserContents` 解析（`assets/user/audio/<id>/content.json` + 主檔案），解碼後用 Windows `waveOut` 播放（本遊戲主混音是 Wwise，Unity `AudioSource` 經常無聲）。官方名字一律放行給原版 Wwise。執行階段禁止讀取 `%APPDATA%/lom_modkit/repository`。兩個 Mod 即使 ID 相同也只解析自身包。支援格式僅 `.ogg` / `.wav`，單條 ≤20MB。自訂 fadeout 是輸出音量淡出（隨後仍有編譯器發射的 `wait`）；切到自訂音樂會先停官方 Wwise 音樂（官方 `StopMusic` 會同時清環境音）。
18. **對白語音**：註冊 `mod_play_voice(ref)` / `mod_stop_voice()`。`mod_play_voice` 先停當前語音再播（不循環，走獨立 `_voice` 通道）。`sound` 節點、自訂音效、`StopMusic` 都不碰這條通道。劇情中斷、切官方腳本、重載 Mod 時 `StopEverything()` 會停語音。無 `voice` 的舊 Lua 不會呼叫這兩個函式，行為不變。
19. **遊戲內 Mod 選單多語言**：選單文案（`src/I18n.cs` 內嵌 zh_CN/zh_TW/ja/ko 四語言目錄）跟隨遊戲當前語言——反射讀 LeanLocalization `CurrentLanguage` 並模糊匹配語言名；官方遊戲本身沒有日語選項，日語目錄實際不會觸發；偵測失敗一律退回 zh_CN。詳見 `i18n.md`。

## 7. AI 工具介面（story_api）

editor/story_api.py 是 AI/編輯器共用的受控寫入口。規則：**AI 不直接手寫 story JSON 或 Lua**，
一切劇情構建經 story_api（models 契約預設值 + lomc 驗證/警告），防止骰子選單崩潰、
transition 黑幕、choice 外觀崩潰、背景黑畫面、人物未登場就做動作等已知坑。

- Python API：
  - `load_editor_data()`：讀取編輯器資料（含 dice_meta 等清單），返回 (editor_data, is_fallback)
  - `new_story(story_id="main", title="新剧情", mood=False)`：新建劇情腳本（show 登場 + 空 say 雙節點開場，先登場再動作）
  - `add_node(story, node_type, fields=None, after=None)`：按 models 預設值新增節點（43 種類型），未知類型/欄位/類型不符→ValueError，節點 id 自動產生，after 指定插入位置（節點 id 或 None=末尾）。登場防線：動作類節點的目標人物在前面未登場/已退場時，自動在它前面插入 show
  - `update_node(story, node_id, fields)`：更新節點欄位（同 add 的欄位驗證），節點不存在→ValueError。登場防線：更新後若動作人物未登場/已退場，自動在該節點前插入 show 並把指向它的 goto/選項/分支跳轉改指新節點
  - `get_node(story, node_id)`：讀取節點，不存在→ValueError
  - `list_nodes(story)`：返回 [{"id","type","summary"}] 清單
  - `delete_node(story, node_id)`：刪除節點，不存在→ValueError
  - `rename_node(story, node_id, new_id)`：重新命名節點 id 並同步 start 與全部跳轉引用（goto/選項/分支/骰子去向），返回改名後的節點；新 id 限 `[A-Za-z0-9_-]+`，與現有節點衝突→ValueError
  - `move_node(story, node_id, delta)`：按相對位移調整節點順序
  - `set_start(story, node_id)`：設定起始節點
  - `add_choice(story, options, after=None)`：新增選項分支（2~4 項，dialog 固定 Options）
  - `add_dice(story, check, goto_成功, goto_失敗, goto_大成功="", band_texts=None, after=None)`：新增骰子檢定（check 必須有官方元資料，按結果帶數驗證 goto；band_texts 條數必須等於結果帶數且每項非空）
  - `add_say(story, text, character=None, mode="character", portrait="normal", voice=None, after=None)`：新增對白（character 模式必填 character；narrative/center 不寫 character；voice 可選 user: 音訊引用）
  - `add_death(story, text, death_id, next="Title", title=None, after=None)`：新增死亡文字節點（text 必填非空多行；death_id 必填 ≥900000 的 mod 專屬數字 id；next 僅接受 Title；title 可選短標題，預設/空字串用「勝敗乃兵家常事」）
  - `add_scene(story, view, after=None)`：新增場景切換
  - `check_story(story)`：只驗證，返回 (errors: list[str], warnings: list[str])
  - `compile_story(story)`：驗證+編譯，返回 (lua|None, errors, warnings)，失敗時 lua 為 None
  - `load_story_json(path)` / `save_story_json(story, path)`：story.json 讀寫（UTF-8）
  - `pack_mod(mod_dir, output=None)`：驗證 manifest + 全部編譯 + 打 .lommod，返回產物路徑
- CLI：python editor/story_api.py check|compile|pack|new-story（AI 子行程友善，退出碼 0/1，中文錯誤）
- 關鍵不變式（編譯器強制，API 透傳）：choice.dialog 僅 Options；dice.check 必須有官方元資料
  （骰子範圍+結果帶）；transition in/out 成對；scene 自動預載背景；
  **show/say 的 (character, portrait) 必須落在 data/editor_data.json 的角色表情表內**
  （表不可用/角色不在表 → 放行；角色在表但表情不在其列表 → LomcError/ValueError——
  遊戲 LoadCharacterPortrait 對無效表情 key 拋 KeyNotFoundException → Lua 協程死 → 對話凍結）。
  say/show 引用的人物必須先 show 上台（未上台同樣拋 KeyNotFoundException），
  寫入口的登場防線會自動補 show（見 add_node/update_node），編輯器體檢對多路徑匯合做圖級保底。

## 8. 使用者內容（User Content，v1 僅音訊）

開發環境倉庫在 `%APPDATA%/lom_modkit/repository/`，**不是**執行階段依賴。劇情只儲存穩定引用：

```text
user:<namespace>.<content_id>     例如 user:mohui.boss_theme
```

官方 ID（`普通_001`、`brother4`）保持原樣，不改成 `official:`。

包內結構（僅打包實際引用）：

```text
assets/user/audio/mohui.boss_theme/content.json
assets/user/audio/mohui.boss_theme/boss_theme.ogg
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

對白語音仍是 `type=audio`，可另加可選管理欄位 `character`（`user:mohui.luoxue` 或官方人物 id，如 `player`）。沒有該欄位的舊音訊繼續合法。`character` 不改變 `say.voice` 播放協議，也不導致未引用音訊被打包。

自訂角色 `content.json` 還可選：`title`（對話短稱號）、`scale`（體型 50–130，預設 100，腳底對齊）、`art_facing`（原圖朝向 `left` 預設 / `right`）。缺省與舊包按 100 / 朝左處理。

- `type`：`audio` / `character`。
- `audio_kind`：`music` / `sound` / `env`。
- `character`（僅音訊、可選）：使用者角色引用或官方人物 id；省略表示旁白/系統/未關聯。
- 內容 ID：`[a-z][a-z0-9_]{0,31}.[a-z0-9][a-z0-9_]{0,47}`，禁止 `..`、`/`、`\`、`:`。
- 缺失、類型不匹配、metadata 損壞、檔案不存在、副檔名不支援、超過 20MB：pack 直接失敗，不得 silently skip。
- Python 側唯一解析入口：`compiler/lomc/content.py`。C# 側契約實作：`ContentRef.cs` + `ModLoader`。

使用說明見 `user_content.md`。
