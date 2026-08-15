# lom_modkit — 活俠傳（Legend of Mortal）Mod ツール

プレイヤーがグラフィカルなエディターでシナリオ（ゲーム内の人物、シーン、エフェクト、音楽、数値を呼び出す）を自由に作り、
`.lommod` パッケージとして書き出して互いに共有できます。ゲーム内では BepInEx プラグインが読み込んで演出し、「新戦役を開始」
（ニューゲームの流れを引き継ぐ）とフリーモードのマップ地点トリガーに対応しています。

MIT ライセンス。本ツールはファン制作のツールであり、ゲーム開発元とは無関係です。ゲーム本体のファイルは一切含みません。

> 言語：[简体中文](README.md) · [繁體中文](README.zh_TW.md) · 日本語（本文） · [한국어](README.ko.md)

## コンポーネント

- `compiler/`（`lomc`）— JSON シナリオ → ゲームネイティブ Lua コンパイラー（Python 標準ライブラリのみ。パッケージ形式の契約は `docs/ja/mod_format.md` を参照）
- `editor/` — PySide6 グラフィカルエディター（3 ペイン：シナリオ構造 / 現在のオブジェクト / プレビュー。ツールバーは試遊と書き出しのみ。F5 で現在のステップからゲームに入る、フロー図、検査、インストール管理、既読リセット、複数チャプター、元に戻す／やり直し）
- `editor/story_api.py` — AI／スクリプトから使える管理されたツールインターフェース（Python API + CLI）：すべての書き込み操作は固定ルールで検証され、AI が story JSON/Lua を直接手書きすることはありません
- `runtime/MortalModHost/` — BepInEx ゲーム内プラグイン（C# net48）：`.lommod` のスキャン、Harmony による演出のインターセプト、戦役ごとのセーブ分離、位置トリガー、既読テキスト、人物紹介カードと死亡／エンディングテキスト。Steam の通常起動で使用できます
- `tools/` — 解凍成果物からエディターデータ／プレビュー素材を抽出するスクリプト、画面キャプチャ補助スクリプト
- `data/` — エディターデータ（`editor_data.json`：人物／表情／シーン／音楽／属性／ダイス検査点の一覧、schema 3）
- `samples/` — サンプル mod（demo_mod、showcase、showcase2 全ノードデモ 2.0、snack_case《点心大盗疑案》、probe）

## クイックスタート

### 1. コンパイラー（依存なし）

```bash
# 校验 / 编译 / 打包
PYTHONPATH=compiler python -m lomc check story.json
PYTHONPATH=compiler python -m lomc build story.json -o out.lua
PYTHONPATH=compiler python -m lomc pack  mod目录 -o 我的mod.lommod
```

### 2. エディター（PySide6）

```bash
cd editor
python -m venv .venv
.venv/Scripts/pip install PySide6
run_editor.bat          # 或直接双击运行
```

### 3. ゲーム内プラグイン（BepInEx）

1. エディターのメニュー「ファイル → インストール管理」で、`Mortal.exe` を含むゲームフォルダーを選択します。
2. 「BepInEx をインストール」をクリックすると、エディターが公式ダウンロードサイトから互換性のある BepInEx 6 Mono x86 build 692 をインストールして検証します。続いてランタイムを自動インストールし、Steam 通常起動用の修正（`version.dll` + `ignore_disable_switch`）を書き込みます。以降、書き出した `.lommod` も自動でコピー・有効化されます。
3. Steam で「開始」を押してもタイトルに「活俠MOD」が出ない、F8 が反応しない場合：インストール管理で「Steam で読み込めない問題を修復」をクリックし、Steam から**通常起動**してください（管理者として実行しないこと）。
4. 同じウィンドウでインストール済み Mod の有効／無効を切り替えられます。手動のパスは従来どおり `BepInEx/plugins/MortalModHost/mods/` です。
5. ゲームに入る：フリーシーン／タイトル画面左下の「活俠MOD」ボタンまたは F8 でメニューを開き、「mod シナリオを再生」または「新戦役を開始」を選びます。

