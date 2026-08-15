# AI エージェント CLI / Python API マニュアル（story_api）

> 言語：[简体中文](../zh_CN/ai_cli.md) · [繁體中文](../zh_TW/ai_cli.md) · 日本語（本文） · [한국어](../ko/ai_cli.md)

`editor/story_api.py` は AI エージェントとスクリプト向けのシナリオデータインターフェースです：管理された書き込み操作（Python API）
+ argparse コマンドライン（check / compile / pack / new-story）。本ドキュメントは**サブプロセスとして
CLI を呼び出す、または直接 import する AI エージェント**向けで、例はすべてリポジトリの実環境で実行済みです（Windows + Python
3.10、editor/.venv）。

形式契約（ノードタイプ、パッケージ構造、ランタイム動作）は `mod_format.md` を参照。その §7 が
story_api の契約条項です。本ドキュメントはその操作マニュアルであり、両者が衝突した場合は契約を優先します。

核心ルール（契約 §7）：**AI は story JSON や Lua を直接手書きしません**。すべてのシナリオ構築は
story_api 経由——ノードは models 契約の既定値で生成、フィールドは NODE_SCHEMAS で検証、未知フィールドは
一律拒否され、コンパイル時の残存問題は lomc 検証がフォールバックします。エディターと AI は同一の防線を共有します。

インターフェースが受け付けるのは `models.NODE_SCHEMAS` の現行 47 種だけです。`combat` は原作 Combat テンプレートの win/lose 編成として実装済みです。未実装の `battle`、`reward`、`quest_*` は生成させないでください。

## 1. 環境要件と呼び出し方法

- Python 3.10+（リポジトリ付属の venv：`editor/.venv`）。story_api の依存は標準ライブラリ +
  `editor/models.py` + `compiler/lomc` のみで、**PySide6 に依存せず**、ヘッドレス環境で使えます。
- リポジトリ内の 2 つのリソースに依存し、どちらも欠かせません（リポジトリ付属、通常のクローンで入手可能）：
  - `compiler/lomc/`（コンパイラー。利用不可の場合 check/compile/pack/add_dice が
    「lomc 编译器不可用」（lomc コンパイラー利用不可）を報告）
  - `data/editor_data.json`（人物／表情／シーン／ダイスチェックポイントなどの公式一覧）

### 1.1 ソースコード形態（開発／AI サブプロセス）

```bash
cd editor
.venv/Scripts/python story_api.py <子命令> [参数]
```

スクリプト内部で `editor/` と `<リポジトリルート>/compiler` を `sys.path` に挿入するため、**カレント
ワーキングディレクトリに依存しません**——リポジトリルートからの実行も同様に成立します：

```bash
editor/.venv/Scripts/python editor/story_api.py check --json samples/demo_mod/story/main.json
# {"ok": true, "errors": [], "warnings": []}
```

パス導出：`EDITOR_DIR = story_api.py の所在ディレクトリ`、`PROJECT_ROOT = その 1 つ上`、
editor_data は `<リポジトリルート>/data/editor_data.json` を読みます。

### 1.2 凍結形態（PyInstaller パッケージ exe）

`editor/build_exe.py` は onedir デュアルエントリーパッケージを生成し、その中の `story_api_cli.exe` が本 CLI です。
対象マシンに Python は不要です：

```bash
editor/dist/lom_modkit/story_api_cli.exe check story.json --json
```

ソースコード形態との差異はパス導出のみです：`__file__` は展開ディレクトリを指し、プロジェクトルートは `_MEIPASS`
（`dist/lom_modkit/_internal`）に変わり、`data/editor_data.json` と lomc はパッケージング spec により
パッケージ内に同梱されます。**サブコマンド、引数、出力形式、終了コードはソースコード形態と完全に一致します**。

### 1.3 共通規約

- **終了コード**：`0` 成功。`1` 検証／コンパイル／パッケージング／IO 失敗。`2` argparse 用法エラー
  （引数不足、未知オプション。usage は stderr に出力）。
- **テキストモード**（既定）：結果パスは stdout に出力。`警告：...` と `错误：...` は一律
  **stderr** に出力。check がすべて通過した場合は**一切出力なし**。静寂が成功です。
