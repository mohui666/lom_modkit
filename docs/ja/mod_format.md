# 活俠傳 Mod パッケージ形式（v3 契約）

> 言語：[简体中文](../zh_CN/mod_format.md) · [繁體中文](../zh_TW/mod_format.md) · 日本語（本文） · [한국어](../ko/mod_format.md)

**すべてのコンポーネント（エディター / コンパイラー / ランタイムプラグイン）は本ドキュメントを正とします。** 変更時は本ドキュメントも同期して更新してください。
文中のルールに対応する公式スクリプト／逆コンパイルの実証資料は `../research/` を参照。本文では繰り返し展開しません。

## 1. パッケージ構造

`.lommod` ファイル = zip 圧縮パッケージ。内部構造：

```
manifest.json          # 必填，包元信息
story/<id>.json        # 必填≥1，剧情源文件（编辑器可编辑的源格式）
lua/<id>.lua           # 必填≥1，编译产物（运行时只读这里）；每个 story/<id>.json 对应一个
texts.json             # 必填，已读文本表：{MOD_<modid>_<scriptid>_<nodeid>: 文本}（say 节点文本）
package-content.sha256 # 必須。圧縮に依存しない論理コンテンツ SHA-256
assets/                # 可选，自定义资源
                       #   图片：结局插图 / 人物介绍图 PNG/JPG
                       #   用户音频：assets/user/audio/<content_id>/
```

