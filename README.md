# lom_modkit — 活侠传（Legend of Mortal）Mod 工具

让玩家用图形编辑器自定义剧情（调用游戏内人物、场景、特效、音乐、数值），
导出 `.lommod` 包并互相分享；游戏内由 BepInEx 插件加载演出，支持"开始新战役"
（接管新游戏流程）与自由模式地图点位触发。

MIT 许可。本工具为粉丝自制工具，与游戏开发商无关，不包含游戏本体任何文件。

## 组件

- `compiler/`（`lomc`）— JSON 剧情 → 游戏原生 Lua 编译器（Python 标准库，包格式契约见 `docs/mod_format.md`）
- `editor/` — PySide6 图形编辑器（三栏：章节与步骤 / 属性 / 画面预览；常用操作工具栏、剧情检查、内置使用指南、多章节、撤销/重做）
- `editor/story_api.py` — AI/脚本可用的受控工具接口（Python API + CLI）：所有写操作经固定规则校验，AI 不直接手写 story JSON/Lua
- `runtime/MortalModHost/` — BepInEx 游戏内插件（C# net48）：扫描 `.lommod`、Harmony 拦截演出 mod Lua、战役模式（隔离存档槽）、位置触发器、已读文本注册（台词变黄/快进）、自定义死亡文本
- `tools/` — 从解包产物提取编辑器数据 / 预览素材 / 屏幕截图辅助脚本
- `data/` — 编辑器数据（`editor_data.json`：人物/表情/场景/音乐/属性/骰子检查点清单，schema 3）
- `samples/` — 示例 mod（demo_mod 全节点演示、snack_case 战役短剧《点心大盗疑案》、probe 诊断探针）

## 快速开始

### 1. 编译器（无依赖）

```bash
# 校验 / 编译 / 打包
PYTHONPATH=compiler python -m lomc check story.json
PYTHONPATH=compiler python -m lomc build story.json -o out.lua
PYTHONPATH=compiler python -m lomc pack  mod目录 -o 我的mod.lommod
```

### 2. 编辑器（PySide6）

```bash
cd editor
python -m venv .venv
.venv/Scripts/pip install PySide6
.venv/Scripts/python -m pip install imageio imageio-ffmpeg   # 可选：录制流程视频
.venv/Scripts/python editor/run_editor.bat                   # 或直接运行 run_editor.bat
```

### 3. 游戏内插件（BepInEx）

1. 给游戏装 BepInEx 6（Unity Mono x86），把 `runtime/MortalModHost/bin/Release/net48/MortalModHost.dll`
   放进 `BepInEx/plugins/MortalModHost/`
2. `.lommod` 包放进 `BepInEx/plugins/MortalModHost/mods/`
3. 进游戏：自由场景/标题画面左下角「活侠MOD」按钮或 F8 打开菜单 →「演出 mod 剧情」或「开始新战役」

### 4. 独立可执行文件（PyInstaller 打包，可选）

编辑器与 AI 命令行各出一个 exe，共享同一运行时目录，目标机器无需 Python：

```bash
cd editor
.venv/Scripts/pip install pyinstaller
.venv/Scripts/python build_exe.py
```

产物在 `editor/dist/lom_modkit/`（`build/`、`dist/` 已被 gitignore）：

| 文件 | 说明 |
| --- | --- |
| `lom_editor.exe` | 图形编辑器（无控制台窗口；数据清单内置，缺素材时用占位图；打开/保存默认从当前工作目录开始） |
| `story_api_cli.exe` | AI / 脚本友好的命令行（check / compile / pack / new-story） |

`story_api_cli` 用法（退出码 0/1，UTF-8；AI 建议加 `--json` 拿单行结构化结果）：

```bash
story_api_cli check story.json
story_api_cli check --json story.json            # {"ok": true, "errors": [], "warnings": []}
story_api_cli compile story.json -o out.lua
story_api_cli pack mod目录 -o 我的mod.lommod
story_api_cli new-story my_story -o story.json
```

`--json` 可放在子命令前或后；失败时同样输出 `{"ok": false, "errors": [...]}` 且退出码仍为 1。

## 开发

```bash
# 编译器测试（94+ 例）
cd compiler && python -m unittest tests.test_lomc

# 编辑器测试（冒烟/压力，offscreen 无头运行）
cd editor && .venv/Scripts/python tests/smoke_test.py
cd editor && .venv/Scripts/python tests/stress_test.py

# story_api 测试（36+ 例）
cd editor && .venv/Scripts/python tests/story_api_test.py

# 插件构建
cd runtime/MortalModHost && dotnet build -c Release

# 插件冒烟测试（ModLoader/MiniJson/ModRegistry 离线验证）
cd runtime/MortalModHost && dotnet run --project test/SmokeTest -c Release
```

游戏内调试：任意场景按 F7 切换「禁用原版剧情」全局临时开关（会话级，不持久化）。
开启后会跳过返回 Free 时自动触发、以及地点点击触发的官方主线、支线和地点默认脚本；mod 触发器仍优先。该开关只在本次游戏会话有效，再按 F7 或重启游戏即可恢复。
已经开始的 Story 演出不会被强制中断，F8 菜单不受影响。

## 说明与致谢

- `docs/mod_format.md` 是全部组件的契约（包格式、38+ 种节点、运行时行为），改代码先改它。
- `data/editor_data.json` 由 `tools/extract_editor_data.py` 从游戏的解包产物生成
  （解包目录用环境变量 `LOM_UNPACK_DIR` 指定）；仓库不包含解包产物与游戏文件。
- 游戏机制调研基于对官方脚本的实证分析（1814 个剧情脚本），反编译的游戏源码
  因版权原因不随仓库公开，仅留在本地 `docs/research/` 供研究。
- 示例 mod 仅演示工具能力，不含游戏原始剧情内容。