- **--json モード**：stdout に**単一行**の構造化 JSON を出力（UTF-8 をバイトストリームに直接書き込み、Windows
  コンソールのエンコーディングを回避）。stderr はクリーンのまま。`--json` はサブコマンドの前後どちらに置いても有効です：

```bash
.venv/Scripts/python story_api.py check --json story.json   # 子命令后
.venv/Scripts/python story_api.py --json check story.json   # 子命令前
```

- プログラム入口は stdout/stderr を UTF-8 に reconfigure 済み。AI がサブプロセスとして呼び出す場合は UTF-8
  でデコードしてください。

## 2. サブコマンド詳解

例は `editor/` ディレクトリで `.venv/Scripts/python story_api.py` を使って実行します。一時ディレクトリ
`C:/Users/mohui666/AppData/Local/Temp/lom_cli_test` は `<TMP>` と略記します。

### 2.1 check — story.json の検証

```
usage: story_api check [-h] [--json] story_json
```

| 引数 | 説明 |
| --- | --- |
| `story_json` | story.json のパス（位置引数、必須） |
| `--json` | 単一行 JSON 出力 |

動作：シナリオを読み込んで検証します。errors が非空 → 終了コード 1。それ以外は 0。エラーと警告はいずれも完全な中文の文です。エラーには `story.json: ` の出所プレフィックスが付きます（このプレフィックスは固定で "story.json"。実際のファイル名とは無関係）。警告にはプレフィックスがありません。

```bash
# 全部通过：文本模式无任何输出，exit=0
.venv/Scripts/python story_api.py check ../samples/demo_mod/story/main.json
.venv/Scripts/python story_api.py check --json ../samples/demo_mod/story/main.json
# {"ok": true, "errors": [], "warnings": []}

# 有警告（非致命，exit 仍为 0）——transition phase=in 之后没有 out
.venv/Scripts/python story_api.py check --json <TMP>/warn.json
# {"ok": true, "errors": [], "warnings": ["节点 \"n2\"(transition, phase=in) 之后没有 phase=out 解除：TransitionIn 会隐藏剧情 UI 并盖满黑幕……请在其后补一个 phase=out 节点，或改用 scene 节点做转场。"]}

# 有错误（悬空 goto），exit=1
.venv/Scripts/python story_api.py check --json <TMP>/bad.json
# {"ok": false, "errors": ["story.json: 节点 \"n1\": goto 指向不存在的节点 \"not_exist\""], "warnings": []}

# 文件不存在，exit=1
.venv/Scripts/python story_api.py check <TMP>/nope.json
# stderr：错误：story.json 读取失败: [Errno 2] No such file or directory: '...nope.json'
```

> 注意：`new-story` したばかりのシナリオをそのまま check しても**通りません**——開場は show 登場 +
> 空 say で、say が最後のノードで明示的 goto がありません。これは正常な現象で、end ノードを 1 つ補えばよいだけです
> （§3.4 のワークフロー参照）：
> `{"ok": false, "errors": ["story.json: 节点 \"n2\"(say): 是最后一个节点且没有显式 goto，脚本无法正常结束（请改用 end/goto_scene/raw 节点或显式 goto）"], "warnings": []}`

### 2.2 compile — story.json → Lua のコンパイル

```
usage: story_api compile [-h] [-o OUTPUT] [--json] story_json
```

| 引数 | 説明 |
| --- | --- |
| `story_json` | story.json のパス（位置引数、必須） |
| `-o, --output` | 出力 .lua パス。既定は入力と同ディレクトリ・同名の `.lua` |
| `--json` | 単一行 JSON 出力 |

