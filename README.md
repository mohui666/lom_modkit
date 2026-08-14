# lom_modkit — 活侠传（Legend of Mortal）Mod 工具

让玩家用图形编辑器自定义剧情（调用游戏内人物、场景、特效、音乐、数值），
导出 `.lommod` 包并互相分享；游戏内由 BepInEx 插件加载演出，支持"开始新战役"
（接管新游戏流程）与自由模式地图点位触发。

MIT 许可。本工具为粉丝自制工具，与游戏开发商无关，不包含游戏本体任何文件。

## 组件

- `compiler/`（`lomc`）— JSON 剧情 → 游戏原生 Lua 编译器（Python 标准库，包格式契约见 `docs/mod_format.md`）
- `editor/` — PySide6 图形编辑器（三栏：剧情结构 / 当前对象 / 预览；工具栏只留试玩与导出；F5 从当前步骤进游戏、流程图、体检、安装管理、已读重置、多章节、撤销/重做）
- `editor/story_api.py` — AI/脚本可用的受控工具接口（Python API + CLI）：所有写操作经固定规则校验，AI 不直接手写 story JSON/Lua
- `runtime/MortalModHost/` — BepInEx 游戏内插件（C# net48）：扫描 `.lommod`、Harmony 拦截演出、战役隔离存档、位置触发器、已读文本、人物介绍卡与死亡/结局文本；Steam 普通启动可用
- `tools/` — 从解包产物提取编辑器数据 / 预览素材 / 屏幕截图辅助脚本
- `data/` — 编辑器数据（`editor_data.json`：人物/表情/场景/音乐/属性/骰子检查点清单，schema 3）
- `samples/` — 示例 mod（demo_mod、showcase、showcase2 全节点演示 2.0、snack_case《点心大盗疑案》、probe）

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
run_editor.bat          # 或直接双击运行
```

### 3. 游戏内插件（BepInEx）

1. 编辑器菜单“文件 → 安装管理”，选择包含 `Mortal.exe` 的游戏文件夹。
2. 点击“安装 BepInEx”，编辑器会从官方下载站安装并校验兼容的 BepInEx 6 Mono x86 build 692；随后自动安装运行时，并写入 Steam 普通启动修复（`version.dll` + `ignore_disable_switch`）。之后导出的 `.lommod` 也会自动复制并启用。
3. 若 Steam 点「开始」后标题没有「活侠MOD」、F8 没反应：在安装管理里点「修复 Steam 无法加载」，然后从 Steam **普通启动**（不要管理员）。
4. 同一窗口可勾选启用/停用已安装 Mod。手动路径仍为 `BepInEx/plugins/MortalModHost/mods/`。
5. 进游戏：自由场景/标题画面左下角「活侠MOD」按钮或 F8 打开菜单 →「演出 mod 剧情」或「开始新战役」。

复测已读变黄时：先退出游戏，再在编辑器「试玩 → 重置剧情已读状态」。它会同时清当前 mod 与 F5 试玩包（`lom_modkit_preview`）的记录。

导出前可按 F6 打开“体检”。它会检查编译错误、断路与不可达步骤、占位文字、图片素材，以及“人物未登场就做动作/说话”的黑屏风险；双击问题可定位到对应步骤。“安全自动修复”只处理不会改变剧情含义的机械问题（含自动补人物登场），并支持撤销。

调试长剧情时，选中步骤后按 F5：编辑器会生成并安装独立临时包，游戏到达 Title/Free 安全场景后自动从该步骤开始；进入前会自动补上该步骤之前的舞台状态（当前场景、台上人物的站位/表情/朝向），因此从剧情中途进入不会再因"角色不存在"黑屏。临时包不会覆盖正式 Mod，读入后自动删除。右侧“流程图”显示真实跳转连线（一对多分支用不同颜色区分），断路、无法结束的死循环和不可达步骤会用红框与文字同时标出。

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

面向 AI 代理的详细手册（环境要求、各子命令参数/输出/退出码、--json 字段结构、Python API 速查、写操作硬性规则、错误对照表）见 `docs/ai_cli.md`。

## 开发

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

游戏内调试：任意场景按 F7 切换「禁用原版剧情」全局临时开关（会话级，不持久化）。
开启后会跳过返回 Free 时自动触发、以及地点点击触发的官方主线、支线和地点默认脚本；mod 触发器仍优先。该开关只在本次游戏会话有效，再按 F7 或重启游戏即可恢复。
已经开始的 Story 演出不会被强制中断，F8 菜单不受影响。

## 0.6.0

- 编辑器信息架构：菜单管低频，工具栏只留试玩/导出；左栏只管章节与步骤，章节属性进中栏；步骤两行文案、右键删除/移动；预览对白按字数撑开并可中文换行。
- 已读重置同时改 `Save_universe.dat` 与 `.json`，并清 F5 试玩包 `lom_modkit_preview` 的记录。
- 音乐/环境音 `fadeout` 之后会 `wait` 满淡出时长，避免下一句 `PlayMusic` 把音量瞬间拉回。
- Steam 普通启动可加载 BepInEx（`version.dll` + `ignore_disable_switch`）。
- 样例 `showcase2`：场景一后半旁白与对话拆开；魏菊在切场/进第二幕前退场。

## 说明与致谢

- `docs/mod_format.md` 是全部组件的契约（包格式、43 种节点、运行时行为），改代码先改它。
- `data/editor_data.json` 由 `tools/extract_editor_data.py` 从游戏的解包产物生成
  （解包目录用环境变量 `LOM_UNPACK_DIR` 指定）；仓库不包含解包产物与游戏文件。
- 游戏机制调研基于对官方脚本的实证分析（1814 个剧情脚本），反编译的游戏源码
  因版权原因不随仓库公开，仅留在本地 `docs/research/` 供研究。
- 示例 mod 仅演示工具能力，不含游戏原始剧情内容。
