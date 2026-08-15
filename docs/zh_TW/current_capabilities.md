# 目前能力與邊界

本文只描述目前倉庫程式碼。節點權威集合是 `editor/models.py` 的 `NODE_SCHEMAS`，目前共 51 種。

## 已實作

- 劇情、舞台演出、多章節、本地化、自訂角色/音訊/背景/CG/Overlay 與自訂結局。
- `campaign.new_game` 的 `mod_<id>` 隔離存檔槽，以及 Manifest 地點/時間/Flag/好感觸發器。
- F5 試玩、熱重載/Debugger、Editing/Release 體檢、恢復副本、範本、統計、語音覆蓋、本機 Release Builder、安裝診斷與 Runtime 回滾。
- 強制非官方披露、包指紋與離線截圖/影片來源水印檢測；它們不是作者簽章。

## 戰鬥邊界

`enemy`、`battle_skill` 和 `goto_scene` 的 `Combat` / `Battle` 會呼叫已驗證的《活俠傳》原版 API。高層 `combat` / `battle` 可選原版模板；前者取得 Combat win/lose，後者只接受 finish=true 的 FriendWin/EnemyWin。這不是自製戰鬥引擎，且新增流程尚未實機驗證。

章節設定已支援 Battle Preset，可重用原版 `combat` / `battle` 模板及已驗證敵方參數；`battle_result` 只按同包同劇情的真實 win/lose 分支；`battle_setup` 與 `reward` 只聚合既有原版／原子介面。仍沒有 draw/escape 回呼、任意商品 Custom Shop、獨立 `mod_quest` 或任意持久化 Mod 變數。`modflags` / `modvars` 僅在 Story 工作階段存在；`game_flag` 只可寫原版已存在的 FlagData。也不支援自訂戰鬥地圖、模型、AI、動畫或機制。

尚未由反編譯與實機驗證確認的遊戲介面不得靠猜測實作。