```bash
# 成功：文本模式 stdout 打印产物路径
.venv/Scripts/python story_api.py compile <TMP>/ok.json
# C:\...\lom_cli_test\ok.lua    exit=0

.venv/Scripts/python story_api.py compile <TMP>/ok.json -o <TMP>/out2.lua --json
# {"ok": true, "output": "C:\\...\\out2.lua", "warnings": []}

# 带警告编译（仍成功；Lua 头部同时嵌 `-- lomc 警告：` 注释）
.venv/Scripts/python story_api.py compile <TMP>/warn.json -o <TMP>/warn.lua --json
# {"ok": true, "output": "...\\warn.lua", "warnings": ["节点 \"n2\"(transition, phase=in) 之后没有 phase=out 解除：..."]}

# 失败（不写文件；注意失败时 JSON 只有 ok/errors 两个键，没有 warnings 键）
.venv/Scripts/python story_api.py compile <TMP>/bad.json --json
# {"ok": false, "errors": ["story.json: 节点 \"n1\": goto 指向不存在的节点 \"not_exist\""]}    exit=1
```

成果物ヘッダーの例（`-- Generated by lomc, do not edit` で始まる）：

```lua
-- Generated by lomc, do not edit
-- Source: story/my_tale.json (id=my_tale, title=测试剧情)

-- mod 内剧情 flag 表（不存档，重进游戏清零）
modflags = modflags or {}
mod_set_mood(false)
```

### 2.3 pack — mod ディレクトリ → .lommod のパッケージング

```
usage: story_api pack [-h] [-o OUTPUT] [--json] mod_dir
```

| 引数 | 説明 |
| --- | --- |
| `mod_dir` | mod ディレクトリ（`manifest.json` と `story/` サブディレクトリを含む。契約 §1/§2） |
| `-o, --output` | 出力 .lommod パス。既定は `<modディレクトリ>` と同階層・同名の `<ディレクトリ名>.lommod` |
| `--json` | 単一行 JSON 出力 |

パッケージング前に manifest を検証し、story/ 以下の全スクリプトを 1 つずつ検証+コンパイルします（ファイル名は内部
id と一致必須）。失敗すれば全体が失敗します。成果物 zip には `manifest.json`、`story/<id>.json`、
`lua/<id>.lua`、`texts.json`（既読テキスト表）が含まれます。

```bash
.venv/Scripts/python story_api.py pack <TMP>/my_mod -o <TMP>/my_mod_v2.lommod --json
# {"ok": true, "output": "C:/Users/mohui666/AppData/Local/Temp/lom_cli_test/my_mod_v2.lommod"}

# 失败样例
.venv/Scripts/python story_api.py pack <TMP>/no_manifest --json
# {"ok": false, "errors": ["mod 目录缺少 manifest.json: C:\\...\\no_manifest"]}    exit=1
```

> `samples/` 内のサンプル mod で pack を試す場合は**必ず `-o` で別の場所を指定してください**：既定の出力は
> `<modディレクトリ>.lommod`（例：`samples/demo_mod.lommod`）で、リポジトリの既存成果物を上書きします。

### 2.4 new-story — 新規シナリオスクリプト story.json の作成

```
usage: story_api new-story [-h] [--title TITLE] -o OUTPUT [--json] story_id
```

| 引数 | 説明 |
| --- | --- |
| `story_id` | シナリオスクリプト id。ルール `[a-zA-Z0-9_-]+`（位置引数、必須） |
| `--title` | タイトル。既定「新剧情」 |
| `-o, --output` | 出力 story.json パス（**必須**） |
| `--json` | 単一行 JSON 出力 |

```bash
.venv/Scripts/python story_api.py new-story my_tale --title "测试剧情" -o <TMP>/story2.json --json
# {"ok": true, "output": "C:\\...\\story2.json"}

.venv/Scripts/python story_api.py new-story "坏id!" -o <TMP>/bad.json --json
# {"ok": false, "errors": ["剧情脚本 id 非法: '坏id!'（规则 [a-zA-Z0-9_-]+）"]}    exit=1

.venv/Scripts/python story_api.py new-story my_tale
# story_api new-story: error: the following arguments are required: -o/--output    exit=2
```

生成されるファイル（UTF-8、インデント 2、中文保持）：開場は **show 登場 + 空 say** の 2
ノード固定（先に登場させてから動作。§4 ルール 4 参照）、`mood=false`（各 show/say の前後に自動で
`mod_hide_mood()` を出力して公式の気分バブルを非表示）、人物フィールドは既定で editor_data の最初の人物：

