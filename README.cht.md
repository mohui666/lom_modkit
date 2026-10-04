# lom_modkit

**《活俠傳》（Legend of Mortal）視覺化劇情 Mod 製作工具。**

編排人物對白、場景、分支與音訊，匯出 `.lommod`，由 C# 遊戲宿主載入。

> 語言：[中文（CHS）](README.md) · 中文（CHT，本文） · [日本語](README.ja.md) · [한국어](README.ko.md)

## 版本與下載

| 版本 | 說明 |
| --- | --- |
| **目前原始碼：v1.2.0** | 編輯器與編譯器已改為 Rust；保留 C# 遊戲宿主及 `.lommod` v3 格式契約。 |
| **最新公開發行版（截至 2026-10-04）：v1.1.1** | 舊 Python / Qt 工具端的 Windows 發行版，不是目前的 Rust 編輯器。 |

[下載舊版 v1.1.1（Windows）](https://github.com/mohui666/lom_modkit/releases/download/v1.1.1/lom_modkit-v1.1.1_windows_x64.zip) · [v1.1.1 發行說明](https://github.com/mohui666/lom_modkit/releases/tag/v1.1.1)

要使用目前的 Rust 編輯器，請從原始碼執行或建置。舊版下載中的 `lom_editor.exe`、選單與操作流程不可直接套用到 Rust 版。

## 執行目前版本

在儲存庫根目錄執行：

```sh
cargo run --locked -p lom-editor
```

macOS 應用程式建置：

```sh
scripts/build-macos.sh
```

產物為 `out/LoM Modkit Rust.app`。環境與素材設定見[目前 Mac 指南（簡中）](docs/chs/macos.md)。Windows 建置與 C# 宿主需求見 [Rust 遷移文件（簡中）](docs/chs/rust_migration.md)。

## 文件

**目前操作以簡中指南為準。** 繁中、日文及韓文的舊版 UI 指南尚未完整同步 Rust；其中的 Qt 選單與 Python 命令僅供歷史版本參考。

| 目前文件（簡中） | 內容 |
| --- | --- |
| [軟體使用](docs/chs/software_usage.md) | Rust 編輯器的操作入口與基本流程 |
| [目前能力](docs/chs/current_capabilities.md) | 已實作能力、限制與驗證範圍 |
| [Rust CLI](docs/chs/ai_cli.md) | `lomc` 命令列與受控編輯 API |
| [Mod 包格式契約](docs/chs/mod_format.md) | `.lommod` v3、63 種節點與 C# 宿主相容性 |
| [遷移與建置](docs/chs/rust_migration.md) | Rust 元件、建置方式及遷移範圍 |

[繁中文件索引與歷史譯文](docs/cht/README.md) · [全部文件](docs/README.md)

## 授權

[MIT 授權](LICENSE)。粉絲自製工具，與遊戲開發商無關，不包含遊戲本體、解包素材或完整反編譯原始碼。範例僅示範工具能力。
