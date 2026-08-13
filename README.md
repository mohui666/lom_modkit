# lom_modkit — 活侠传（Legend of Mortal）Mod 工具

让玩家用图形编辑器自定义剧情（调用游戏内人物、场景、特效、音乐、数值），
导出 `.lommod` 包并互相分享；游戏内由 BepInEx 插件加载，通过新增入口演出。

## 目录

- `docs/mod_format.md` — **mod 包格式契约（所有组件以它为准）**
- `docs/research/` — 游戏反编译调研笔记
- `runtime/MortalModHost/` — BepInEx 游戏内加载器（C#）
- `compiler/` — JSON 剧情 → 游戏原生 Lua 编译器（Python，包名 `lomc`）
- `editor/` — PySide6 图形编辑器（三栏：脚本切换+节点列表 / 属性表单 / 演出预览+Lua 预览）
- `tools/` — 从 lom_unpack 解包产物提取编辑器数据的脚本
- `data/` — 编辑器数据（`editor_data.json`：人物/表情/场景/音乐/特效/属性清单）
- `samples/` — 示例 mod

### 编辑器特性（editor/）

- **多剧情脚本管理**：项目 = 多个 story + manifest（对应 .lommod 包）。左栏脚本切换条（新建/删除/改名），导入 .lommod 载入包内全部剧情，导出打包全部剧情；`end.next_script` 用包内脚本下拉。
- **撤销/重做**（Ctrl+Z / Ctrl+Y）：整项目快照式，连续输入暂停 600ms 合并为一步，跨剧情切换可撤销。
- **脏标记**：标题 `*`，关闭/新建/打开/导入前弹确认（导出保存/放弃/取消）；单剧情保存只清当前脚本脏标记。

## 数据流

```
编辑器(PySide6) ──导出──> .lommod(zip: manifest.json + story/*.json + lua/*.lua)
                                │                        ▲
                                │              compiler(lomc): story.json → lua
                                ▼
游戏: BepInEx 插件 MortalModHost 扫描 mods/*.lommod → 注册 Lua → 新增入口演出
```

解包数据来源：`C:/Users/mohui666/lom_unpack`（raw/ 文本表、raw_scripts/ 1814 个原生 Lua 剧本、output/csv/ 分类 CSV）。
游戏目录：`C:/Program Files (x86)/Steam/steamapps/common/LegendOfMortal`（Unity Mono，已装 BepInEx 5）。