```json
{
  "id": "my_tale",
  "title": "测试剧情",
  "mood": false,
  "start": "n1",
  "nodes": [
    {
      "id": "n1",
      "type": "show",
      "character": "artist1",
      "position": "M",
      "portrait": "normal",
      "facing": "right",
      "fadeDuration": 0,
      "moveDuration": 0
    },
    {
      "id": "n2",
      "type": "say",
      "text": "",
      "character": "artist1",
      "portrait": "normal",
      "mode": "character"
    }
  ]
}
```

### 2.5 --json フィールド構造まとめ

| サブコマンド | 成功（exit 0） | 失敗（exit 1） |
| --- | --- | --- |
| check | `{"ok": true, "errors": [], "warnings": [...]}` | `{"ok": false, "errors": [...], "warnings": []}` |
| compile | `{"ok": true, "output": "<luaパス>", "warnings": [...]}` | `{"ok": false, "errors": [...]}`（**warnings キーなし**） |
| pack | `{"ok": true, "output": "<lommodパス>"}` | `{"ok": false, "errors": [...]}` |
| new-story | `{"ok": true, "output": "<story.jsonパス>"}` | `{"ok": false, "errors": [...]}` |

- `ok`：bool。唯一常に存在するキー。`errors`/`warnings`：文字列配列。要素は完全な中文の文。`output`：文字列パス（Windows では JSON 内のバックスラッシュが `\\` にエスケープ。pack で明示的に
  `-o` した場合は渡した形式のまま返却）。
- check は失敗時にも `warnings` キーを伴う唯一のサブコマンド。それ以外の失敗は一律 `ok`/`errors` のみ。
- 複数エラーの場合 `errors` は行ごとに分割されます（lomc の複数行メッセージは複数件に分割）。

## 3. Python API クイックリファレンス

```python
import sys
sys.path.insert(0, r"<仓库根>/editor")   # 任意 cwd 均可，story_api 内部自理 compiler 路径
import story_api
```

すべての関数のエラーメッセージは全中文です。**`ValueError` のみを投げます**（pack_mod は内部で lomc.LomcError
を ValueError に変換）。検証／コンパイル系の関数は例外を投げず、エラーリストを返します。

### 3.1 データと環境

| 関数 | 説明 |
| --- | --- |
| `load_editor_data() -> (dict, bool)` | `data/editor_data.json` を読み、(データ, フォールバックかどうか) を返す。毎回ディスクから読み直す。フォールバック=true はファイル欠損／破損で内蔵フォールバック一覧を使用したことを示す |

### 3.2 シナリオとノードの読み書き（書き込み操作はすべて固定ルールで検証）

