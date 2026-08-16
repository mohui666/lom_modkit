# 腳本 / API 文件

編輯器「說明 → 文件 → 腳本 / API 文件」由實際 schema 動態產生，逐一列出 63 種節點的 JSON 鍵、介面含義、必填狀態、型別/列舉、預設值、最小範例與執行階段介面。

- [Mod v3 格式與編譯契約](mod_format.md)
- [story_api / CLI](ai_cli.md)
- [多語言契約](i18n.md)
- [目前能力與邊界](current_capabilities.md)

`combat` 是一對一 Combat 對手覆寫；`battle` 是多人 Battle 三方陣容覆寫。`battle_setup` 僅為舊版 Battle 相容節點。官方資源欄位顯示可讀名稱但只儲存穩定 ID；使用者內容統一使用 `user:<id>`。
