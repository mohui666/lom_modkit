# lom_modkit — 活俠傳（Legend of Mortal）Mod 工具

讓玩家用圖形編輯器自訂劇情（呼叫遊戲內人物、場景、特效、音樂、數值），
匯出 `.lommod` 包並互相分享；遊戲內由 BepInEx 外掛載入演出，支援「開始新戰役」
（接管新遊戲流程）與自由模式地圖點位觸發。

MIT 授權。本工具為粉絲自製工具，與遊戲開發商無關，不包含遊戲本體任何檔案。

> 語言：[简体中文](README.md) · 繁體中文（本文） · [日本語](README.ja.md) · [한국어](README.ko.md)

## 組件

- `compiler/`（`lomc`）— JSON 劇情 → 遊戲原生 Lua 編譯器（Python 標準函式庫，包格式契約見 `docs/zh_TW/mod_format.md`）
- `editor/` — PySide6 圖形編輯器（三欄：劇情結構 / 目前物件 / 預覽；工具列只留試玩與匯出；F5 從目前步驟進遊戲、流程圖、體檢、安裝管理、已讀重置、多章節、復原/重做）
- `editor/story_api.py` — AI/腳本可用的受控工具介面（Python API + CLI）：所有寫入操作經固定規則校驗，AI 不直接手寫 story JSON/Lua
- `runtime/MortalModHost/` — BepInEx 遊戲內外掛（C# net48）：掃描 `.lommod`、Harmony 攔截演出、戰役隔離存檔、位置觸發器、已讀文本、人物介紹卡與死亡/結局文本；Steam 一般啟動可用
- `tools/` — 從解包產物提取編輯器資料 / 預覽素材 / 螢幕截圖輔助腳本
- `data/` — 編輯器資料（`editor_data.json`：人物/表情/場景/音樂/屬性/骰子檢查點清單，schema 3）
- `samples/` — 範例 mod（demo_mod、showcase、showcase2 全節點演示 2.0、snack_case《點心大盜疑案》、probe）

## 快速開始

### 1. 編譯器（無依賴）

```bash
# 校验 / 编译 / 打包
PYTHONPATH=compiler python -m lomc check story.json
PYTHONPATH=compiler python -m lomc build story.json -o out.lua
PYTHONPATH=compiler python -m lomc pack  mod目录 -o 我的mod.lommod
```

### 2. 編輯器（PySide6）

```bash
cd editor
python -m venv .venv
.venv/Scripts/pip install PySide6
run_editor.bat          # 或直接双击运行
```

### 3. 遊戲內外掛（BepInEx）

1. 編輯器選單「檔案 → 安裝管理」，選擇包含 `Mortal.exe` 的遊戲資料夾。
2. 點擊「安裝 BepInEx」，編輯器會從官方下載站安裝並校驗相容的 BepInEx 6 Mono x86 build 692；隨後自動安裝執行階段，並寫入 Steam 一般啟動修復（`version.dll` + `ignore_disable_switch`）。之後匯出的 `.lommod` 也會自動複製並啟用。
3. 若 Steam 點「開始」後標題沒有「活侠MOD」、F8 沒反應：在安裝管理裡點「修復 Steam 無法載入」，然後從 Steam **一般啟動**（不要系統管理員）。
4. 同一視窗可勾選啟用/停用已安裝 Mod。手動路徑仍為 `BepInEx/plugins/MortalModHost/mods/`。
5. 進遊戲：自由場景/標題畫面左下角「活侠MOD」按鈕或 F8 開啟選單 →「演出 mod 劇情」或「開始新戰役」。

複測已讀變黃時：先退出遊戲，再在編輯器「試玩 → 重置劇情已讀狀態」。它會同時清目前 mod 與 F5 試玩包（`lom_modkit_preview`）的記錄。

自訂音樂和音效：選單「檔案 → 使用者內容庫」匯入 `.ogg` / `.wav`（≤20MB），會得到穩定編號如 `user:mohui.battle`。在音樂/音效步驟裡按「使用者 / 官方」分組選擇；對白步驟可綁定「對白語音」。劇情只儲存這個編號。匯出時只打包目前 Mod 真正引用的音訊，換電腦安裝後不依賴作者本機內容庫。說明見 `docs/zh_TW/user_content.md`。

匯出前可按 F6 開啟「體檢」。它會檢查編譯錯誤、斷路與不可達步驟、佔位文字、圖片素材、使用者音訊引用，以及「人物未登場就做動作/說話」的黑畫面風險；雙擊問題可定位到對應步驟。「安全自動修復」只處理不會改變劇情含義的機械問題（含自動補人物登場），並支援復原。

除錯長劇情時，選中步驟後按 F5：編輯器會產生並安裝獨立暫存包，遊戲到達 Title/Free 安全場景後自動從該步驟開始；進入前會自動補上該步驟之前的舞台狀態（目前場景、台上人物的站位/表情/朝向），因此從劇情中途進入不會再因「角色不存在」黑畫面。暫存包不會覆蓋正式 Mod，讀入後自動刪除。右側「流程圖」顯示真實跳轉連線（一對多分支用不同顏色區分），斷路、無法結束的無窮迴圈和不可達步驟會用紅框與文字同時標出。