| 関数 | 主要な制約 |
| --- | --- |
| `new_story(story_id="main", title="新剧情", mood=False) -> dict` | story_id は `[a-zA-Z0-9_-]+` に一致。title は str。mood は bool。show 登場(n1) + 空 say(n2) の開場を持つシナリオ dict を返す（先に登場させてから動作。§4 ルール 4 参照） |
| `get_node(story, node_id) -> dict` | 不存在 → ValueError。返すのは story 内の**元オブジェクト**（update でそのまま反映される） |
| `list_nodes(story) -> list[dict]` | 各項目は `{"id", "type", "summary"}`。summary は中文の要約（例：`对白·唐惟元: 师弟，你来了。`） |
| `add_node(story, node_type, fields=None, after=None) -> dict` | node_type は 47 種限定（`models.NODE_TYPES`）。fields のキーは NODE_SCHEMAS の合法フィールド+汎用フィールド（id/type/goto）に限定。型は kind に従い緩く検証。未知タイプ／フィールド／型不一致 → ValueError。id は自動生成（say1、show2、choice1…）。after=ノード id でその後ろに挿入、None で末尾に追加。**登場防線**：動作系ノードの対象人物がそれ以前に未登場／退場済みの場合、その前に show ノードを自動挿入（§4 ルール 4 参照） |
| `update_node(story, node_id, fields) -> dict` | add_node と同じフィールド検証。ノード不存在 → ValueError。マージ後に branch 正規化と表情検証を実施。**登場防線**：更新後に動作人物が未登場／退場済みなら、そのノードの前に show を自動挿入し、それを指す goto／選択肢／分岐ジャンプを新ノードへ付け替え（§4 ルール 4 参照） |
| `delete_node(story, node_id) -> dict` | 削除したノードを返す。**ダングリング goto は遮断せず**、check_story の報告に委ねる |
| `rename_node(story, node_id, new_id) -> dict` | ノード id を改名し、start と全ジャンプ参照（goto / choice 選択肢 / branch cases / dice 行き先）を同期。改名後のノードを返す。新 id は `[A-Za-z0-9_-]+` 限定（前後空白は除去）。old==new は空操作。番号占用または元ノード不存在 → ValueError |
| `move_node(story, node_id, delta) -> dict` | delta は ±1 のみ。範囲外（すでに先頭／末尾）→ ValueError |
| `set_start(story, node_id) -> dict` | story["start"] を設定。ノード不存在 → ValueError |
| `add_say(story, text, character=None, mode="character", portrait="normal", voice=None, after=None) -> dict` | mode ∈ character/think/narrative/center。character/think モードは character 必須（人物 id）。narrative/center は character フィールドを書かない。text は改行可。(character, portrait) は公式表情表で検証。voice は任意の user: 音声参照 |
| `add_scene(story, view, after=None) -> dict` | view は非空のシーン id 文字列 |
| `add_choice(story, options, after=None) -> dict` | options は [(text, goto), ...] の 2〜4 項目。text は非空 str、goto はノード id str。dialog は強制的に "Options"（§4 ルール 1 参照） |
| `add_dice(story, check, goto_成功, goto_失败, goto_大成功="", band_texts=None, after=None) -> dict` | check は公式メタデータに命中必須（`lomc.dice_data.get_dice_meta`）。2 バンドのチェックポイントは goto_成功/goto_失败 必須、goto_大成功 は空可。≥3 バンドは 3 つすべて必須。band_texts は任意のバンドごとの選択肢テキスト上書きで、件数=結果バンド数、各項目非空 |
| `add_death(story, text, death_id, next="Title", title=None, after=None) -> dict` | text は非空（複数行可）。death_id は ≥900000 の数字文字列（規約 9+公式 id）。next は "Title" のみ。title は任意。既定／空文字列はフィールドを書かない（codegen が「勝敗乃兵家常事」を使用） |

fields の型検証規約（`_check_kind`）：int/float フィールドは数値を受け付けるが **bool は拒否**
（`True` を float フィールドに渡すと拒否されます）。bool フィールドは bool のみ。options/cases/vars/
dice_options フィールドは list を受け付ける。それ以外は一律 str。値は変更せず検証のみ行います。

### 3.3 検証 / コンパイル / パッケージング / ファイル

| 関数 | 戻り値の規約 |
| --- | --- |
| `check_story(story) -> (errors, warnings)` | 2 つの文字列リスト。errors 非空で失敗。errors には `story.json: ` プレフィックス、warnings にはプレフィックスなし |
| `compile_story(story) -> (lua \| None, errors, warnings)` | 失敗 `(None, errors, [])`。成功 `(luaソース, [], warnings)`。lua ヘッダーには `-- lomc 警告：` コメントが埋め込み済み |
| `load_story_json(path) -> dict` | story.json を読む。読み込み失敗／構造不正 → ValueError |
| `save_story_json(story, path) -> None` | UTF-8、インデント 2、中文保持、末尾改行付きで書き出す |
| `pack_mod(mod_dir, output=None) -> str` | manifest 検証 + 全件コンパイル + zip パッケージング。.lommod パスを返す。失敗 → ValueError |

### 3.4 典型的なワークフロー（Python）

新規作成 → 開始ノードを埋める → ノード追加 → check → compile → pack：

