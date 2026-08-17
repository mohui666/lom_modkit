# 目前能力與邊界

本文只描述目前倉庫程式碼。節點權威集合是 `editor/models.py` 的 `NODE_SCHEMAS`，目前共 62 種。

## 已實作

- 劇情、舞台演出、多章節、本地化、自訂角色/音訊/背景/CG/Overlay 與自訂結局。
- 標題「開始 MOD 戰役」重用原版讀檔槽；MOD 手動槽、三類自動槽、Universe 最近槽和持久變數均與原版隔離。Manifest 支援地點／時間／Flag／好感觸發器。
- F5 試玩、熱重載/Debugger、Editing/Release 體檢、恢復副本、範本、統計、語音覆蓋、本機 Release Builder、安裝診斷與 Runtime 回滾。
- 強制非官方披露、包指紋與離線截圖/影片來源水印檢測；它們不是作者簽章。

## 戰鬥邊界

`combat` 可設定本次對手 HP、氣力、內力、內功、其他能力、天賦與行動機率；原版從不讀取的三格絕招欄位已刪除，只保留實際生效的 `ultimate_rate`。`battle` 可分別重用原版我方、敵方、中立陣容並設定人數、NPC HP 和玩家技能。前者取得 Combat win/lose，後者取得原版 Battle 的 finish=true FriendWin/EnemyWin。原版資產不被修改，這也不是自製戰鬥引擎；新增流程仍需實機驗收。

底層 `enemy`、`battle_skill` 與 `goto_scene` 仍保留供相容和進階編排使用。

`combat` 的人物只決定決鬥名稱與四類戰鬥動畫，背景從官方 `views` 獨立選擇，血量、屬性、技能與行動機率自由填寫；`battle` 只設定敵我陣營、總人數與已核實的官方具名角色，具名角色計入總人數。舊預設與 `battle_setup` 已刪除。`battle_result` 只按真實 win/lose 分支。`persistent_var` / `persistent_check` 綁定 `mod_campaign_<campaign_id>` 隔離槽；不修改原版 GameSave。

底層 `enemy` / `battle_skill` 仍可獨立使用；Combat / Battle 結果可配合 `reward`、`result_screen`、`custom_shop`、`mod_quest` 等現有節點編排。

尚未由反編譯與實機驗證確認的遊戲介面不得靠猜測實作。