再プレイ時に既読が黄色くなる場合：まずゲームを終了してから、エディターの「試遊 → 既読状態をリセット」を実行します。現在の mod と F5 試遊パッケージ（`lom_modkit_preview`）の記録を同時に消去します。

自作の音楽と効果音：メニュー「ファイル → ユーザーコンテンツ庫」で `.ogg` / `.wav`（≤20MB）を取り込むと、`user:mohui.battle` のような安定した番号が付きます。音楽／効果音ステップでは「ユーザー / 公式」のグループに分けて選択でき、台詞ステップでは「台詞ボイス」を結びつけられます。シナリオにはこの番号だけを保存します。書き出し時は現在の Mod が実際に参照している音声だけが同梱されるため、別の PC にインストールしても作者のローカルのコンテンツ庫に依存しません。説明は `docs/ja/user_content.md` を参照してください。

書き出す前に F6 で「検査」を開けます。コンパイルエラー、断線や到達不能ステップ、プレースホルダーテキスト、画像素材、ユーザー音声参照、そして「人物が登場する前に動作・発言する」ブラックスクリーンのリスクをチェックします。問題をダブルクリックすると該当ステップに移動できます。「安全自動修復」はシナリオの意味を変えない機械的な問題（人物の自動登場補完を含む）だけを処理し、元に戻す操作に対応しています。

長いシナリオのデバッグでは、ステップを選択して F5 を押します。エディターは独立した一時パッケージを生成・インストールし、ゲームが Title/Free の安全なシーンに到達すると自動的にそのステップから開始します。開始前にそのステップ以前の舞台状態（現在のシーン、舞台上の人物の位置／表情／向き）が自動で補完されるため、シナリオの途中から入っても「キャラクターが存在しない」ことによるブラックスクリーンにはなりません。一時パッケージは正式な Mod を上書きせず、読み込み後に自動で削除されます。右側の「フロー図」には実際のジャンプの接続線が表示され（1 対多の分岐は色で区別）、断線、終了できない無限ループ、到達不能ステップは赤枠と文字で同時に示されます。

### 4. 単体実行ファイル（PyInstaller パッケージング、任意）

エディターと AI コマンドラインからそれぞれ exe を生成し、同一のランタイムディレクトリを共有します。対象マシンに Python は不要です。

```bash
cd editor
.venv/Scripts/pip install pyinstaller
.venv/Scripts/python build_exe.py
```

成果物は `editor/dist/lom_modkit/` に出力されます（`build/`、`dist/` は gitignore 済み）：

| ファイル | 説明 |
| --- | --- |
| `lom_editor.exe` | グラフィカルエディター（コンソールウィンドウなし。データ一覧は内蔵で、素材がない場合はプレースホルダー画像を使用。開く／保存は既定で現在の作業ディレクトリから開始） |
| `story_api_cli.exe` | AI／スクリプト向けのコマンドライン（check / compile / pack / new-story） |

`story_api_cli` の使い方（終了コード 0/1、UTF-8。AI では `--json` を付けて 1 行の構造化結果を取得するのがおすすめ）：

```bash
story_api_cli check story.json
story_api_cli check --json story.json            # {"ok": true, "errors": [], "warnings": []}
story_api_cli compile story.json -o out.lua
story_api_cli pack mod目录 -o 我的mod.lommod
story_api_cli new-story my_story -o story.json
```

`--json` はサブコマンドの前後どちらに置いても構いません。失敗時も同様に `{"ok": false, "errors": [...]}` を出力し、終了コードは 1 のままです。

AI エージェント向けの詳細マニュアル（環境要件、各サブコマンドの引数／出力／終了コード、--json のフィールド構造、Python API クイックリファレンス、書き込み操作の硬性ルール、エラー対応表）は `docs/ja/ai_cli.md` を参照してください。

## 開発