```python
import story_api

# 1. 新建（开场是 show 登场 + 空 say；先填 say 文本，默认人物已在 n1 登场）
story = story_api.new_story("my_tale", "测试剧情")
say_id = story["nodes"][1]["id"]
story_api.update_node(story, say_id, {"text": "山门前，风很大。"})

# 2. 加节点（id 自动分配；choice/dice 的 goto 用节点 id 字符串）
story_api.add_scene(story, "center")
story_api.add_say(story, "师弟，你来了。", character="brother4")
#   ↑ brother4 此前未登场：登场防线自动在它前面插一个 show·唐惟元（§4 规则 4）
story_api.add_choice(story, [("迎上去", say_id), ("转身离开", say_id)])
story_api.add_node(story, "end")          # 别忘收尾，否则 check 报「无法正常结束」

# 3. 校验
errors, warnings = story_api.check_story(story)
if errors:
    raise SystemExit("\n".join(errors))

# 4. 编译 + 存档（可选）
lua, errors, warnings = story_api.compile_story(story)
story_api.save_story_json(story, "my_tale.json")

# 5. 打包（mod 目录需含 manifest.json 与 story/<id>.json，文件名=内部 id）
out = story_api.pack_mod("path/to/my_mod")
```

list_nodes の出力例（人物／シーンの表示名は editor_data の一覧由来。artist1=武師、
brother4=唐惟元、center=演武場_昼。n5 は登場防線が自動補完した show で、n4 の前に挿入されています）：

```python
[{'id': 'n1', 'type': 'show',   'summary': '人物登场·武师@M'},
 {'id': 'n2', 'type': 'say',    'summary': '对白·武师: 山门前，风很大。'},
 {'id': 'n3', 'type': 'scene',  'summary': '切换背景·校場_白天'},
 {'id': 'n5', 'type': 'show',   'summary': '人物登场·唐惟元@M'},
 {'id': 'n4', 'type': 'say',    'summary': '对白·唐惟元: 师弟，你来了。'},
 {'id': 'n6', 'type': 'choice', 'summary': '选项分支·2个选项'},
 {'id': 'n7', 'type': 'end',    'summary': '结束剧情·结束'}]
```

等価の CLI チェーンは §2 の 4 つのサブコマンドを順に呼び出すだけです（new-story → check → compile → pack）。

## 4. 書き込み操作の硬性ルール（破壊防止）

これらはゲーム側の既知クラッシュの落とし穴を書き込み入口で食い止めるルールです。**迂回しようとしないでください**（迂回しても
check_story/compile_story が遮断します）。各ルールのゲーム側の仕組みの詳細は契約 `mod_format.md`
§3/§4 を参照してください。

1. **choice スキンは Options に固定**。`add_choice` は `dialog="Options"` を強制します。その他のスキン
   はフリーシーンの break 形式メニューで、プレーンテキスト選択肢はゲーム内メニューをフリーズさせます。
   `update_node` で別スキンに変更しても、check_story がエラーを報告します。
2. **dice チェックポイントは公式メタデータに命中必須**。`add_dice` の check は
   `data/editor_data.json` の dice_meta 表内になければなりません（`load_editor_data()` で確認可。
   テスト用例：`S0205_01_001` は 2 バンド、`Ch_6_8_2_Break_01_001` は 3 バンド）。
   メタデータなしのチェックポイントはゲーム内ダイスメニューをクラッシュさせます。結果バンド数が goto の必須項目を決めます：
   2 バンドは goto_成功/goto_失败。≥3 バンドは goto_大成功 も必須。band_texts 上書きの
   件数は結果バンド数と一致必須。
3. **say モードと人物の連動**。mode=character/think では character（人物 id）必須。
   narrative/center では character フィールドを書きません（与えても削除されます）。
4. **動作人物は先に登場必須（登場防線）**。舞台上にいない人物への動作（say 会話／独白、
   move/face/hide/focus/offset/shock/dim/rotate）はゲームのシナリオコルーチンをクラッシュさせて黒画面になります。
   `add_node`/`add_say`/`update_node` は書き込み時にその人物が以前に登場済みで
   未退場かを線形チェックし、欠けていればそのノードの前に `show` ノードを自動挿入します（update_node は
   それを指す goto／選択肢／分岐ジャンプも新 show へ付け替えます）。複数経路合流などの複雑な状況はエディターの
   「検査」がグラフレベル解析でフォールバックします。自動補完された show を手動で削除しないでください。
