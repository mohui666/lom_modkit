# 腳本 / API 文件

> **版本提醒：** 本頁尚未全面同步目前的 Rust 版本；舊版 UI 選單與實作描述僅供歷史參考。請以[最新簡體中文版](../chs/script_reference.md)為準。

編輯器「說明 → 文件 → 腳本 / API 文件」由實際 schema 動態產生，逐一列出 63 種節點的 JSON 鍵、介面含義、必填狀態、型別/列舉、預設值、最小範例與執行階段介面。

- [Mod v3 格式與編譯契約](mod_format.md)
- [story_api / CLI](ai_cli.md)
- [多語言契約](i18n.md)
- [目前能力與邊界](current_capabilities.md)

`combat` 的人物只決定決鬥名稱與戰鬥動畫，背景從官方 `views` 獨立選擇，其他參數自由填寫；`battle` 只設定敵我陣營、總人數與已核實的官方具名角色。舊預設與 `battle_setup` 已刪除。使用者內容統一使用 `user:<id>`。
