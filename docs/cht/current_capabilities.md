# 目前能力與邊界

本文只描述目前倉庫程式碼。節點權威集合是 `editor/models.py` 的 `NODE_SCHEMAS`，目前共 63 種。

## 已實作

- 劇情、舞台演出、多章節、本地化、自訂角色/音訊/背景/CG/Overlay 與自訂結局。
- 標題「開始 MOD 戰役」重用原版讀檔槽；MOD 手動槽、三類自動槽、Universe 最近槽和持久變數均與原版隔離。Manifest 支援地點／時間／Flag／好感觸發器。
- F5 試玩、熱重載/Debugger、Editing/Release 體檢、恢復副本、範本、統計、語音覆蓋、本機 Release Builder、安裝診斷與 Runtime 回滾。
- 強制非官方披露、包指紋與離線截圖/影片來源水印檢測；它們不是作者簽章。

## 戰鬥邊界

`combat` 可用原版一對一人物／場景範本並覆寫本次對手 HP、體力、能力、天賦、絕招與行動機率；`battle` 可分別重用原版我方、敵方、中立陣容並設定人數、NPC HP 和玩家技能。前者取得 Combat win/lose，後者取得原版 Battle 的 finish=true FriendWin/EnemyWin。原版資產不被修改，這也不是自製戰鬥引擎；新增流程仍需實機驗收。

底層 `enemy`、`battle_skill` 與 `goto_scene` 仍保留供相容和進階編排使用。

`combat` / `battle` 節點直接設定所有已驗證參數，不再使用工具預設。`key` 只選擇原版角色／場景底板，血量、屬性、行動機率、三方陣容、人數、NPC 血量與戰場技能均寫在目前節點。`battle_result` 只按同包同劇情的真實 win/lose 分支；`battle_setup`、`reward` 與 `activity` 只聚合既有原版／原子介面。`result_screen` 使用原版 `mainui.DisplayMessageText` 顯示結算標題與說明，再以 `reward` 的既有介面發放獎勵，不建立新 UI。`custom_shop` 可暫時替換原版三類商店庫存並使用數量、條件與統一折扣，但不偽造逐商品價格。`mod_quest` / `quest_check` 是 Host 戰役工作階段狀態；`persistent_var` / `persistent_check` 則把 Int32 狀態原子保存到與 `mod_<id>` 隔離槽綁定的 Host sidecar，不修改原版 GameSave。任意 Lua 物件仍不持久化。也不支援 draw/escape、自訂戰鬥地圖、模型、AI、動畫或機制。

尚未由反編譯與實機驗證確認的遊戲介面不得靠猜測實作。
