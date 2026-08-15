# lom_modkit

**『活俠傳』（Legend of Mortal）ビジュアルシナリオ Mod 制作ツール。**

Lua を書く必要はありません。グラフィカルエディターで人物の台詞、シーン演出、分岐シナリオ、音楽・効果音を組み立て、
ワンクリックで `.lommod` を書き出し、そのままゲーム内で実行できます。

[![Release v0.7.0](https://img.shields.io/badge/release-v0.7.0-blue)](https://github.com/mohui666/lom_modkit/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-lightgrey)](#互換性)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)

**[⬇ Windows 版をダウンロード](https://github.com/mohui666/lom_modkit/releases/download/v0.7.0/lom_modkit-v0.7.0_windows_x64.zip)** ·
[クイックスタート](#クイックスタート) ·
[ドキュメント](docs/ja/README.md)

> 言語：[简体中文](README.md) · [繁體中文](README.zh_TW.md) · 日本語（本文） · [한국어](README.ko.md)

<!-- TODO(宣伝素材): ここに 8〜15 秒のループ GIF を置く：
     lom_editor.exe を開く → 新規シナリオ → キャラクター／台詞／シーン／音楽を選択 → F5 → ゲーム内の実際の演出。
     推奨パスは docs/assets/screenshots/demo.gif。作成後は ![demo](docs/assets/screenshots/demo.gif) でこのコメントを置き換える。 -->

## これは何

lom_modkit を使うと、『活俠傳』が**本来持つ人物、シーン、音楽、エフェクト、数値システム**をそのまま使ってオリジナルシナリオを作れます。
グラフィカルエディターでクリックして設定し、独立した `.lommod` Mod パッケージとして書き出すと、ゲーム内プラグインが読み込んで演出します。
「新戦役を開始」（ニューゲームの流れを引き継ぎ、セーブスロットを分離）とフリーモードのマップ地点トリガーに対応しています。

ファン制作のツールであり、ゲーム開発元とは無関係です。ゲーム本体のファイルは一切含みません。MIT ライセンス。

## できること

- **ビジュアルシナリオ編集**：人物、台詞、表情、立ち位置、シーン、音楽、効果音、エフェクトをすべて UI で設定できます。
- **分岐シナリオ**：選択肢、条件分岐、属性判定、ダイス判定、複数チャプターの連鎖スクリプトに対応。
- **ストーリー内容のローカライズ**：同じ Story で簡体字・繁体字・日本語・韓国語を管理でき、既定言語と欠落時のフォールバックに対応。旧プロジェクトの移行は不要です。
- **ゲームコンテンツを直接呼び出し**：ゲーム既存の人物、シーン、音楽、演出システムを使えるため、自分で一から作り直す必要はありません。
- **カスタム音声と台詞ボイス**：`.ogg` / `.wav` を取り込み、音楽・効果音として、またキャラクターの台詞ごとのボイスとして使えます。
- **ワンクリックでゲーム内試遊**：任意のシナリオステップを選んで F5 を押すと、そのステップから直接ゲームでテストできます。
- **本物の Mod パッケージ**：書き出した `.lommod` は自己完結型で、そのまま他のプレイヤーに配れます。

## クイックスタート

### 1. ダウンロード

[lom_modkit-v0.7.0_windows_x64.zip](https://github.com/mohui666/lom_modkit/releases/download/v0.7.0/lom_modkit-v0.7.0_windows_x64.zip) をダウンロードして解凍します。Python のインストールは不要です。

### 2. 起動

`lom_editor.exe` を実行します。

### 3. 『活俠傳』に接続

メニュー「ファイル → インストール管理」で `Mortal.exe` を含むゲームフォルダーを選び、「BepInEx をインストール」をクリックします。
エディターが互換性のある BepInEx 6 とゲーム内ランタイムを自動でダウンロード・インストールし、Steam 通常起動用の修正も書き込みます。

### 4. 最初のシナリオを作る

新規シナリオ → キャラクターを追加 → 台詞を追加 → **F5** で試遊。

### 5. 書き出し

書き出すと `xxx.lommod` が得られます。あとは相手に渡すだけです（相手側にも本ツールでインストールしたランタイムが必要です）。

## スクリーンショット

<!-- TODO(宣伝素材): 少なくとも 4 枚の画像。docs/assets/screenshots/ に置くのがおすすめ：
     ① メインエディターの全体像 ② シナリオフロー図 ③ ユーザー音声／台詞ボイス ④ ゲーム内の実際の効果。
     コメントを解除してパスを差し替える：
![シナリオエディター](docs/assets/screenshots/editor.png)
![ゲーム内の効果](docs/assets/screenshots/ingame.png)
![分岐とフロー図](docs/assets/screenshots/flow_graph.png)
-->

## シナリオ制作のためのワークフロー

### 任意の位置から試遊（F5）

ステップを選んで **F5** を押すと、エディターが独立した一時パッケージを生成し、ゲームが安全なシーンに到達した時点で自動的にそのステップから開始します。
開始前には、そのステップ以前の舞台状態（現在のシーン、舞台上の人物の立ち位置・表情・向き）が自動で補完されるため、
シナリオの途中から入っても「キャラクターが存在しない」ことによるブラックスクリーンにはなりません。一時パッケージは正式な Mod を上書きせず、読み込み後に自動で削除されます。

### 書き出し前の検査（F6）

**F6** でチェックできる項目：コンパイルエラー、断線や到達不能ステップ、無限ループ、プレースホルダーテキスト、不足している素材、
誤ったユーザー音声参照、「人物が登場する前に発言・行動する」ブラックスクリーンのリスク。問題をダブルクリックすると該当ステップに移動できます。
「安全な自動修復」はシナリオの意味を変えない機械的な問題（人物の自動登場補完を含む）だけを処理し、元に戻す操作に対応しています。

### シナリオフロー図

右側の「フロー図」には実際のジャンプの接続線が表示され（1 対多の分岐は色で区別）、断線、
終了できない無限ループ、到達不能ステップは赤枠と文字で同時に示されます。

## ユーザーコンテンツ

PC 上のキャラクター、音声、画像を「ユーザーコンテンツ庫」（メニュー「ファイル → ユーザーコンテンツ庫」）に取り込むと、安定した番号
（例：`user:mohui.battle`）が付き、シナリオステップでは「ユーザー / 公式」のグループに分けて選択できます。シナリオにはこの番号だけを保存します。
書き出し時は現在の Mod が実際に参照しているコンテンツだけが同梱されるため、プレイヤーの PC は作者のローカルコンテンツ庫に依存しません。

| コンテンツ種別 | 状態 |
| --- | --- |
| カスタム音楽 / 効果音 / 環境音 | ✅ 対応済み |
| キャラクター台詞ボイス | ✅ 対応済み |
| カスタム立ち絵 / 称号 / 紹介カード / 体型 | ✅ 対応済み |
| カスタム背景 / CG / Overlay 画像 | ✅ 対応済み |
| コミュニティコンテンツ庫 | ◯ Roadmap |

詳しい使い方は [ユーザーコンテンツ庫のドキュメント](docs/ja/user_content.md) を参照してください。

## 他人が作った Mod をインストールする

`.lommod` をエディターの「ファイル → インストール管理」でインストールし、有効にチェックを入れるだけです
（手動のパス：`BepInEx/plugins/MortalModHost/mods/`）。
ゲームに入ったら、フリーシーン／タイトル画面左下の「活俠MOD」ボタン（または F8）を押し、
「mod シナリオを再生」または「新戦役を開始」を選びます。ゲーム内メニューはゲームの現在の言語に従います。

## 互換性

| 項目 | 状態 |
| --- | --- |
| Windows 10/11 | ✅ |
| Steam 版『活俠傳』 | ✅（通常起動の修正を含む） |
| BepInEx | エディターが自動インストール |
| Python | Windows 版では不要 |
| ゲーム原本ファイルの改変 | 不要 |

## 現在のバージョン

**v0.7.0**：カスタム立ち絵 · 台詞ボイス紐づけ · 紹介カードと称号 · 体型スライダー ·
退場時の清台 · ノードを種類で番号付け。

全変更内容は [Release Notes](https://github.com/mohui666/lom_modkit/releases) を参照してください。

## Roadmap

- コミュニティコンテンツリポジトリ（ユーザーコンテンツの共有・再利用）
- 作者向け戦闘 / 戦役オーケストレーション層（現在は検証済み低レベルノードのみ）

## ドキュメント

| ドキュメント | 内容 |
| --- | --- |
| [ドキュメント索引](docs/ja/README.md) | 言語ナビゲーションと読者ガイド |
| [ユーザーコンテンツ庫](docs/ja/user_content.md) | カスタム音声 / 台詞ボイスの使い方 |
| [現在の機能と境界](docs/ja/current_capabilities.md) | 実装済み、低レベルのみ、未実装の境界 |
| [Mod パッケージ形式の契約](docs/ja/mod_format.md) | パッケージ構造、49 種のノード、コンパイル規約、ランタイムの挙動 |
| [AI / CLI マニュアル](docs/ja/ai_cli.md) | story_api コマンドラインと Python API |
| [多言語対応](docs/ja/i18n.md) | UI とドキュメントの i18n アーキテクチャ |

## For Developers

### アーキテクチャ

```text
┌─────────────┐
│ lom_editor  │  PySide6 图形编辑器
└──────┬──────┘
       │ story JSON
       ▼
┌─────────────┐
│    lomc     │  JSON → 游戏原生 Lua 编译器（纯标准库）
└──────┬──────┘
       │ Lua + assets
       ▼
┌─────────────┐
│   .lommod   │  自包含 Mod 包（zip）
└──────┬──────┘
       ▼
┌──────────────────┐
│ MortalModHost    │  BepInEx 游戏内插件（C# net48）
└──────┬───────────┘
       ▼
  Legend of Mortal
```

### ソースコード構成

- `compiler/`（`lomc`）— JSON シナリオ → ゲームネイティブ Lua コンパイラー
- `editor/` — PySide6 グラフィカルエディター。`editor/story_api.py` は AI／スクリプト向けの管理されたインターフェース（Python API + CLI）
- `runtime/MortalModHost/` — BepInEx ゲーム内プラグイン
- `tools/` — 解凍成果物からエディターデータ／素材を抽出するスクリプト
- `data/` — エディターデータ（`editor_data.json`、schema 3）
- `samples/` — サンプル mod（demo_mod、showcase、showcase2 全ノードデモ 2.0、snack_case《点心大盗疑案》、probe）

### ソースから実行

```bash
# 编辑器
cd editor
python -m venv .venv
.venv/Scripts/pip install PySide6
run_editor.bat

# 编译器（无依赖）
PYTHONPATH=compiler python -m lomc check story.json
PYTHONPATH=compiler python -m lomc pack mod目录 -o 我的mod.lommod
```

### ビルドとテスト

```bash
# 编译器测试（160 例）
cd compiler && python -m unittest tests.test_lomc

# 编辑器测试（冒烟/压力，offscreen 无头运行）
cd editor && .venv/Scripts/python tests/smoke_test.py
cd editor && .venv/Scripts/python tests/stress_test.py

# story_api / 登场防线测试（61 + 18 例）
cd editor && .venv/Scripts/python tests/story_api_test.py
cd editor && .venv/Scripts/python tests/stage_guard_test.py

# 插件构建与冒烟测试
cd runtime/MortalModHost && dotnet build -c Release
cd runtime/MortalModHost && dotnet run --project test/SmokeTest -c Release
```

Windows 配布版のパッケージング：`cd editor && .venv/Scripts/python build_exe.py`
（成果物は `editor/dist/lom_modkit/` に出力。`lom_editor.exe` と `story_api_cli.exe` を含みます）。

ゲーム内デバッグ：任意のシーンで **F7** を押すと「原版シナリオを無効化」のセッション単位スイッチを切り替えられます（永続化しません）。
既読が黄色くなる場合の再テストには、エディターの「試遊 → 既読状態をリセット」を使います。

## FAQ

**Q：Steam で「開始」を押しても「活俠MOD」ボタンが出ない、F8 が反応しない？**
エディターの「ファイル → インストール管理」で「Steam で読み込めない問題を修復」をクリックし、Steam から**通常起動**してください（管理者として実行しないこと）。

**Q：Python のインストールは必要ですか？**
不要です。Windows 版は独立した exe です。ソースから実行・開発する場合のみ、Python 3.10+ と .NET（プラグインのビルド用）が必要です。

**Q：Mod はゲームファイルやセーブデータを変更しますか？**
公式スクリプトやテキスト表は変更しません。「新戦役を開始」は分離されたセーブスロット（`mod_<modid>`）を使うため、通常のセーブを上書きしません。

**Q：作った Mod を他人に配れますか？**
配れます。書き出した `.lommod` は自己完結型（参照している音声・画像を同梱）で、相手が本ツールでランタイムをインストール済みならそのまま遊べます。

## ライセンスと免責

MIT ライセンス（[LICENSE](LICENSE)）。ファン制作のツールであり、ゲーム開発元とは無関係です。ゲーム本体のファイルは一切含みません。

- ゲームメカニクスの調査は公式スクリプトの実証分析（1814 本のシナリオスクリプト）に基づきます。逆コンパイルしたソースコードは著作権の都合でリポジトリに公開していません。
- `data/editor_data.json` は `tools/extract_editor_data.py` が解凍成果物から生成します。リポジトリには解凍成果物とゲームファイルは含まれません。
- サンプル mod はツールの能力を示すためのもので、ゲーム原作のシナリオコンテンツは含みません。