5. **(character, portrait) は公式キャラ表情表内必須**。show/say ノードのキャラが表にあり
   表情がそのリストにない → ValueError。キャラが表にない（自作キャラ）→ 通過。表情表が利用不可
   （lomc 欠損）の場合、検証は check_story に降格します。
6. **未知フィールドは一律拒否**。fields は NODE_SCHEMAS が宣言したフィールド + 汎用フィールド
   （id/type/goto）のみ許可。余分なキーが 1 つでもあれば ValueError で許可集合を列挙します。型は kind で検証
   （数値フィールドは bool を拒否、bool フィールドは数値を拒否、リストフィールドは文字列を拒否、その逆も同様）。
7. **death_id は ≥900000 必須**（規約 9+公式 id。例：公式 10021 → 910021。公式 id
   は公式結末のアンロックと記録を引き起こし、セーブを汚染します）。death の next は "Title" のみ許可。
8. **branch キーフィールドの正規化**。source=stat は stat フィールドを使い flag をクリア。その他のソース
   （mod/game/flag_value/condition）は flag を使い stat をクリア。add_node/update_node が自動処理するため
   呼び出し側の対応は不要ですが、2 つのキーを手書きしないでください。
9. **削除／移動はグラフの完全性を保証しません**。delete_node が生むダングリング goto や、最後のノードに
   goto がない問題は書き込み入口では遮断せず、一律 check_story が報告します——**変更後は必ず check してください**。

## 5. よくあるエラーメッセージ対照表

以下のメッセージはすべて実際の実行から採取したものです（Python API は ValueError を送出。CLI テキストモードでは
`错误：`/`警告：` プレフィックスを付けて stderr に出力）。

