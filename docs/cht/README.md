# lom_modkit 文件

> 語言：[中文（CHS）](../README.md) · 中文（CHT，本文） · [日本語](../ja/README.md) · [한국어](../ko/README.md)

目前原始碼為 **v1.2.0**：Rust 編輯器與編譯器，保留 C# 遊戲宿主及 `.lommod` v3 契約。截至 2026-10-04，最新公開發行版仍為 [v1.1.1](https://github.com/mohui666/lom_modkit/releases/tag/v1.1.1)，屬於舊 Python / Qt 工具端。

## 目前版本的指南

**操作以簡中（CHS）文件為準。** 本目錄的 UI 指南尚未完整同步 Rust，請勿將舊版選單、快捷鍵或 Python 命令當作目前操作。

繁中入門：[Rust CLI 簡介](ai_cli.md)。完整 API 與參數請查閱下方簡中參考。

| 指南（簡中） | 內容 |
| --- | --- |
| [軟體使用](../chs/software_usage.md) | Rust 編輯器基本流程 |
| [目前能力](../chs/current_capabilities.md) | 功能邊界與驗證範圍 |
| [Mac 編輯器](../chs/macos.md) | 建置、啟動與素材設定 |
| [CLI / API](../chs/ai_cli.md) | Rust `lomc` 與受控編輯 API |
| [Mod 包格式](../chs/mod_format.md) | v3 契約與 63 種節點 |
| [使用者內容](../chs/user_content.md) | 自訂人物、圖片與音訊 |
| [維護者手冊](../chs/maintainers.md) | 保存、檢查、發布與遊戲接入 |
| [Rust 遷移](../chs/rust_migration.md) | 元件與遷移範圍 |

## 尚未同步的繁中譯文

以下保留供查閱舊版與術語；目前行為及契約請核對上方簡中正本。

[軟體使用](software_usage.md) · [目前能力](current_capabilities.md) · [腳本 / API](script_reference.md) · [Mod 包格式](mod_format.md) · [使用者內容](user_content.md) · [多語言約定](i18n.md)

反編譯流程見[簡中文件](../chs/decompiled_api.md)；完整遊戲反編譯原始碼不進儲存庫。更新文件時先改 `chs/`，再同步相應譯文。
