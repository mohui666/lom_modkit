# 使用者內容庫（User Content Library）

> 語言：[简体中文](../zh_CN/user_content.md) · 繁體中文（本文） · [日本語](../ja/user_content.md) · [한국어](../ko/user_content.md)

本機、離線、按 Mod 自包含。沒有帳號、沒有線上市集、沒有雲端同步。

## 怎麼匯入

1. 編輯器選單 **檔案 → 使用者內容庫**。
2. 點「匯入音訊…」，選擇本機 `.ogg` 或 `.wav`（單條不超過 20MB）。
3. 填寫顯示名稱、命名空間、內部名稱和用途（音樂 / 音效 / 環境音）。
4. 得到穩定編號，例如匯入 `battle.ogg` 後：

```text
user:mohui.battle
```

命名空間預設取本機使用者名稱的安全形式，可改。同一編號再匯入會被拒絕，避免悄悄覆蓋。

## 支援哪些格式

| 格式 | 說明 |
| --- | --- |
| `.ogg` | Vorbis。第一次播放可能有很短的載入延遲。 |
| `.wav` | PCM 8/16 位元或 32 位元 float。常見 WAV 可立即播放。 |

不支援 mp3 / flac / aac。檔案大小上限 20MB。

## ID 是什麼

劇情裡儲存的是引用，不是檔案路徑。

- 官方曲目 / 音效：繼續寫原來的名字，例如 `普通_001`、`鈴鐺_001`。
- 使用者內容：必須是 `user:<命名空間>.<名稱>`，只含小寫字母、數字、底線。

顯示名稱可以是中文（「決戰曲」）；內部 ID 不能含空格、中文或路徑。

## 使用者內容儲存在哪裡

開發時在本機：

```text
%APPDATA%/lom_modkit/repository/audio/<id>/
  content.json
  音频文件
```

這只是編輯器倉庫。換電腦、只拷 `.lommod` 的玩家不需要這個目錄。

人物介紹圖 / 結局插圖仍在 `%APPDATA%/lom_modkit/assets/`，劇情裡寫 `assets/檔名.png`，與本系統分開，以免破壞已有 Mod。

## Story 儲存的是什麼

音樂 / 音效步驟的 `name`：

```json
{ "type": "music", "name": "user:mohui.battle" }
```

禁止儲存 `C:\Users\...\battle.ogg`。

## 匯出後還依賴本機內容庫嗎？

不依賴。匯出只複製**當前劇情真正引用**的使用者音訊進 `.lommod`：

```text
assets/user/audio/mohui.battle/content.json
assets/user/audio/mohui.battle/battle.ogg
```

沒被引用的匯入音訊不會打進包。引用缺失、類型不對、檔案壞了：匯出直接失敗。

## 如何分享 Mod

把匯出的 `.lommod` 發給別人即可。對方安裝後由遊戲外掛從包內播放。對方電腦上沒有你的使用者內容庫也能聽。

用編輯器「匯入 Mod」開啟別人的包時，包內使用者音訊會登記進你的本機倉庫，方便繼續改。

## 刪除資源有什麼限制

使用者內容庫裡可以刪。如果當前專案的某個音樂/音效步驟還在用這條內容，刪除會被阻止，並指出是哪一章哪一步。

## 對白語音

`say` 可加可選欄位 `voice`，值必須是使用者內容引用：

```json
{ "type": "say", "character": "player", "text": "师兄，早。", "voice": "user:mohui.line_01" }
```

沒有 `voice` 的對白與以前完全一樣。有則進入這句時停掉上一句語音並播放，玩家點下一句或劇情結束/中斷時停止。普通音效節點不會打斷對白語音。

在編輯器對白步驟裡選「對白語音」，或點「匯入…」；「清除」去掉綁定。

## 執行階段行為

- 官方名字：原版 Wwise，行為與以前完全相同。
- `user:`：只從**當前正在演出的那個 .lommod** 裡找。另一個 Mod 登記了同名 ID 也不會串音。
- 自訂音訊用 Windows `waveOut` 播放，不走 Unity `AudioSource`，也不走 Wwise。本遊戲主混音是 Wwise，Unity 播了經常沒聲。
- 音量大致跟隨遊戲的主音量 × 音樂/音效滑桿；不是 Wwise RTPC，不能做到完全一致。
- 自訂 fadeout 是輸出音量漸弱（隨後仍會按節點等待）。
- 切到自訂音樂時會先停官方背景樂；官方 `StopMusic` 本來就會把環境音一起清掉。
- 自訂角色走獨立 Runtime（`mod_char_*`），不註冊進原版 Addressables。第一版支援 show / say / hide / move / face / focus。詳見簡中文檔。
- 可用 `samples/audio_test/` 做驗收：自己匯入 `user:test.bgm` / `user:test.sfx` / `user:test.env`。
