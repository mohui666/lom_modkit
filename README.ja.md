# lom_modkit

**『活侠伝』（Legend of Mortal）のストーリー Mod を作るビジュアルツールです。**

会話、背景、分岐、音声を編集して `.lommod` に書き出し、C# のゲームホストで読み込みます。

> 言語：[中文（CHS）](README.md) · [中文（CHT）](README.cht.md) · 日本語（このページ） · [한국어](README.ko.md)

## バージョンとダウンロード

| バージョン | 内容 |
| --- | --- |
| **現在のソース：v1.4.10** | Rust 製エディター・コンパイラーと C# ゲームホストを使用し、`.lommod` v3 の仕様を維持しています。 |
| **Windows リリース：v1.4.10** | プレビュー改善、台詞ごとの表情アイコン、章のグループとノードメモ。 |

[v1.4.10 をダウンロード（Windows x64）](https://github.com/mohui666/lom_modkit/releases/download/v1.4.10/lom_modkit-v1.4.10_windows_x64.zip) · [更新内容と検証範囲（簡体字中国語）](RELEASE_NOTES_v1.4.10.md)

ZIP 全体を展開し、`lom_modkit/lom-editor.exe` を起動してください。ゲームを終了して「ゲームと MOD」からゲーム側の Host を 1.4.10 に更新します。新機能に必要な Host の最低バージョンは、書き出すパッケージに自動で設定されます。

## 1.4.10 の新機能

- 台詞ごとに表情アイコンを設定し、終了後に元の設定へ戻せます。
- アイコン・エフェクト・長い章のプレビューを高速化し、旧素材で人物が大きく表示される問題を修正しました。
- 章のグループ化、折り畳み、名前変更と移動に対応しました。
- 手順一覧にメモを表示し、未入力の場合はノード ID を表示します。

ゲーム側の Host を 1.4.10 に更新してください。

## 現在のバージョンを実行する

リポジトリのルートで実行します。

```sh
cargo run --locked -p lom-editor
```

macOS アプリのビルド：

```sh
scripts/build-macos.sh
```

出力先は `out/LoM Modkit Rust.app` です。環境と素材の設定は[現在の Mac ガイド（簡体字中国語）](docs/chs/macos.md)、Windows のビルドと C# ホストの要件は [Rust ビルドガイド（簡体字中国語）](docs/chs/rust.md)を参照してください。

## ドキュメント

**現在の操作手順は簡体字中国語版を正本とします。** 日本語・繁体字中国語・韓国語の旧 UI ガイドは、まだ Rust 版に完全対応していません。Qt のメニューや Python のコマンドは旧版の参考情報です。

| 現在のドキュメント（簡体字中国語） | 内容 |
| --- | --- |
| [使い方](docs/chs/software_usage.md) | Rust エディターの操作と基本的な流れ |
| [対応機能](docs/chs/current_capabilities.md) | 実装済みの機能、制限、検証範囲 |
| [Rust CLI](docs/chs/ai_cli.md) | `lomc` コマンドと制御された編集 API |
| [Mod パッケージ仕様](docs/chs/mod_format.md) | `.lommod` v3、67 種類のノード、C# ホストとの互換性 |
| [Rust ビルド](docs/chs/rust.md) | Rust の構成、ビルド方法、移行範囲 |

[日本語ドキュメント索引と旧版の翻訳](docs/ja/README.md) · [全ドキュメント](docs/README.md)

## ライセンス

[MIT ライセンス](LICENSE)。ファンによる非公式ツールで、ゲーム開発元とは関係ありません。ゲーム本体、展開した素材、完全な逆コンパイルソースは含みません。サンプルはツールの機能を示すものです。