- `<id>` のルール：`[a-zA-Z0-9_\-]+`。パッケージ内で一意。すなわち「シナリオスクリプト id」。
- エクスポート（パッケージング）時は必ず再コンパイルします：story/*.json → lua/*.lua。両者は同名です。
- ランタイムプラグインが読むのは **manifest.json、lua/ ディレクトリと assets/ のみ**です。story/*.json はエディターが再読込／再編集するためのものです。コンパイラーはシナリオが明示的に参照する PNG/JPG（1 枚 ≤8MB）と明示的に参照される `user:` 音声のみを同梱します。エクスポートされた `.lommod` は自己完結しており、プレイヤーのマシンにエディターのリポジトリは不要です。
- texts.json はパッケージング時に自動生成されます：各 story の全 **say** ノードのテキストを収集し、key は lua 内の `GetStoryText` の key と一対一で対応します。ランタイムで LeanLocalization に登録されます（§4/§6 参照）。**death テキストは texts.json に入りません**：codegen が `mod_set_death_text(<タイトル>, <テキスト>)` の 2 引数 lua_str リテラルとして出力します（§3.1/§6 参照）。
- エントリー、JSON、Lua、ZIP の時刻／権限を固定し、同一 Python/zlib ツールチェーンでは同一入力をバイト単位で再現します。`package-content.sha256` は圧縮結果に依存しない論理内容ハッシュです。異なるツールチェーン間の完全な reproducible build は保証せず、このハッシュも署名や公式認証ではありません。
- エディターの「ファイル → Mod パッケージを検査」は Manifest、Story、Lua、Texts、アセット、ユーザーコンテンツ、サイズ、各 SHA-256 を読み取り専用で表示し、互換性、形式、論理ハッシュ、参照差分を検査します。Lua の実行、ディスクへの展開、内容のインポートは行いません。

## 2. manifest.json

```json
{
  "format": 1,
  "package_format": 1,
  "story_schema": 1,
  "content_schema": 1,
  "min_host_version": "0.6.0",
  "tested_host_version": "0.6.0",
  "tested_game_version": "1.2.3",
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

`package_format` / `story_schema` / `content_schema` は、それぞれパッケージ、Story、ユーザーコンテンツの明示的な形式バージョンで、現在はすべて `1` 固定です。`format: 1` は旧 reader 互換用です。未知のバージョンや矛盾する宣言はエディター、コンパイラー、Runtime のすべてで拒否されます。

旧 v1 の Story／ユーザーコンテンツは、元のバイト列を `*.pre-migration-v1.bak` に保存してから、同一ディレクトリ内で原子的に移行します。検証・バックアップ・置換に失敗した場合、元ファイルは変更されません。未知フィールドは保持され、`migration.restore_migration_backup` で明示的に復元できます。旧 `.lommod` のインポートはメモリ上のコピーだけを移行し、元パッケージを変更しません。

- `format`：固定で `1`。
- `id`：mod の一意な id（`[a-z0-9_\-]+`）。ランタイムの登録名プレフィックスとして衝突を防ぎます。
- `entry`：エントリーのシナリオスクリプト id。必ず存在すること。
- `min_host_version` は SemVer のハード要件、`tested_host_version` を現在の Host が超える場合は警告のみです。`game_version` は Unity の実際の `Application.version` と完全一致するハード要件、`tested_game_version` は不一致時の警告です。4 項目はすべて任意で、未指定の旧 manifest は従来どおり動作します。
- `campaign`（任意）：キャンペーンモード。
  - `new_game`：true のとき、ゲーム内 mod メニューの「新しいキャンペーンを開始」区に表示され、クリックすると**分離セーブスロット**（`SetSlot("mod_<modid>")`。プレイヤーの通常セーブを上書きしない）で新規ゲームを開始し、最初のシナリオスクリプトを本 mod の `entry` に置き換えます。
  - `disable_official_events`（任意、bool、既定 false）：true のとき本キャンペーンは**公式シナリオイベントを無効化**します——Free に戻っても場所なしメイン／サブイベントを自動開始せず、マップ地点には本 mod のトリガーのみが残ります（未命中の場合その地点の既定アクティビティは使用不可。mod 側でフォールバックトリガーを用意する必要があります）。
  - `triggers`：フリーモードトリガーの配列。`type="position"`：マップ地点 `position`（PositionType 列挙 id：Mall/Center/Alchemy/Forge/BackMountain/Room1/Door/Study/Kitchen/Room2/Secret）をクリックしたとき、その地点の既定アクティビティスクリプトを `script`（同一パッケージ内のスクリプト id）に置き換えます。任意条件はすべて命中した場合のみ有効（複数条件は AND。**配列の順序＝優先度**。ランタイムは最初に全条件命中したトリガーを採用）：
    - `when_flag_set` / `when_flag_clear`：シナリオ flag（`flag` ノードの AddStory の key、セーブに永続化）が設定済み／未設定。
    - `when_month`：整数 1〜12。その月のみ有効。
    - `when_stage`：整数 1〜3（旬：上／中／下）。その旬のみ有効。
    - `when_affinity`：`{"character": <人物 id>, "min": <整数>}`。好感度 ≥ min。
  - 既定では公式メイン／サブが優先。`disable_official_events` または F7 の一時スイッチが有効なときは公式クエスト判定をスキップし、mod トリガーを優先マッチします。
  - **トリガーはキャンペーン単位で分離**：アクティブな mod キャンペーンがあるときは現在のキャンペーン mod のトリガーのみをマッチ。キャンペーンがないときは全 mod がマッチに参加し、先に読み込まれたものが優先（読み込み順＝ファイル名順）。
  - トリガー例（練功場：好感イベント > 下旬の夜練 > 既定の散策）：

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

## 3. story/*.json — シナリオスクリプト形式

```json
{
  "story_schema": 1,
  "id": "main",
  "title": "显示给玩家的标题",
  "mood": false,
  "start": "n1",
  "nodes": [ ... ]
}
```

- `mood`（任意、bool、既定 false）：気分バブルのスイッチ。false=各 show ノード末尾と各 say ノードの前後に `mod_hide_mood()` を出力（公式の丸い感情パネルを非表示）。true=公式の気分バブルを残します。
- `nodes` はノード配列。既定では配列順に順次実行します（次ノードへの暗黙の goto）。
- 各ノードは一意な `id` を持ち、任意のノードで明示的に `"goto": "<nodeId>"` を書いて順次フローを上書きできます。
- `choice` / `branch` / `dice` の分岐は必ず `goto` で対象ノード id を指します。
- 複数の先行ノードが同一ノードに合流（合流点）するのは合法です。

### 3.1 ノードタイプ（全 47 種）

この表が現在の合法ノードすべてです。`combat` は原作 Combat テンプレートを使う高レベル編成です。`battle`、`reward`、`quest_*` はまだノードではなく、戦闘機能は逆コンパイル確認済み原作 API だけを呼びます。

**演出系**

| type | フィールド | 説明 |
| --- | --- | --- |
| `music` | `name`；任意 `op`("play"既定/"stop"/"fadeout")、fadeout 時は `seconds`(既定2) | 公式名に従い `PlayMusic` / `StopMusic` / `FadeOutMusic(seconds)` の後に **`wait(seconds)`**。`name` が `user:` で始まる場合はユーザーコンテンツ参照（§8）で、ランタイムが**現在のパッケージ**の `assets/user/` から解決して再生します。ローカル絶対パスは禁止 |
| `sound` | `name`；任意 `kind`("sound"既定/"env")、`op`("play"既定/"fadeout"は env のみ、`seconds`既定1) | 公式名に従い `PlaySound` / `PlayEnvSound` / `FadeOutEnvSound(seconds)` の後に同様に **`wait(seconds)`**。`user:` 参照ルールは music と同じで、`audio_kind` がノードと一致している必要があります |
| `scene` | `view` | シーン切替：`runblock(flowcharts.view,"out")` の後 `ViewName=view; runblock(...,"view")`。`view="out"` はフェードアウトのみ。`"black"/"white"` は単色。単色以外の view は先に `runwait(flowcharts.LoadView(view))` で背景アセットをプリロードします（プリロードしないと背景が真っ黒になります） |
| `background` | `action`(`set`/`show`/`replace`/`fadein`/`fadeout`/`clear`)。表示系は `image`(`user:` 画像)必須、`fade`(既定0.5)任意 | 現在のパッケージ内ユーザー画像をカスタム背景として表示。章・公式 `scene`・シーン切替・再読込時に自動消去し、原版 View 資源は変更しません |
| `custom_cg` | `action`(`show`/`hide`)。show は `image` 必須、`fade`・`scale`・`x/y` 任意 | 人物レイヤー前にユーザー画像 CG を表示。拡大率と中心位置を調整でき、hide・章・シーン切替時に消去します。公式 `cg` は変更しません |
| `overlay` | `action`(`show`/`hide`) と `slot` 必須。show は `image` 必須。`position`・`scale`・`opacity`・`layer`・`fade` 任意 | 複数スロットの前景・小道具・挿絵・マスク。同じスロットは置換でき、章・シーン切替時に消去します |
| `show` | `character`, `position`；任意 `portrait`(既定normal), `facing`(既定right), `fadeDuration`(0), `moveDuration`(0) | 人物を読み込んで表示。story.mood が false のとき末尾（Focus の後）に `mod_hide_mood()` を追加 |
| `move` | `character`, `from`, `to`；任意 `duration`(既定1) | 移動して `wait(duration)` |
| `face` | `character`, `facing` | 向き変更 |
| `hide` | `character`；任意 `fadeDuration`(既定0) | 人物を非表示 |
| `focus` | `character` | `characters.Focus` |
| `offset` | `character`, `x`, `y`, `duration` | 人物オフセット演出 `runwait(characters.MoveOffsetCoroutine(id,x,y,t))` |
| `say` | `text`；任意 `character`, `portrait`(既定normal), `mode`("character"既定/"think"/"narrative"/"center")、任意 `voice` | 会話／内心独白(os_mask 付き)／ナレーション／中央ナレーション。narrative と center は character を無視。**既読機構**：テキストは Lua に直接埋め込まず、`say(luamanager.GetStoryText("MOD_<modid>_<scriptid>_<nodeid>"))` を出力します（modid がない場合は "MOD" で代替）。テキスト本体は texts.json に入りランタイムが登録。**`voice`**（任意）：ユーザー音声参照。例：`user:mohui.line_01`。このセリフに入る前に `mod_play_voice`（前のセリフを先に停止）、`say()` 復帰後に `mod_stop_voice`。ボイスは独立チャンネルを通り、`sound` / `StopMusic` では止まりません。絶対パスと公式効果音名は禁止 |
| `choice` | `options`: `[{"text","goto"}]`（2〜4 項目）；任意 `dialog`(既定"Options"、スキンは §3.3) | 選択肢メニュー `choose()` |
| `shock` | `character`；任意 `duration`(既定0.5) | 人物シェイク（flowcharts.common "shock"） |
| `mask` | `show`(bool) | 独白マスク `os_mask.Show` |
| `intro` | 任意 `intro_source`(`official` 既定/`custom`)。official は `character` 必須。custom は `name`,`text` 必須、任意 `title`,`image`（パッケージ内 `assets/` の PNG/JPG、≤8MB）、`image_scale`(40〜160、既定100)、`image_x`/`image_y`(-30〜30、既定0) | official は原版 `runwait(intropanel.Show(character))` を呼び出し。custom は `mod_prepare_character_intro(title,name,text,image,scale,x,y)` を呼び出し、同じ CharacterIntroPanel を再利用。画像は画面セーフエリアに独立レイアウトされアスペクト比を保持。x 正数は右、y 正数は上。画像なしの場合はポートレート領域を非表示 |
| `effect` | `name`；任意 `x`,`y`,`a`,`b`,`c`(数値、既定0/0/1/1/1)、`play`(bool、既定true) | 画面エフェクト `effects.SetupEffect(name,x,y,a,b,c,play)`。例：Hit_001/Blood_002/Sword_001。`play=false` は停止呼び出しを出力（末尾引数 0）：**ループ系エフェクトは自動消滅しません**（EventBubble/Glow など）。必ず後続に play=false の同引数ノードで停止すること。さもないと画面に残り続けます（旧データの `d` フィールドは互換維持：play がない場合は d を使用） |
| `transition` | `phase`("in"/"out")；任意 `dir`(既定"lr"、lr/rl/tb/bt) | 暗転トランジション `runwait(transitionblack.TransitionIn/Out(dir))`。**必ずペアで使用**：TransitionIn はシナリオ UI を隠して画面を黒幕で覆い、TransitionOut で初めて復帰します。in があって out がない場合はコンパイラーが警告します（画面が黒いままになります） |
| `camera` | `name`, `active`(bool) | カメラフィルター `maincamera.ActiveVolume(name, 0 | 1)`。例：stage-memory/stage-dream/stage-fire/stage-blurdim |
| `block` | `flowchart`("view"/"common"), `name`；任意 `vars`: `[{"name","value"}]` | 汎用 flowchart ブロック呼び出し：`getvar` で順に代入してから `runblock(fc, name)`。out_white/shake/flash/vshock などをカバー |
| `cg` | `action`("show"/"hide"), `kind`("picture"/"item"/"big"/"map"/"family"/"title")；任意 `key`, `key2`, `n1`, `n2` | mainui の画像／マップ／家系図／タイトル：`ShowPicture(key)`/`HidePicture`/`ShowItemPicture`/`ShowBigPicture`/`ShowMap(key,key2)`/`ShowFamilyTree(key,key2,n1,n2)`/`DisplayTitle(key)` など |
| `dim` | `character`, `dimmed`(bool 必須、既定 true) | 人物を暗くする `stage.SetDimmed(character, dimmedState)`（実引数は character が先、bool が後。dimmed=true のとき公式実装はそのキャラの気分バブルも非表示にします） |
| `message` | `text`（必須・非空、複数行可） | システムメッセージ `mainui.DisplayMessageText(text)` は**原文**を表示します（DisplayMessage はローカライズ key 解決を通るため、Text 版を使ってカスタムテキストが key として空引きされるのを回避） |
| `rotate` | `character`, `angle`(int 必須、既定 180), `duration`(float 必須、既定 1、>0) | 人物回転 `characters.Rotate(key, angle, duration)`——**公式の引数順は angle が先、duration が後** |
| `dayenv` | `day_type`（int 必須、1=昼 / 2=夜） | 昼夜環境 `luamanager.SetGameDayEnvironment(day_type)`。**フィールド名は day_type**：ノード共通キー "type" との衝突を避けるため |

**数値／状態系**

| type | フィールド | 説明 |
| --- | --- | --- |
| `stat` | `key`, `delta`；任意 `waitDisplay`(既定true), `display`(既定1), `mode`(既定"") | 主人公の属性増減 `statmodifymanager.Player(key, delta, mode, display)` |
| `stat_set` | `key`, `value`；任意 `update`(bool既定false) | 絶対設定 `SetPlayer(key, value)`。update=true は `UpdateSetPlayerStat` を使用（title など用） |
| `affinity` | `character`, `delta` | 人物好感度 `statmodifymanager.Character(character, delta, 1)` |
| `talent` | `talent`, `level`(±1) | 天賦 `statmodifymanager.AddTalent(id, level)` |
| `item` | `kind`("book"/"misc"/"special"), `item`, `count`(既定1)；任意 `remove`(bool既定false) | アイテム増減 `AddBook/AddMisc/AddSpecial(id,count)`。remove 時は `RemoveBook/RemoveMisc(id)`（book/misc のみ） |
| `flag` | `flag` | mod シナリオ flag：`statmodifymanager.AddStory(flag)` + `modflags[flag]=true` |
| `game_flag` | `flag`, `value`；任意 `op`("set"既定/"add") | 公式クエスト flag：`SetFlag(id, 状態)` / `AddFlag(id, ±増分)`。**id はゲーム既存の FlagData でなければなりません**（14_属性とFlag 表）。さもないとゲームが黙って無視します |
| `enemy` | `op`("team"/"level"/"people"/"id"), `enemy`, `value`(数値、id の op は不要), `display`(既定1) | 敵パーティー変更 `ModifyEnemyTeam/Level/People/Id` |
| `battle_skill` | `op`("set"/"active"/"reset"), `key`(reset は不要), `index`(set 用、既定2), `active`(active 用、既定1) | 戦場スキル `SetPlayerBattleSkill/SetBattleSkillActive/ResetBattleSkill` |
| `combat` | `key`(原作 Combat id), `win`, `lose`(ノード id)。任意 `enemy`, `team`, `level`, `people`, `display` | 原作の敵設定を組み合わせて Combat へ入り、Host が `CombatManager.GameOver(bool)` の win/lose を指定ノードへ戻します。draw/escape と追加 goto は非対応 |
| `mission` | `name`, `key` | クエスト操作 `statmodifymanager.Mission(name, key)`：`Mission("Main","M0001")` でメイン進行 / `Mission("S2200","clear")` でサブクリア |
| `time` | `op`("set"/"round"/"month"/"mission")；set は `year,month,stage`；mission は `name,year,month,stage` | 時間 `SetGameTime/NextRound/NextMonth/SetMissionTime` |
| `autosave` | 任意 `kind`("story"既定/"free"/"prologue")；任意 `save_button`(0/1、セーブボタンを個別制御) | `AutoSave()/AutoFreeSave()/PrologueSave(mode)`。`save_button` は単独で `ToggleSaveButton(n)` を emit |

**フロー系**

| type | フィールド | 説明 |
| --- | --- | --- |
| `branch` | `cases`(≥1)；任意 `source`("mod"既定/"game"/"stat"/"flag_value"/"condition")。キーフィールド：source=stat のとき `stat`（属性 id、editor_data の stats 一覧）、それ以外のソースは `flag`（非空） | 条件分岐、5 ソース：mod=modflags が設定済みかどうか。game=公式チェックポイント `checkpointmanager.Switch(flag)`。stat=主人公属性 `luamanager.GetStatData(stat, 1)`。flag_value=公式クエスト旗標 `tonumber(luamanager.GetFlagData(flag))`。condition=公式条件チェックポイント `checkpointmanager.Condition(flag)`（bool）。case 構造はソース別：mod/condition は `[{"value","goto"}]`（value は 1/2 のみ：mod=設定済み/未設定、condition=真/偽）。game は `[{"value","goto"}]`（任意の整数）。stat/flag_value は `[{"op","value","goto"}]`（op 既定 ">="、>=/>/<=/</== を許可）。未命中は一律 else で順次の次ノードへフォールバック（末ノードで全値を網羅していない → LomcError。mod/condition は 2 case 揃っていれば網羅） |
| `dice` | `check`, `options`: `[{"goto_大成功","goto_成功","goto_失败","band_texts"?}]`（ちょうど 1 件） | ダイス判定。**check は公式メタデータ付きのチェックポイントでなければなりません**（editor_data の dice_meta：ダイス範囲 max と結果バンド bands。メタデータなしのチェックポイントはゲーム内ダイスメニューを NRE クラッシュさせます）。公式の 5 ステップチェーンを出力し、結果バンド数に応じてバンドごとに選択肢（テキスト+条件）を出力。分岐はバンド品質順位でマッピング：最悪バンド→goto_失败、中間バンド→goto_成功、最良バンド→3 バンド以上は goto_大成功 / 2 バンドは goto_成功。バンド品質は条件数値から推定（同値では >系が <系より上位）。**band_texts**（任意）：バンドごとにダイスメニュー選択肢テキストを上書き（件数=結果バンド数、各項目非空。でなければ LomcError）。`<作者テキスト> \| <公式cond>` を出力（作者テキストはリテラルで texts.json に入らない。ASCII \| は全角｜にサニタイズ。cond は常に公式メタデータ）。省略時は公式結果バンドテキスト |
| `goto_scene` | `scene`("Free"/"Title"/"Combat"/"Battle"/"GameOver"/"End"/"Story"/"DemoEnd")；任意 `key`(Combat=戦闘id/Battle=戦役id/GameOver=死亡画面id/End=結末識別子), `next`, `title`, `desc`（すべて str、End/GameOver のみ使用）, `image`(str、**End のみ**：パッケージ内画像の相対パス、例 `assets/ending.png`) | 通常シーンは従来どおり `luamanager.ChangeScene(scene,key,next)`。**End 特例は原版の汗青書フローに従う**：カスタムタイトル／本文／挿絵をキャッシュ → `runwait(endgamepanel.Open("__MORTAL_MOD_END__"))` → プレイヤー確認 → 暗幕 → Title。ランタイムが本物の `EndGamePanel` を patch し、公式版式を完全に再利用。`image` は左ページ `_picImage` に書き込み。空の場合は原版結末 20047 の Picture を借用してプレースホルダー。画像の欠損／破損は警告のみでプレースホルダーにフォールバック。End/GameOver の next は無効（原版ボタンはロード／タイトルに固定）。旧値は無視して警告（旧互換値 Story は Title として処理、警告なし）。カスタムコンテンツなしで公式 key を指定した場合のみ公式結末エントリを直接開きます（原版どおりアンロック／記録し警告を出す）。mod 専用 End key で title/desc/image がない、mod 専用 GameOver key で title/desc がない場合は検証失敗とし、空白カードを回避します |
| `panel` | `panel`("martial"/"weapon"/"poison"/"cg"/"cgvideo"/"shop"/"newshop"/"credit"/"endgame")；任意 `key`(cg/cgvideo/endgame の id), `discount`(shop 用、既定0), `mode`(martial 用、既定0) | システムパネルを開く。newshop 以外はすべて `runwait`：`martialpanel.Open(mode)`/`weaponupgradepanel.Open()`/`poisonupgradepanel.Open()`/`cgpanel.Open(key)`/`cgvideopanel.Open(key,0)`/`shoppanel.Open(discount)`/`shoppanel.NewShop()`/`creditpanel.Open()`/`endgamepanel.Open(key)` |
| `wait` | `seconds` | `wait(seconds)` |
| `end` | 任意 `next_script` | あり：`SetNextScript("MOD_<modid>_<id>")`+`Init()` で同一パッケージのスクリプトにチェーン。なし：`ChangeScene("Free","","")` でフリーモードに復帰 |
| `death` | `text`（必須・非空、複数行可）、`death_id`（必須）；任意 `title`（str、既定「勝敗乃兵家常事」）、旧フィールド `next` | **死亡テキスト**：暗転（view="black"）→ `mod_set_death_text(title, text)`（2 引数 lua_str リテラル、**texts.json／既読システムには入らない**）→ `luamanager.ChangeScene("GameOver", death_id, "Title")` で**公式 GameOver 死亡画面**へ（黒地に赤文字 + ロード／タイトルボタン、§6 参照）。原版はカスタム next を読みません。旧値は無視して警告。`death_id` は ≥900000 の mod 専用数値 id でなければなりません（でなければ LomcError。「死亡／結末 id 規約」参照）。終端ノード（独自の遷移を持ち、明示的 goto は不可。末ノードとして締められます） |
| `raw` | `code` | 生 Lua エスケープハッチ：コードをそのまま挿入（複数行可）。**機構のフォールバック**：どのノードでも表現できない公式機構はこれを使います |

### 3.2 よく使う値（data/editor_data.json が権威一覧。schema 2 以降は中文名付き）

- 立ち位置 position：`SL L1 L2 M R1 R2 RM2 SR …`（全 36 個。S=画面外 L=左 M=中央 R=右 B=後 C=央）
- 表情 portrait：`normal nervous1..3 angry1 angry2 laugh1 gloomy2 …`（人物設定による。欠落時はゲームが最初の立ち絵にフォールバック）
- say mode：`character` 会話 / `think` 内心独白 / `narrative` ナレーション / `center` 中央ナレーション
- stat key：`mental(心相) money(銀両) disposition behaviour karma fame talking team …`（31 個）

### 3.3 選択肢メニュースキン（choice.dialog）

**`Options` のみ使用可能**（既定、プレーンテキスト選択肢。Dice はダイスノード内部専用）。その他のスキン（Talk/Meet/Door/Section_* など）はフリーシーンの break 形式メニュー（選択肢テキストは `タイプ+key+行動ポイント+貢献` の 4 段 `+` 区切り）で、プレーンテキスト選択肢は `BreakOptionButton.UpdateContent` の IndexOutOfRange クラッシュ（メニューフリーズ）を引き起こします——コンパイラーは直接エラーで拒否します。出力：`setmenudialog(menudialogs.Options)` → `choose()` → `menudialogs.Options.SetActive(false)`。

## 4. story.json → Lua コンパイル規約（lomc 実装）

- 各ノードは 1 つの Lua 関数にコンパイルされます。ファイル先頭で `local node_n1, node_n2, ...` を前方宣言し、続いて `node_nX = function() ... end`。遷移は末尾呼び出し `return node_<goto>()`。トップレベルは `return node_<start>()`。
- テキストのエスケープ：`\`→`\\`、`"`→`\"`、改行→`\n`、`\r`→`\r`。
- 各スクリプト冒頭で `modflags = modflags or {}` を emit（グローバルテーブル。Story シーンのセッション内で持続し、チェーンスクリプト間で共有。セーブには含まれない）。直後に 1 行 `mod_set_mood(true|false)`（story トップレベルの mood 宣言、既定 false。§6 参照）。
- `flag` ノードは二重 emit：`AddStory` + `modflags[flag]=true`。
- **分岐フォールバック**：choice 以外の多岐構造は暗黙の空振りを許しません——case 未命中時は else で順次の次ノードへ。フォールバック不能（branch が末ノードで全戻り値を網羅していない）は検証エラーとみなします。
- ノード id の文字集合 `[a-zA-Z0-9_]+`（スクリプト id は `-` を許可）。
- story トップレベルの `title` は任意。
- **既読 key ルール**：すべての say（character/think/narrative/center）ノードのテキストは一律 `say(luamanager.GetStoryText(key))` を出力し、key = `MOD_<modid>_<scriptid>_<nodeid>`。modid は manifest 由来（パッケージング時）。単独 build／エディタープレビューで欠ける場合は "MOD" で代替。**death テキストは既読 key を通りません**：`mod_set_death_text(<タイトルリテラル>, <テキストリテラル>)` を出力（タイトル既定／空文字列は「勝敗乃兵家常事」）。テキストは texts.json に入りません。
- **結末／死亡カードルール**：goto_scene で scene=End かつ title/desc/image 付きのとき、先に `mod_set_ending_text(...)` を出力してから原版汗青書フローで表示。image は左ページの挿絵であり、全画面背景ではありません。scene=GameOver で title/desc 付きのときは `mod_set_death_text(<title>, <desc>)` に切り替え。death ノードも同様に `mod_set_death_text(<title>, <text>)` を出力（2 引数。1 引数の旧パッケージ互換はランタイムが引き続きサポート）。この 2 つのグローバル呼び出しはランタイムプラグインが登録します（§6）。
- **mood ルール**：story.mood=false のとき、show ノード末尾（Focus の後）と say ノードの say(...) 前後にそれぞれ 1 回ずつ `mod_hide_mood()` を出力。true のときは出力しません。
- **death 出力**：§3.1 の death 行を参照（runblock out → ViewName="black" → runblock view → `mod_set_death_text(title, text)` → ChangeScene("GameOver", death_id, next)）。
- 最後のノードが `end`/`death`/`goto_scene`/`raw` のいずれでもなく goto もない → 検証エラー。
- `choice`/`branch`/`dice`/`end`/`death`/`goto_scene` に明示的 `goto` を書く → 検証エラー。
- `say` の narrative/center モードで character を与えるのは許可されますが無視されます。
- `raw` ノードの内容はそのまま挿入（コンパイラーは構文チェックしません）。その後の遷移は通常どおり（順次/goto）。
- **非致命警告**：`-- lomc 警告：` コメント形式で Lua ヘッダーに挿入（transition で in があって out がない場合など）。`lomc check` でも stderr に同時出力されます。

主要 API のパターン：

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

## 5. data/editor_data.json — エディターデータ契約（schema 3）

`tools/extract_editor_data.py` が生成します。schema 2 以降、`characters`/`stats`/`positions`/`views`/`music`/`free_positions` はすべて `{id, name}` オブジェクト配列（characters はさらに portraits を持つ）。schema 3 では `dice_meta`（ダイスチェックポイントのメタデータ：`{check: {max, bands: [{text, cond}]}}`。bands は公式の表示順）と `death_ids`/`ending_ids` のリッチ化オブジェクト配列（name は `data/ref/death_ending_ids.json` 由来。後述の「死亡／結末 id 規約」参照）が追加されました。**dice_meta はストーリーシーンのチェックポイントのみを含みます**：旅行システムのチェックポイント（Travel_*）はストーリーシーンの CheckPointManager で見つからずクラッシュするため、抽出時に除外済みです。`dice_checks` は全件名一覧で、すべての呼び出し箇所（旅行を含む）を保持します：

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

### 死亡／結末 id 規約（mod 専用区間）

公式 GameOver/EndGamePanel は id で LibrarySystem を参照し、LibraryItemData.Add()（公式結末のアンロック／記録）を実行する可能性があります。カスタム End は存在しない内部 key を固定使用し、公式結末を参照／書き込みしません。「カスタムコンテンツなしで公式 End key を直接開く」場合のみ原版どおり記録されます。

- **mod の死亡／結末 id = `9<公式id>`**（900000 区間）：公式死亡 10021 → mod 910021。公式結末 20003 → mod 920003。公式の 1xxxx（死亡）/2xxxx（結末）/4xxxx（後日談）の全区间と衝突しません。
- 自作 GameOver id は公式エントリに見つからない → 副作用なし。テキストはプラグインが注入。自作 End id は mod 内識別子としてのみ使われ、実際の表示は固定内部 key を通ります。
- `death` ノードの `death_id` は ≥900000 の整数と検証されます。空白の自作 GameOver/End カードはコンパイラーが拒否します。カスタムコンテンツなしで公式 key を直接使用した場合は非致命のセーブ汚染警告を出します。
- 権威参照：`data/ref/death_ending_ids.json`：`death` 106 件（10000〜10104、11000）、`ending` 54 件（20000〜20053）、`epilogue` 4 件（40000〜40003）。抽出器はそのタイトルで editor_data の death_ids/ending_ids をリッチ化します。エディターの death_id 入力欄には公式参考の先頭 5 件が表示されます。

## 6. ランタイムプラグインの動作（MortalModHost）

1. 起動時に `BepInEx/plugins/MortalModHost/mods/*.lommod` をスキャンし、`MOD_<modid>_<scriptid>` → lua テキストを登録します。
2. Harmony prefix `LuaManager.ExecuteLuaScript()`：登録名に命中した場合は mod の lua で実行し、元メソッドをスキップします。
3. 入口：Free フリーシーンと Title タイトル画面左下の「活侠MOD」ボタン + F8（設定可）でメニューを開きます。Free メニューは「mod シナリオを演出」と「新しいキャンペーンを開始」の 2 区。Title メニューは「新しいキャンペーンを開始」区のみ（シナリオ演出には読み込み済みセーブのプレイヤー状態が必要なため Free でのみ提供）。
4. **キャンペーン**：「新しいキャンペーンを開始」クリック → `SetSlot("mod_<modid>")`（分離セーブスロット）→ 公式 `NewGameData()` → postfix が最初のシナリオスクリプトをその mod の entry に置き換え → LoadStory。
5. **原版シナリオ抑制と位置トリガー**：`disable_official_events` または F7 が有効なとき、`UpdateCheckMissions` 内でメインのトリガー状態を一時的に隠し、`HasAnyMissionTrigger` を false にして、Free 復帰時に公式メイン／サブが自動開始するのを防ぎます。地点クリックの postfix `FreePositionData.GetExecuteScript` は manifest.triggers を優先マッチし、mod の命中がない場合は公式地点の既定スクリプトを抑制します。
6. **フォールバック**：Story シーンが要求した MOD_ スクリプトが未登録（mod が削除された）の場合、実行せず `ChangeScene("Free","","")` でソフトロックを防ぎます。
7. mod は公式スクリプトとテキスト表を変更しません。mod の flag は StoryKeyList に入り、セーブ互換です。
8. **texts.json 登録**：.lommod 読み込み時に texts.json の key→テキストを LeanLocalization（`Story/`+key）に登録。`GetStoryText` は key で既読システムを参照：既読→黄色+スキップ可、未読→通常色+既読に記録、見つからない場合は key 自体を返します。
9. **mod_hide_mood**：グローバル Lua 関数 `mod_hide_mood()`（引数なし）を登録し、全キャラの丸い感情パネル（CharacterMoodPanel）を非表示にします。コンパイラーは story.mood スイッチに従って show/say 箇所に出力します（§4 参照）。
10. **mod_set_mood**：グローバル Lua 関数 `mod_set_mood(bool)` を登録し、スクリプト冒頭の宣言どおり公式気分パネルのスイッチ（ShowMood）を強制制御します。各 mod スクリプト入口で 1 回出力され、チェーンスクリプトではスクリプトごとに切り替わります。
11. **UpdateTranslations の wipe 防止**：公式のテキスト更新はプラグインが登録した mod テキストを消去するため、hook して更新後に texts.json の登録を再生する必要があります（読み込み時に全登録項目をキャッシュ）。これで mod key が失効しないことを保証します。
12. **人物紹介カード**：公式人物は従来どおり `CharacterIntroPanel.Show(key)` の動作。カスタム人物は特殊 key 上で Harmony が引き継ぎ、公式パネル版式を再利用してカスタム称号／氏名／本文を書き込みます。任意の `image` は現在の `.lommod` の `assets/` からデコードして独立したセーフレイアウトに配置：既定は画面中央 `(31%,50%)`、最大幅／高さは画面の `(30%,62%)` でアスペクト比保持。`image_scale` は自動適合サイズの上でスケール、`image_x/image_y` は画面パーセントで微調整。閉じるときに一時テクスチャを破棄し、原版コントロールを完全に復元します。画像なしのときはポートレート領域を非表示。公式のローカライズ表や関係データは変更しません。
13. **結末／死亡カード描画**：2 つのグローバル Lua 関数を登録（§3.1/§4 参照）：
    - `mod_set_death_text(title, desc)`：死亡タイトル／説明をキャッシュ。Harmony postfix の `GameOverController` が 2 段のテキストを公式 `_titleText`/`_descTextPrefab` コントローラーに書き込み、公式レイアウトで死亡画面中央に表示します。1 引数呼び出しは旧契約どおり desc として扱い、タイトルは空のまま（旧パッケージ互換）。
    - `mod_set_ending_text(title, desc[, image])`：結末タイトル／説明と任意のパッケージ内画像をキャッシュ。Harmony postfix が `EndGamePanel.Open` をラップし、公式の最初のキャンバス fade 前に `_titleText/_descText` と左ページ `_picImage` を書き込みます。画像未指定時は公式結末 20047 の Picture を借用してプレースホルダー。公式のフェードイン、確認待ち、フェードアウトはすべて保持。表示中は `_saveLibrary` を一時的に閉じて mod key が汗青書セーブスロットに入るのを防ぎ、終了後に復元します。
    - 新コンパイラーのカスタム End は簡易 `EndGameController` シーンに入りません。旧パッケージは従来の End シーン上書き互換を保持します。GameOver 自作 id で文字なし、End 自作 id で内容なしはいずれもコンパイル時に阻止されます。
14. **エディター単発試遊プロトコル**：エディターはエントリーチャプターの `start` を現在選択中のノードに一時変更し、固定パッケージ `__lom_modkit_preview.lommod`（manifest id `lom_modkit_preview`）としてインストールした後、プラグインディレクトリに `preview-request.json` をアトミックに書き込みます。ランタイムは 0.35 秒ごとにチェック：Free シーンでは直接演出、Title シーンでは `mod_lom_modkit_preview` 分離スロットで開始、その他のシーンでは安全なシーンまで待機。消費後にリクエストと一時パッケージを削除します。リクエストは format=1 と `[A-Za-z0-9_-]+` の mod/script/node id のみ受け付け、正式な Mod パッケージは自動削除の対象外です。
15. **mod 新キャンペーンに運命 2 ポイント付与**：公式の新規ゲームは初期に運命ポイントを持ちますが、mod 分離セーブの初期値は 0 で、ダイス「逆天」フロー（`DiceMenuDialog.CheckRevolution` は 命運>0 を要求）が mod キャンペーンで使用不可でした。NewGameData postfix は最初のスクリプト置き換え後に mod キャンペーンへ `GameStatType.命運` を 2 ポイント加算します。公式の新規ゲームには影響しません。
16. **mod シナリオでのダイス範囲変更の開放**：公式の「修改范围」ボタンは 2 周目かつ実績 30016 所持を要求します。mod シナリオ中（`CurrentStoryScript` が `MOD_` で始まる）は `get_NewGamePlus` prefix を true 返しにし、かつ `CheckRevolution` の元の戻り値が true のとき `_rangeButton` を直接アクティブにします（mod 内で公式実績 30016 をアンロックせず、公式セーブの汚染を回避）。公式シナリオにはまったく影響しません。
17. **ユーザー音声**：`LuaManager.PlayMusic/PlaySound/PlayEnvSound` の引数が `user:` で始まる場合はプラグインが引き継ぎ、**現在演出中の Mod パッケージ**の `UserContents` から解決（`assets/user/audio/<id>/content.json` + メインファイル）してデコード後に Windows `waveOut` で再生します（本ゲームのメインミックスは Wwise で、Unity `AudioSource` はしばしば無音）。公式名は一律そのまま原版 Wwise に渡します。ランタイムは `%APPDATA%/lom_modkit/repository` を読みません。2 つの Mod が同じ ID を持っていても自身のパッケージのみを解決します。対応フォーマットは `.ogg` / `.wav` のみ、1 件 ≤20MB。カスタム fadeout は出力音量のフェードアウト（その後もコンパイラー出力の `wait` は続きます）。カスタム音楽への切替時は先に公式 Wwise 音楽を停止します（公式 `StopMusic` は環境音も同時にクリアします）。
18. **セリフボイス**：`mod_play_voice(ref)` / `mod_stop_voice()` を登録。`mod_play_voice` は現在のボイスを先に停止してから再生（ループなし、独立 `_voice` チャンネル）。`sound` ノード、カスタム効果音、`StopMusic` はいずれもこのチャンネルに触れません。シナリオ中断、公式スクリプトへの切替、Mod の再読み込み時は `StopEverything()` がボイスを停止します。`voice` のない旧 Lua はこれらの関数を呼ばないため、動作は変わりません。
19. **ゲーム内 Mod メニューの多言語**：メニュー文案（`src/I18n.cs` に zh_CN/zh_TW/ja/ko の 4 言語カタログを内蔵）はゲームの現在の言語に従います——リフレクションで LeanLocalization の `CurrentLanguage` を読み、言語名を曖昧マッチ。公式ゲーム自体に日本語オプションはないため、日本語カタログが実際に発火することはありません。検出失敗時は一律 zh_CN にフォールバック。詳細は `i18n.md` を参照。

20. **構造化 Runtime エラー**：Mod 再生を fail-closed で中止する障害は、一行の `[mod-runtime-error]` JSON ログとして記録されます。固定項目は `mod_id`、`mod_name`、`version`、`story`、`node`、`category`、`error`、`recent_trace` と UTC 時刻です。通常 Mod は変数値を含まないノード/遷移 breadcrumb を最大 32 件だけ保持し、エラーには最大 16 件を長さ制限付きで添付します。F5 の完全な 256 件開発 trace は従来どおりです。例外整形、trace 取得、JSON 化、ログ出力自体が失敗しても最小レポートへ退避し、元の障害や安全な Free 復帰を妨げません。最後のレポートは診断バンドル用にメモリ保持します。

## 7. AI ツールインターフェース（story_api）

editor/story_api.py は AI／エディター共用の管理された書き込み入口です。ルール：**AI は story JSON や Lua を直接手書きしません**。
すべてのシナリオ構築は story_api 経由（models 契約の既定値 + lomc の検証／警告）で行い、ダイスメニュークラッシュ、
transition の黒幕、choice スキンクラッシュ、背景の黒画面、人物未登場での動作など既知の落とし穴を防ぎます。

- Python API：
  - `load_editor_data()`：エディターデータ（dice_meta などの一覧を含む）を読み、(editor_data, is_fallback) を返す
  - `new_story(story_id="main", title="新剧情", mood=False)`：新規シナリオスクリプト（show 登場 + 空 say の 2 ノード開場。先に登場させてから動作）
  - `add_node(story, node_type, fields=None, after=None)`：models 既定値でノードを追加（47 種）。未知のタイプ／フィールド／型不一致→ValueError。ノード id は自動生成。after で挿入位置を指定（ノード id または None=末尾）。登場防線：動作系ノードの対象人物がそれ以前に未登場／退場済みの場合、その前に show を自動挿入
  - `update_node(story, node_id, fields)`：ノードフィールドを更新（add と同じフィールド検証）。ノード不存在→ValueError。登場防線：更新後に動作人物が未登場／退場済みなら、そのノードの前に show を自動挿入し、それを指す goto／選択肢／分岐ジャンプを新ノードへ付け替え
  - `get_node(story, node_id)`：ノードを読む。不存在→ValueError
  - `list_nodes(story)`：[{"id","type","summary"}] の一覧を返す
  - `delete_node(story, node_id)`：ノードを削除。不存在→ValueError
  - `rename_node(story, node_id, new_id)`：ノード id を改名し、start と全ジャンプ参照（goto/選択肢/分岐/ダイス行き先）を同期。改名後のノードを返す。新 id は `[A-Za-z0-9_-]+` に限定。既存ノードと衝突→ValueError
  - `move_node(story, node_id, delta)`：相対移動量でノード順序を調整
  - `set_start(story, node_id)`：開始ノードを設定
  - `add_choice(story, options, after=None)`：選択肢分岐を追加（2〜4 項目、dialog は Options 固定）
  - `add_dice(story, check, goto_成功, goto_失败, goto_大成功="", band_texts=None, after=None)`：ダイス判定を追加（check は公式メタデータ必須、結果バンド数に応じて goto を検証。band_texts の件数は結果バンド数と一致し各項目非空）
  - `add_say(story, text, character=None, mode="character", portrait="normal", voice=None, after=None)`：セリフを追加（character モードは character 必須。narrative/center は character を書かない。voice は任意の user: 音声参照）
  - `add_death(story, text, death_id, next="Title", title=None, after=None)`：死亡テキストノードを追加（text 必須・非空・複数行可。death_id 必須、≥900000 の mod 専用数値 id。next は Title のみ。title は任意の短いタイトル、既定／空文字列は「勝敗乃兵家常事」）
  - `add_scene(story, view, after=None)`：シーン切替を追加
  - `check_story(story)`：検証のみ。(errors: list[str], warnings: list[str]) を返す
  - `compile_story(story)`：検証+コンパイル。(lua|None, errors, warnings) を返す。失敗時 lua は None
  - `load_story_json(path)` / `save_story_json(story, path)`：story.json の読み書き（UTF-8）
  - `pack_mod(mod_dir, output=None)`：manifest 検証 + 全件コンパイル + .lommod パッケージング。成果物パスを返す
- CLI：python editor/story_api.py check|compile|pack|new-story（AI サブプロセスに優しい。終了コード 0/1、中文エラーメッセージ）
- 重要な不変条件（コンパイラーが強制、API が透過）：choice.dialog は Options のみ。dice.check は公式メタデータ必須
  （ダイス範囲+結果バンド）。transition in/out はペア。scene は背景を自動プリロード。
  **show/say の (character, portrait) は data/editor_data.json のキャラ表情表内になければなりません**
  （表が利用不可／キャラが表にない → 通過。キャラが表にあるが表情がそのリストにない → LomcError/ValueError——
  ゲームの LoadCharacterPortrait は無効な表情 key に KeyNotFoundException を投げ → Lua コルーチン死亡 → 会話フリーズ）。
  say/show が参照する人物は先に show で登場させる必要があります（未登場でも同様に KeyNotFoundException）。
  書き込み入口の登場防線が show を自動補完します（add_node/update_node 参照）。エディターの検査は複数経路合流に対してグラフレベルでフォールバックします。

## 8. ユーザーコンテンツ（User Content、v1 は音声のみ）

開発環境のリポジトリは `%APPDATA%/lom_modkit/repository/` にあり、ランタイム依存**ではありません**。シナリオは安定参照のみを保存します：

```text
user:<namespace>.<content_id>     例如 user:mohui.boss_theme
```

公式 ID（`普通_001`、`brother4`）はそのまま保持し、`official:` に変更しません。

パッケージ内構造（実際に参照されるもののみ同梱）：

```text
assets/user/audio/mohui.boss_theme/content.json
assets/user/audio/mohui.boss_theme/boss_theme.ogg
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

台詞ボイスも `type=audio` のままです。任意の管理フィールド `character`（`user:mohui.luoxue` または公式キャラ id、例：`player`）を追加できます。このフィールドがない旧音声も引き続き合法です。`character` は `say.voice` の再生規約を変えず、未参照音声のパック原因にもなりません。

カスタムキャラの `content.json` には任意で `title`（台詞の短い称号）、`scale`（体型 50–130、既定 100、足元基準）、`art_facing`（原画の向き `left` 既定 / `right`）を書けます。未指定と旧パッケージは 100 / 左向きです。

- `type`：`audio` / `character`。
- `audio_kind`：`music` / `sound` / `env`。
- `character`（音声のみ、任意）：ユーザーキャラ参照または公式キャラ id。省略時はナレーション／システム／未紐づけ。
- コンテンツ ID：`[a-z][a-z0-9_]{0,31}.[a-z0-9][a-z0-9_]{0,47}`。`..`、`/`、`\`、`:` は禁止。
- 欠損、型不一致、metadata 破損、ファイル不存在、非対応拡張子、20MB 超過：pack は直接失敗します。暗黙のスキップは禁止。
- Python 側の唯一の解析入口：`compiler/lomc/content.py`。C# 側の契約実装：`ContentRef.cs` + `ModLoader`。

使用方法は `user_content.md` を参照してください。
