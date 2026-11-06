# lom_modkit

**《活俠傳》（Legend of Mortal）視覺化劇情 Mod 製作工具。**

編排人物對白、場景、分支與音訊，匯出 `.lommod`，由 C# 遊戲宿主載入。

> 語言：[中文（CHS）](README.md) · 中文（CHT，本文） · [日本語](README.ja.md) · [한국어](README.ko.md)

## 版本與下載

| 版本 | 說明 |
| --- | --- |
| **目前原始碼：v1.4.10** | Rust 編輯器與編譯器，搭配 C# 遊戲宿主；維持 `.lommod` v3 格式。 |
| **Windows 發行版：v1.4.10** | 預覽、單句氣泡、章節分組及節點備註。 |

[下載 v1.4.10（Windows x64）](https://github.com/mohui666/lom_modkit/releases/download/v1.4.10/lom_modkit-v1.4.10_windows_x64.zip) · [本版更新與驗證範圍（簡中）](RELEASE_NOTES_v1.4.10.md)

完整解壓縮後執行 `lom_modkit/lom-editor.exe`。退出遊戲，在「遊戲與 MOD」更新遊戲 Host 至 1.4.10；新功能的相容性需求會在匯出時寫入作品包。

## 1.4.10 新功能

- 對白支援單句表情氣泡，結束後自動恢復原設定。
- 改善氣泡、特效及長章節預覽效能，修正舊素材造成的人物放大。
- 章節可分組、折疊、重新命名與移動。
- 步驟優先顯示備註，空白時顯示節點編號。

遊戲 Host 需更新至 1.4.10。

## 執行目前版本

在儲存庫根目錄執行：

```sh
cargo run --locked -p lom-editor
```

macOS 應用程式建置：

```sh
scripts/build-macos.sh
```

產物為 `out/LoM Modkit Rust.app`。環境與素材設定見[目前 Mac 指南（簡中）](docs/chs/macos.md)。Windows 建置與 C# 宿主需求見 [Rust 建置文件（簡中）](docs/chs/rust.md)。

## 文件

**目前操作以簡中指南為準。** 繁中、日文及韓文的舊版 UI 指南尚未完整同步 Rust；其中的 Qt 選單與 Python 命令僅供歷史版本參考。

| 目前文件（簡中） | 內容 |
| --- | --- |
| [軟體使用](docs/chs/software_usage.md) | Rust 編輯器的操作入口與基本流程 |
| [目前能力](docs/chs/current_capabilities.md) | 已實作能力、限制與驗證範圍 |
| [Rust CLI](docs/chs/ai_cli.md) | `lomc` 命令列與受控編輯 API |
| [Mod 包格式契約](docs/chs/mod_format.md) | `.lommod` v3、67 種節點與 C# 宿主相容性 |
| [Rust 建置](docs/chs/rust.md) | Rust 元件、建置方式及遷移範圍 |

[繁中文件索引與歷史譯文](docs/cht/README.md) · [全部文件](docs/README.md)

## 授權

[MIT 授權](LICENSE)。粉絲自製工具，與遊戲開發商無關，不包含遊戲本體、解包素材或完整反編譯原始碼。範例僅示範工具能力。