| エラーメッセージ（例） | 出所 | 原因と対処 |
| --- | --- | --- |
| `未知节点类型: no_such_type（支持 47 种，见 models.NODE_TYPES）` | add_node | タイプ名の綴り間違い。`models.NODE_TYPES` または契約 §3.1 の 47 種を使用 |
| `节点类型 wait 不支持字段: bogus（允许: goto, id, seconds, type）` | add_node/update_node | フィールド名がタイプ表にない。メッセージ内の許可集合に従って修正 |
| `节点类型 wait 字段 "seconds" 类型不符（kind=float，应为 数值），实际为 'abc'` | add_node/update_node | フィールド型の誤り。`True` も数値フィールドに拒否されることに注意 |
| `通用字段 "goto" 必须是字符串` | add_node/update_node | goto/id/type は文字列のみ受付 |
| `after 指定的节点不存在: no_such` | add_* 系列 | after は既存ノード id または None |
| `节点不存在: no_such` | get/update/delete/move/rename/set_start | node_id の綴り間違いまたは削除済み。先に list_nodes で確認 |
| `节点编号已被占用: n5` / `节点编号只使用英文字母、数字、下划线或短横线` | rename_node | 新 id が既存ノードと衝突、または非法文字を含む |
| `delta 只能是 ±1，实际为 2` / `节点 n1 已在开头，无法再移动` | move_node | 1 マスずつの移動のみ対応。範囲外の移動 |
| `choice 选项必须是 2~4 项，实际 1 项` / `第 1 个选项必须是 (text, goto) 二元组` | add_choice | 選択肢は 2〜4 項目。各項目は (text, goto) の 2 要素組 |
| `骰子检查点 "X" 无官方元数据，请在编辑器清单里选择...` | add_dice | check が dice_meta 表にない。`load_editor_data()["dice_meta"]` で合法なチェックポイントを選択 |
| `检查点 "Ch_6_8_2_Break_01_001" 有 3 个结果带，goto_大成功 必填（最优带）` | add_dice | ≥3 バンドのチェックポイントは大成功ジャンプ必須 |
| `dice band_texts 条数必须等于检查点 "X" 的结果带数（N 条），实际为 M 条` | add_dice | 上書きテキスト件数と結果バンド数の不一致 |
| `mode="character" 时 character 必填（人物 id）` | add_say | character/think モードは人物 id 必須 |
| `say 模式非法: 'shout'（允许 character/think/narrative/center）` | add_say | mode の綴り間違い |
| `角色 "brother4" 没有表情 "angry3"（该角色表情：angry1、angry2、…、shock）。…KeyNotFoundException…` | add_say/add_node/update_node | 表情 id がそのキャラのリストにない。メッセージに列挙された合法な表情に修正 |
| `death_id 必须是 ≥900000 的 mod 专属数字 id…实际为 '10021'` | add_death | 公式 id を使用。9+公式 id（910021）に変更 |
| `death next 非法: 'Free'（原版死亡画面固定返回标题，只允许 Title）` | add_death | next は Title のみ受付 |
| `剧情脚本 id 非法: '坏id!'（规则 [a-zA-Z0-9_-]+）` | new_story / CLI new-story | id に非法文字 |
| `story.json: 节点 "n1": goto 指向不存在的节点 "not_exist"` | check/compile | ダングリング goto。goto を実在ノードに向けるか、ノード削除後に再接続 |
| `story.json: 节点 "n1"(say): 是最后一个节点且没有显式 goto，脚本无法正常结束…` | check/compile | シナリオに締めがない。末尾に end/goto_scene/raw ノードか明示的 goto を補う |
| `story.json: 节点 "n2"(choice): dialog 只支持 "Options"。…BreakOptionButton 解析崩溃…` | check/compile | choice スキンが非 Options に変更されている。Options に戻す |
| `story.json: 节点 "n2"(branch): source="stat" 时必填字段 "stat"…` | check/compile | branch の stat ソースで stat フィールド欠落 |
| `节点 "n2"(transition, phase=in) 之后没有 phase=out 解除…黑幕将一直覆盖到脚本结尾…` | check/compile（**警告**、exit 0） | transition in/out はペア必須。out を補うか scene トランジションに変更 |
| `mod 目录缺少 manifest.json: …` / `mod 目录缺少 story/ 子目录: …` | pack | 契約 §1/§2 に従いディレクトリ構造を補完 |
| `story/xx.json: 文件名与内部 id 不一致…` | pack | story ファイル名は内部 id と一致必須（my_tale.json ↔ "id": "my_tale"） |
| `story.json 读取失败: [Errno 2] No such file or directory…` | CLI 各サブコマンド | パスの誤り。Windows で exe に渡すパス区切り文字に注意 |
| `story.json 不是合法 JSON: …` / `story.json 结构非法：缺少 nodes 数组` | load_story_json / CLI | ファイル破損またはシナリオスクリプトでない。JSON を手書きせず API で生成 |
| `lomc 编译器不可用（ImportError: …）。预期位置：…/compiler` | check/compile/pack/add_dice | compiler/ ディレクトリの欠損または移動。リポジトリ構造を復元 |
| `usage: story_api ... error: the following arguments are required: -o/--output`（exit=2） | CLI | argparse 用法エラー。usage に従い引数を補う |

## 6. AI エージェントへの呼び出しアドバイス

- **--json を優先**：単一行、UTF-8、構造が安定（先に `ok` を読み、上表に従ってキーを取得）。stderr
  はクリーン。テキストモードは人が読むのに適しています。
- 各サブプロセス呼び出しは独立したプロセスで、editor_data は毎回ディスクから読み直されます——
  `data/editor_data.json` を変更しても何も再起動する必要はありません。
- story dict は Python プロセス内で再利用可能なオブジェクトです：複数回の add_/update_ の後に一度 check。
  ただしプロセス間では save_story_json / load_story_json で受け渡す必要があります。
- 書き込み操作が失敗（ValueError）した場合、シナリオオブジェクトは**部分的に変更済みの可能性があります**（例：update_node は
  フィールドをマージしてから表情検証を行う）。一括構築時は小刻みに check することを推奨します。
- テスト参照：`editor/tests/story_api_test.py`（GUI 依存なし。直接
  `.venv/Scripts/python tests/story_api_test.py` で実行可）。全公開関数をカバーしています。
