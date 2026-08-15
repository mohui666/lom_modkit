# lom_modkit 文件

> 語言：[简体中文](../README.md) · 繁體中文（本文） · [日本語](../ja/README.md) · [한국어](../ko/README.md)

權威版為簡體中文（`zh_CN/`），譯文按語言碼分目錄、同名存放。
改文件先改 `zh_CN/`，再同步譯文；約定見 [i18n.md §4](i18n.md)。

## 按讀者找文件

| 文件 | 讀者 | 內容 |
| --- | --- | --- |
| [mod_format](mod_format.md) | 全部元件開發者 | **v3 契約**：包結構、45 種節點、story→Lua 編譯約定、editor_data、執行階段行為、story_api 契約、使用者內容協定。改程式碼先改它 |
| [ai_cli](ai_cli.md) | AI 代理 / 腳本作者 | story_api 操作手冊：CLI 子命令、--json 欄位、Python API 速查、硬性規則、錯誤對照表 |
| [user_content](user_content.md) | mod 作者 | 使用者內容庫：匯入自訂音訊、對白語音、匯出與分享、執行階段行為 |
| [i18n](i18n.md) | 維護者 | 多語言架構：編輯器介面、遊戲內 Mod 選單、名詞對照表再產生、文件翻譯約定 |

`research/` 是逆向研究材料（反編譯腳本等），僅存檔，不翻譯。