```bash
# 编译器测试（160 例）
cd compiler && python -m unittest tests.test_lomc

# 编辑器测试（冒烟/压力，offscreen 无头运行）
cd editor && .venv/Scripts/python tests/smoke_test.py
cd editor && .venv/Scripts/python tests/stress_test.py

# story_api / 登场防线测试（61 + 18 例）
cd editor && .venv/Scripts/python tests/story_api_test.py
cd editor && .venv/Scripts/python tests/stage_guard_test.py

# 插件构建
cd runtime/MortalModHost && dotnet build -c Release

# 插件冒烟测试（ModLoader/MiniJson/ModRegistry 离线验证）
cd runtime/MortalModHost && dotnet run --project test/SmokeTest -c Release
```

ゲーム内デバッグ：任意のシーンで F7 を押すと「原版シナリオを無効化」グローバル一時スイッチを切り替えられます（セッション単位で、永続化しません）。
有効にすると、Free に戻ったときに自動トリガーされるものと、地点クリックでトリガーされる公式のメイン・サブ・地点デフォルトスクリプトをスキップします。mod トリガーは引き続き優先されます。このスイッチは今回のゲームセッションでのみ有効で、F7 をもう一度押すかゲームを再起動すれば元に戻ります。
すでに開始された Story 演出が強制中断されることはなく、F8 メニューにも影響しません。

## 0.6.0

- エディターの情報設計：メニューは低頻度の操作を担当し、ツールバーは試遊／書き出しのみ。左ペインはチャプターとステップだけを管理し、チャプターのプロパティは中央ペインへ。ステップは 2 行のテキスト表示、右クリックで削除／移動。プレビューの台詞は文字数に応じて広がり、中国語の折り返しに対応。
- 既読リセットは `Save_universe.dat` と `.json` の両方を変更し、F5 試遊パッケージ `lom_modkit_preview` の記録も消去します。
- 音楽／環境音の `fadeout` の後にフェードアウト時間いっぱい `wait` し、次の `PlayMusic` が音量を瞬時に戻してしまうのを防ぎます。
- Steam の通常起動で BepInEx を読み込めます（`version.dll` + `ignore_disable_switch`）。
- サンプル `showcase2`：シーン 1 後半のナレーションと会話を分割。魏菊が場面転換／第 2 幕に入る前に退場します。

## 言語

エディターのメニュー「言語」で簡体字中国語・繁体字中国語・日本語・韓国語を切り替えられ、設定は記憶されます。

ゲーム内の名詞は次の順で取得します。

1. [LoM-wiki](https://github.com/mohui666/LoM-wiki-CNS)（人物、門派、汗青書 / 生死簿、属性など）
2. wiki にない項目は、ゲーム解凍の公式言語表（`lom_unpack/raw/*_zh-cn.txt` / `*_zh-tw.txt` / `*_kr.txt`）を使用

公式ゲームのインターフェースは繁体字・簡体字・韓国語のみで、**日本語はありません**。日本語の人物名と属性名は wiki の日本語ページ由来で、韓国語はすべて公式の解凍データ由来です。ゲーム内の Mod メニューはゲームの現在の言語に従います。

実装の詳細（ディレクトリ構造、フォールバック規則、名詞の再生成、新しい言語の追加方法）は `docs/ja/i18n.md` を参照してください。

## 補足と謝辞

- `docs/ja/mod_format.md` は全コンポーネントの契約（パッケージ形式、43 種のノード、ユーザーコンテンツ、ランタイムの挙動）です。コードを変える前にまずこれを更新してください。自作音声の使い方は `docs/ja/user_content.md` を参照してください。
- `data/editor_data.json` は `tools/extract_editor_data.py` がゲームの解凍成果物から生成します
  （解凍ディレクトリは環境変数 `LOM_UNPACK_DIR` で指定）。リポジトリには解凍成果物とゲームファイルは含まれません。
- ゲームメカニクスの調査は公式スクリプトの実証分析（1814 本のシナリオスクリプト）に基づきます。逆コンパイルしたゲームのソースコードは
  著作権の都合でリポジトリには公開せず、ローカルの `docs/research/` にのみ研究用として残しています。
- サンプル mod はツールの能力を示すためのもので、ゲーム原作のシナリオコンテンツは含みません。
