# lom_modkit

**《活俠傳》（Legend of Mortal）視覺化劇情 Mod 製作工具。**

編排人物對白、場景、分支與音訊，匯出 `.lommod`，由 C# 遊戲宿主載入。

> 語言：[中文（CHS）](README.md) · 中文（CHT，本文） · [日本語](README.ja.md) · [한국어](README.ko.md)

## 版本與下載

| 版本 | 說明 |
| --- | --- |
| **目前原始碼：v1.4.4** | Rust 編輯器與編譯器，搭配 C# 遊戲宿主；維持 `.lommod` v3 格式。 |
| **Windows 發行版：v1.4.4** | 預覽拖曳與四角等比縮放，保留原版人物補繪、完整遊玩包及視窗／DPI 適配。 |

[下載 v1.4.4（Windows x64）](https://github.com/mohui666/lom_modkit/releases/download/v1.4.4/lom_modkit-v1.4.4_windows_x64.zip) · [本版更新與驗證範圍（簡中）](RELEASE_NOTES_v1.4.4.md)

完整解壓縮後執行 `lom_modkit/lom-editor.exe`。退出遊戲，在「遊戲與 MOD」更新遊戲 Host 至 1.4.4；新功能的相容性需求會在匯出時寫入作品包。

## 1.4.4 新功能

在畫面預覽先勾選「调整画面」，再拖曳人物或自訂插圖主體改位置，拉四角等比縮放。「复位」恢復預設佈局，`Esc` 取消尚未放開滑鼠的拖曳；支援復原／重做及儲存後重開。修改屬於本次演出，素材本身的預設大小保持原值，新增佈局需要 Host 1.4.4。

包／劇情／內容格式維持 **3 / 2 / 1**；未填新欄位的既有步驟保留原有 Lua 呼叫。操作與實測範圍見上方本版說明。

## 1.4.3 新功能

- 在作品素材中為原版人物追加 `tired`、`sleep` 等表情或替換指定立繪，保留原人物 ID、姓名和其他表情。可選擇表情並預覽對應圖片，不需手動修改預覽快取。
- 人物、CG、疊加圖片及介紹卡圖片支援遊戲可表示的任意正數百分比與小數，取消原先的固定縮放上限；人物依完整站位矩形等比適配。
- 一鍵匯出完整遊玩 ZIP，包含作品、自有素材、Host、載入元件與安裝入口。首次匯出會下載缺少的載入元件；玩家解壓後雙擊「安装MOD.cmd」即可離線安裝，仍需本機已有正版遊戲。
- 編輯器保留系統 DPI，依視窗大小自動適配介面；設定中也可手動調整介面縮放。

操作細節見 [原版人物補繪與素材分享（簡中）](docs/chs/user_content.md)。遊玩包不包含遊戲本體或官方預覽圖片。

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
| [Mod 包格式契約](docs/chs/mod_format.md) | `.lommod` v3、67 種節點與 C# 宿主相容性 |
| [遷移與建置](docs/chs/rust_migration.md) | Rust 元件、建置方式及遷移範圍 |

[繁中文件索引與歷史譯文](docs/cht/README.md) · [全部文件](docs/README.md)

## 授權

[MIT 授權](LICENSE)。粉絲自製工具，與遊戲開發商無關，不包含遊戲本體、解包素材或完整反編譯原始碼。範例僅示範工具能力。
