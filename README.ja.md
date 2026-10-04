# lom_modkit

**『活侠伝』（Legend of Mortal）のストーリー Mod を作るビジュアルツールです。**

会話、背景、分岐、音声を編集して `.lommod` に書き出し、C# のゲームホストで読み込みます。

> 言語：[中文（CHS）](README.md) · [中文（CHT）](README.cht.md) · 日本語（このページ） · [한국어](README.ko.md)

## バージョンとダウンロード

| バージョン | 内容 |
| --- | --- |
| **現在のソース：v1.2.0** | エディターとコンパイラーは Rust に移行済みです。C# ゲームホストと `.lommod` v3 の仕様は維持しています。 |
| **最新の公開リリース（2026-10-04 時点）：v1.1.1** | 旧 Python / Qt ツールの Windows 版です。現在の Rust エディターではありません。 |

[旧版 v1.1.1 をダウンロード（Windows）](https://github.com/mohui666/lom_modkit/releases/download/v1.1.1/lom_modkit-v1.1.1_windows_x64.zip) · [v1.1.1 リリース情報](https://github.com/mohui666/lom_modkit/releases/tag/v1.1.1)

現在の Rust エディターはソースから実行またはビルドしてください。旧版の `lom_editor.exe`、メニュー、操作手順は Rust 版とは異なります。

## 現在のバージョンを実行する

リポジトリのルートで実行します。

```sh
cargo run --locked -p lom-editor
```

macOS アプリのビルド：

```sh
scripts/build-macos.sh
```

出力先は `out/LoM Modkit Rust.app` です。環境と素材の設定は[現在の Mac ガイド（簡体字中国語）](docs/chs/macos.md)、Windows のビルドと C# ホストの要件は [Rust 移行ガイド（簡体字中国語）](docs/chs/rust_migration.md)を参照してください。

## ドキュメント

**現在の操作手順は簡体字中国語版を正本とします。** 日本語・繁体字中国語・韓国語の旧 UI ガイドは、まだ Rust 版に完全対応していません。Qt のメニューや Python のコマンドは旧版の参考情報です。

| 現在のドキュメント（簡体字中国語） | 内容 |
| --- | --- |
| [使い方](docs/chs/software_usage.md) | Rust エディターの操作と基本的な流れ |
| [対応機能](docs/chs/current_capabilities.md) | 実装済みの機能、制限、検証範囲 |
| [Rust CLI](docs/chs/ai_cli.md) | `lomc` コマンドと制御された編集 API |
| [Mod パッケージ仕様](docs/chs/mod_format.md) | `.lommod` v3、63 種類のノード、C# ホストとの互換性 |
| [移行とビルド](docs/chs/rust_migration.md) | Rust の構成、ビルド方法、移行範囲 |

[日本語ドキュメント索引と旧版の翻訳](docs/ja/README.md) · [全ドキュメント](docs/README.md)

## ライセンス

[MIT ライセンス](LICENSE)。ファンによる非公式ツールで、ゲーム開発元とは関係ありません。ゲーム本体、展開した素材、完全な逆コンパイルソースは含みません。サンプルはツールの機能を示すものです。
