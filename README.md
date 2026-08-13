# lom_modkit — 活侠传（Legend of Mortal）Mod 工具

让玩家用图形编辑器自定义剧情（调用游戏内人物、场景、特效、音乐、数值），
导出 `.lommod` 包并互相分享；游戏内由 BepInEx 插件加载演出，支持"开始新战役"
（接管新游戏流程）与自由模式地图点位触发。

MIT 许可。本工具为粉丝自制工具，与游戏开发商无关，不包含游戏本体任何文件。

## 组件

- `compiler/`（`lomc`）— JSON 剧情 → 游戏原生 Lua 编译器（Python 标准库，包格式契约见 `docs/mod_format.md`）
- `editor/` — PySide6 图形编辑器（三栏：脚本切换+节点列表 / 属性表单 / 演出预览+Lua 预览；多剧情项目、撤销/重做、脏标记）
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
```

## 说明与致谢

- `docs/mod_format.md` 是全部组件的契约（包格式、38+ 种节点、运行时行为），改代码先改它。
- `data/editor_data.json` 由 `tools/extract_editor_data.py` 从游戏的解包产物生成
  （解包目录用环境变量 `LOM_UNPACK_DIR` 指定）；仓库不包含解包产物与游戏文件。
- 游戏机制调研基于对官方脚本的实证分析（1814 个剧情脚本），反编译的游戏源码
  因版权原因不随仓库公开，仅留在本地 `docs/research/` 供研究。
- 示例 mod 仅演示工具能力，不含游戏原始剧情内容。
