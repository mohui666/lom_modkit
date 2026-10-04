# lom_modkit ドキュメント

> 言語：[中文（CHS）](../README.md) · [中文（CHT）](../cht/README.md) · 日本語（このページ） · [한국어](../ko/README.md)

現在のソースは **v1.2.0** です。エディターとコンパイラーは Rust 製で、C# ゲームホストと `.lommod` v3 の仕様を維持しています。2026-10-04 時点の最新の公開リリース [v1.1.1](https://github.com/mohui666/lom_modkit/releases/tag/v1.1.1) は、旧 Python / Qt ツールです。

## 現在のバージョンのガイド

**操作手順は簡体字中国語（CHS）版を正本とします。** このディレクトリの UI ガイドは、まだ Rust 版に完全対応していません。旧メニュー、ショートカット、Python コマンドを現在の手順として使用しないでください。

日本語の入門：[Rust CLI 概要](ai_cli.md)。API とパラメーターの詳細は、以下の簡体字中国語版を参照してください。

| ガイド（簡体字中国語） | 内容 |
| --- | --- |
| [使い方](../chs/software_usage.md) | Rust エディターの基本操作 |
| [対応機能](../chs/current_capabilities.md) | 機能の範囲と検証状況 |
| [Mac エディター](../chs/macos.md) | ビルド、起動、素材の設定 |
| [CLI / API](../chs/ai_cli.md) | Rust `lomc` と制御された編集 API |
| [Mod パッケージ仕様](../chs/mod_format.md) | v3 の仕様と 63 種類のノード |
| [ユーザーコンテンツ](../chs/user_content.md) | カスタムキャラクター、画像、音声 |
| [メンテナーガイド](../chs/maintainers.md) | 保存、検査、公開、ゲームとの連携 |
| [Rust 移行](../chs/rust_migration.md) | 構成と移行範囲 |

## 未更新の日本語訳

以下は旧版や用語を参照するために残しています。現在の動作と仕様は、上記の簡体字中国語版で確認してください。

[使い方](software_usage.md) · [対応機能](current_capabilities.md) · [スクリプト / API](script_reference.md) · [Mod パッケージ仕様](mod_format.md) · [ユーザーコンテンツ](user_content.md) · [多言語化の規約](i18n.md)

逆コンパイルの手順は[簡体字中国語版](../chs/decompiled_api.md)にあります。ゲームの完全な逆コンパイルソースはリポジトリに含めません。ドキュメントは先に `chs/` を更新し、その後に各翻訳を同期します。
