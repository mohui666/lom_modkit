# lom_modkit

**《活俠傳》（Legend of Mortal）視覺化劇情 Mod 製作工具。**

不用寫 Lua。用圖形編輯器編排人物對白、場景演出、分支劇情、音樂音效，
一鍵匯出 `.lommod`，直接在遊戲中執行。

[![Release v1.1.0](https://img.shields.io/badge/release-v1.1.0-blue)](https://github.com/mohui666/lom_modkit/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-lightgrey)](#相容性)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)

**[⬇ 下載 Windows 版](https://github.com/mohui666/lom_modkit/releases/download/v1.1.0/lom_modkit-v1.1.0_windows_x64.zip)** ·
[快速開始](#快速開始) ·
[文件](docs/cht/README.md)

> 語言：[中文（CHS）](README.md) · 中文（CHT，本文） · [日本語](README.ja.md) · [한국어](README.ko.md)

<!-- TODO(宣傳素材): 在這裡放 8~15 秒循環 GIF：
     開啟 lom_editor.exe → 新增劇情 → 選角色/對白/場景/音樂 → F5 → 遊戲內實際演出。
     建議路徑 docs/assets/screenshots/demo.gif，然後用 ![demo](docs/assets/screenshots/demo.gif) 取代本註解。 -->

## 這是什麼

lom_modkit 讓你用《活俠傳》**原有的人物、場景、音樂、特效與數值系統**製作原創劇情：
在圖形編輯器裡點選設定，匯出獨立的 `.lommod` Mod 包，由遊戲內外掛載入演出。
支援「開始新戰役」（接管新遊戲流程，獨立存檔槽）與自由模式地圖點位觸發。

粉絲自製工具，與遊戲開發商無關，不包含遊戲本體任何檔案。MIT 授權。

## 能做什麼

- **視覺化劇情編輯**：人物、對白、表情、站位、場景、音樂、音效、特效全部透過 UI 設定。
- **分支劇情**：選項、條件分支、屬性判定、骰子檢定、多章節鏈式腳本。
- **劇情內容本地化**：同一 Story 可維護簡中、繁中、日文、韓文譯文，支援預設語言與缺漏回退；舊專案無需遷移。
- **直接呼叫遊戲內容**：使用遊戲現有人物、場景、音樂與演出系統，不需要自己重做一套。
- **自訂音訊與對白語音**：匯入 `.ogg` / `.wav`，可用作音樂、音效，以及逐句的角色對白語音。
- **一鍵遊戲內試玩**：選取任意劇情步驟按 F5，直接從該步驟進遊戲測試。
- **真正的 Mod 包**：匯出的 `.lommod` 自包含，可以直接分享給其他玩家。

## 快速開始

### 1. 下載

下載 [lom_modkit-v1.1.0_windows_x64.zip](https://github.com/mohui666/lom_modkit/releases/download/v1.1.0/lom_modkit-v1.1.0_windows_x64.zip) 並解壓縮。無需安裝 Python。

### 2. 啟動

執行 `lom_editor.exe`。

### 3. 連線《活俠傳》

功能表「檔案 → 安裝管理」，選擇包含 `Mortal.exe` 的遊戲資料夾，點「安裝 BepInEx」——
編輯器會自動下載安裝相容的 BepInEx 6 與遊戲內執行階段，並寫入 Steam 一般啟動修復。

### 4. 做第一段劇情

新增劇情 → 新增角色 → 新增對白 → 按 **F5** 試玩。

### 5. 匯出

匯出得到 `xxx.lommod`，傳給別人即可（對方同樣需要本工具安裝的執行階段）。

## 截圖

<!-- TODO(宣傳素材): 至少四張圖，建議放 docs/assets/screenshots/：
     ① 主編輯器全貌 ② 劇情流程圖 ③ 使用者音訊/對白語音 ④ 遊戲內實際效果。
     取消註解並取代路徑：
![劇情編輯器](docs/assets/screenshots/editor.png)
![遊戲內效果](docs/assets/screenshots/ingame.png)
![分支與流程圖](docs/assets/screenshots/flow_graph.png)
-->

## 為劇情製作設計的工作流程

### 從任意位置試玩（F5）

選取一個步驟按 **F5**：編輯器產生獨立臨時包，遊戲到達安全場景後自動從該步驟開始，
進入前自動補上該步驟之前的舞台狀態（目前場景、台上人物的站位/表情/朝向）——
從劇情中途進入不會再因「角色不存在」黑屏。臨時包不覆蓋正式 Mod，讀入後自動刪除。

### 匯出前體檢（F6）

按 **F6** 檢查：編譯錯誤、斷路與不可達步驟、死循環、佔位文字、缺少素材、
錯誤的使用者音訊參照、「人物未登場就說話/行動」的黑屏風險。雙擊問題可定位到對應步驟；
「安全自動修復」只處理不改變劇情含義的機械問題（含自動補人物登場），支援復原。

### 劇情流程圖

右側「流程圖」顯示真實跳轉連線（一對多分支用不同顏色區分），斷路、
無法結束的死循環和不可達步驟會用紅框與文字同時標出。

## 使用者內容

把本機角色、音訊或圖片匯入「使用者內容庫」（功能表「檔案 → 使用者內容庫」），得到穩定編號
（如 `user:mohui.battle`），在劇情步驟裡按「使用者 / 官方」分組選擇。劇情只儲存編號；
匯出時只打包目前 Mod 真正參照的內容，玩家機器不依賴作者本機內容庫。

| 內容類型 | 狀態 |
| --- | --- |
| 自訂音樂 / 音效 / 環境音 | ✅ 已支援 |
| 角色對白語音 | ✅ 已支援 |
| 自訂人物立繪 / 稱號 / 介紹卡 / 體型 | ✅ 已支援 |
| 自訂背景 / CG / Overlay 圖片 | ✅ 已支援 |
| 社群內容庫 | ◯ Roadmap |

詳細用法見 [使用者內容庫文件](docs/cht/user_content.md)。

## 安裝別人做的 Mod

把 `.lommod` 交給編輯器「檔案 → 安裝管理」安裝並勾選啟用即可
（手動路徑：`BepInEx/plugins/MortalModHost/mods/`）。
進遊戲後點自由場景/標題畫面左下角「活俠MOD」按鈕（或按 F8），
選擇「演出 mod 劇情」或「開始新戰役」。遊戲內選單會跟隨遊戲目前語言。

## 相容性

| 項目 | 狀態 |
| --- | --- |
| Windows 10/11 | ✅ |
| Steam《活俠傳》 | ✅（含一般啟動修復） |
| BepInEx | 編輯器自動安裝 |
| Python | Windows 發行版無需 |
| 修改遊戲原檔 | 不需要 |

## 目前版本

**v1.1.0**：完善原版風格 MOD 存檔頁與每個 MOD 的 001～020 獨立欄位；修正 Combat/Battle 讀檔、回合、血量重複累加與劇情背景恢復；新增趙活基準屬性/天賦覆寫並保留原版體力與被動加成。

完整變更見 [1.1.0 Release Notes](RELEASE_NOTES_v1.1.0.md)。歷史版本見 [Release Notes](https://github.com/mohui666/lom_modkit/releases)。

**v1.0.0**：統一 Editor/Runtime 版本 · `.lommod` v2 嚴格驗證與 Story/Lua 一致性 ·
Lua 環境隔離與完整生命週期清理 · 一次性快捷鍵遷移 · Runtime 自動化測試與 CI。

**v0.7.0**：自訂角色立繪 · 對白語音歸屬 · 介紹卡與稱號 · 體型滑桿 ·
離場清台 · 節點按類型編號。

完整變更見 [Release Notes](https://github.com/mohui666/lom_modkit/releases)。

## Roadmap

- 社群內容倉庫（分享/重複使用使用者內容）
- 面向作者的戰鬥 / 戰役編排層（目前僅有已驗證的底層節點）

## 文件

| 文件 | 內容 |
| --- | --- |
| [文件索引](docs/cht/README.md) | 語言導覽與讀者導引 |
| [使用者內容庫](docs/cht/user_content.md) | 自訂音訊 / 對白語音用法 |
| [目前能力與邊界](docs/cht/current_capabilities.md) | 已實作、僅有底層介面和尚未實作的功能邊界 |
| [Mod 包格式契約](docs/cht/mod_format.md) | 包結構、62 種節點、編譯約定、執行階段行為 |
| [AI / CLI 手冊](docs/cht/ai_cli.md) | story_api 命令列與 Python API |
| [多語言](docs/cht/i18n.md) | 介面與文件的 i18n 架構 |

## For Developers

### 架構

```text
┌─────────────┐
│ lom_editor  │  PySide6 图形编辑器
└──────┬──────┘
       │ story JSON
       ▼
┌─────────────┐
│    lomc     │  JSON → 游戏原生 Lua 编译器（纯标准库）
└──────┬──────┘
       │ Lua + assets
       ▼
┌─────────────┐
│   .lommod   │  自包含 Mod 包（zip）
└──────┬──────┘
       ▼
┌──────────────────┐
│ MortalModHost    │  BepInEx 游戏内插件（C# net48）
└──────┬───────────┘
       ▼
  Legend of Mortal
```

### 原始碼目錄

- `compiler/`（`lomc`）— JSON 劇情 → 遊戲原生 Lua 編譯器
- `editor/` — PySide6 圖形編輯器；`editor/story_api.py` 為 AI/腳本受控介面（Python API + CLI）
- `runtime/MortalModHost/` — BepInEx 遊戲內外掛
- `tools/` — 從解包產物擷取編輯器資料/素材的腳本
- `data/` — 編輯器資料（`editor_data.json`，schema 3）
- `samples/` — 範例 mod（demo_mod、showcase、showcase2 全節點演示 2.0、snack_case《點心大盜疑案》、probe）

### 從原始碼執行

```bash
# 编辑器
cd editor
python -m venv .venv
.venv/Scripts/pip install PySide6
run_editor.bat

# 编译器（无依赖）
PYTHONPATH=compiler python -m lomc check story.json
PYTHONPATH=compiler python -m lomc pack mod目录 -o 我的mod.lommod
```

### 建置與測試

```bash
# 编译器测试（160 例）
cd compiler && python -m unittest tests.test_lomc

# 编辑器测试（冒烟/压力，offscreen 无头运行）
cd editor && .venv/Scripts/python tests/smoke_test.py
cd editor && .venv/Scripts/python tests/stress_test.py

# story_api / 登场防线测试（61 + 18 例）
cd editor && .venv/Scripts/python tests/story_api_test.py
cd editor && .venv/Scripts/python tests/stage_guard_test.py

# 插件构建与冒烟测试
cd runtime/MortalModHost && dotnet build -c Release
cd runtime/MortalModHost && dotnet run --project test/SmokeTest -c Release
```

建置 Windows 發行版：先正式建置 Runtime，再執行 `editor/.venv/Scripts/python editor/build_exe.py`，最後執行 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-windows.ps1`。腳本會核對版本與必要檔案，並拒絕快取、範例包、使用者設定及符號連結混入；同名產物預設不覆寫，確認替換時才加 `-Force`。
（產物在 `editor/dist/lom_modkit/`，含 `lom_editor.exe` 與 `story_api_cli.exe`）。

遊戲內除錯：任意場景按 **F7** 切換「停用原版劇情」工作階段級開關（不持久化）；
複測已讀變黃時用編輯器「試玩 → 重設劇情已讀狀態」。

## FAQ

**Q：Steam 點「開始」後沒有「活俠MOD」按鈕、F8 沒反應？**
在編輯器「檔案 → 安裝管理」裡點「修復 Steam 無法載入」，然後從 Steam **一般啟動**（不要系統管理員）。

**Q：需要裝 Python 嗎？**
不需要。Windows 發行版是獨立 exe。只有從原始碼執行/開發才需要 Python 3.10+ 與 .NET（建置外掛）。

**Q：Mod 會修改我的遊戲檔案或存檔嗎？**
不會修改官方腳本與文字表。「開始新戰役」使用 `mod_campaign_<campaign_id>` 命名空間下的獨立欄位與自動檔，不覆蓋你的正常存檔。

**Q：做好的 Mod 可以發給別人嗎？**
可以。匯出的 `.lommod` 自包含（含參照的音訊/圖片），對方用本工具裝好執行階段即可遊玩。

## 授權與聲明

MIT 授權（[LICENSE](LICENSE)）。粉絲自製工具，與遊戲開發商無關，不包含遊戲本體任何檔案。

- 遊戲機制調研基於對官方腳本的實證分析（1814 個劇情腳本）；反編譯原始碼因版權原因不隨倉庫公開。
- `data/editor_data.json` 由 `tools/extract_editor_data.py` 從解包產物產生；倉庫不包含解包產物與遊戲檔案。
- 範例 mod 僅演示工具能力，不含遊戲原始劇情內容。