### 4. 獨立可執行檔（PyInstaller 打包，可選）

編輯器與 AI 命令列各出一個 exe，共用同一執行階段目錄，目標機器無需 Python：

```bash
cd editor
.venv/Scripts/pip install pyinstaller
.venv/Scripts/python build_exe.py
```

產物在 `editor/dist/lom_modkit/`（`build/`、`dist/` 已被 gitignore）：

| 檔案 | 說明 |
| --- | --- |
| `lom_editor.exe` | 圖形編輯器（無主控台視窗；資料清單內建，缺素材時用佔位圖；開啟/儲存預設從目前工作目錄開始） |
| `story_api_cli.exe` | AI / 腳本友善的命令列（check / compile / pack / new-story） |

`story_api_cli` 用法（退出碼 0/1，UTF-8；AI 建議加 `--json` 拿單行結構化結果）：

```bash
story_api_cli check story.json
story_api_cli check --json story.json            # {"ok": true, "errors": [], "warnings": []}
story_api_cli compile story.json -o out.lua
story_api_cli pack mod目录 -o 我的mod.lommod
story_api_cli new-story my_story -o story.json
```

`--json` 可放在子命令前或後；失敗時同樣輸出 `{"ok": false, "errors": [...]}` 且退出碼仍為 1。

面向 AI 代理的詳細手冊（環境需求、各子命令參數/輸出/退出碼、--json 欄位結構、Python API 速查、寫入操作硬性規則、錯誤對照表）見 `docs/zh_TW/ai_cli.md`。

## 開發

```bash
# 编译器测试（160 例）
cd compiler && python -m unittest tests.test_lomc

# 编辑器测试（冒烟/压力，offscreen 无头运行）
cd editor && .venv/Scripts/python tests/smoke_test.py
cd editor && .venv/Scripts/python tests/stress_test.py

# story_api / 登场防线测试（61 + 18 例）
cd editor && .venv/Scripts/python tests/story_api_test.py
cd editor && .venv/Scripts/python tests/stage_guard_test.py

# 插件构建
cd runtime/MortalModHost && dotnet build -c Release

# 插件冒烟测试（ModLoader/MiniJson/ModRegistry 离线验证）
cd runtime/MortalModHost && dotnet run --project test/SmokeTest -c Release
```

遊戲內除錯：任意場景按 F7 切換「停用原版劇情」全域暫時開關（工作階段級，不持久化）。
開啟後會跳過返回 Free 時自動觸發、以及地點點擊觸發的官方主線、支線和地點預設腳本；mod 觸發器仍優先。該開關只在本次遊戲工作階段有效，再按 F7 或重啟遊戲即可恢復。
已經開始的 Story 演出不會被強制中斷，F8 選單不受影響。

## 0.6.0

- 編輯器資訊架構：選單管低頻，工具列只留試玩/匯出；左欄只管章節與步驟，章節屬性進中欄；步驟兩行文案、右鍵刪除/移動；預覽對白按字數撐開並可中文換行。
- 已讀重置同時改 `Save_universe.dat` 與 `.json`，並清 F5 試玩包 `lom_modkit_preview` 的記錄。
- 音樂/環境音 `fadeout` 之後會 `wait` 滿淡出時長，避免下一句 `PlayMusic` 把音量瞬間拉回。
- Steam 一般啟動可載入 BepInEx（`version.dll` + `ignore_disable_switch`）。
- 範例 `showcase2`：場景一後半旁白與對話拆開；魏菊在切場/進第二幕前退場。

## 語言

編輯器選單「語言」可切換簡體中文、繁體中文、日語、韓語，偏好會記住。

遊戲內名詞按這個順序取：

1. [LoM-wiki](https://github.com/mohui666/LoM-wiki-CNS)（人物、門派、汗青書 / 生死簿、屬性等）
2. wiki 沒有的條目，用遊戲解包官方語言表（`lom_unpack/raw/*_zh-cn.txt` / `*_zh-tw.txt` / `*_kr.txt`）

官方遊戲介面只有繁中、簡中、韓語，**沒有日語**。日語人物名與屬性名來自 wiki 日文頁；韓語全文來自官方解包。遊戲內 Mod 選單會跟隨遊戲目前語言。

實作細節（目錄結構、回退規則、名詞再產生、如何加新語言）見 `docs/zh_TW/i18n.md`。

## 說明與致謝

- `docs/zh_TW/mod_format.md` 是全部組件的契約（包格式、43 種節點、使用者內容、執行階段行為），改程式碼先改它。自訂音訊用法見 `docs/zh_TW/user_content.md`。
- `data/editor_data.json` 由 `tools/extract_editor_data.py` 從遊戲的解包產物產生
  （解包目錄用環境變數 `LOM_UNPACK_DIR` 指定）；倉庫不包含解包產物與遊戲檔案。
- 遊戲機制調研基於對官方腳本的實證分析（1814 個劇情腳本），反編譯的遊戲原始碼
  因版權原因不隨倉庫公開，僅留在本地 `docs/research/` 供研究。
- 範例 mod 僅演示工具能力，不含遊戲原始劇情內容。
