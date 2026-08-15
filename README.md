# lom_modkit

**《活侠传》（Legend of Mortal）可视化剧情 Mod 制作工具。**

不用写 Lua。用图形编辑器编排人物对白、场景演出、分支剧情、音乐音效，
一键导出 `.lommod`，直接在游戏中运行。

[![Release v0.7.0](https://img.shields.io/badge/release-v0.7.0-blue)](https://github.com/mohui666/lom_modkit/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-lightgrey)](#兼容性)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)

**[⬇ 下载 Windows 版](https://github.com/mohui666/lom_modkit/releases/download/v0.7.0/lom_modkit-v0.7.0_windows_x64.zip)** ·
[快速开始](#快速开始) ·
[文档](docs/README.md)

> 语言：简体中文（本文） · [繁體中文](README.zh_TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

<!-- TODO(宣传素材): 在这里放 8~15 秒循环 GIF：
     打开 lom_editor.exe → 新建剧情 → 选角色/对白/场景/音乐 → F5 → 游戏内实际演出。
     建议路径 docs/assets/screenshots/demo.gif，然后用 ![demo](docs/assets/screenshots/demo.gif) 替换本注释。 -->

## 这是什么

lom_modkit 让你用《活侠传》**原有的人物、场景、音乐、特效与数值系统**制作原创剧情：
图形编辑器里点选配置，导出独立的 `.lommod` Mod 包，由游戏内插件加载演出。
支持「开始新战役」（接管新游戏流程，隔离存档槽）与自由模式地图点位触发。

粉丝自制工具，与游戏开发商无关，不包含游戏本体任何文件。MIT 许可。

## 能做什么

- **可视化剧情编辑**：人物、对白、表情、站位、场景、音乐、音效、特效全部通过 UI 配置。
- **分支剧情**：选项、条件分支、属性判定、骰子检定、多章节链式脚本。
- **直接调用游戏内容**：使用游戏现有人物、场景、音乐与演出系统，不需要自己重做一套。
- **自定义音频与对白语音**：导入 `.ogg` / `.wav`，可用作音乐、音效，以及逐句的角色对白语音。
- **一键游戏内试玩**：选中任意剧情步骤按 F5，直接从该步骤进游戏测试。
- **真正的 Mod 包**：导出的 `.lommod` 自包含，可以直接分享给其他玩家。

## 快速开始

### 1. 下载

下载 [lom_modkit-v0.7.0_windows_x64.zip](https://github.com/mohui666/lom_modkit/releases/download/v0.7.0/lom_modkit-v0.7.0_windows_x64.zip) 并解压。无需安装 Python。

### 2. 启动

运行 `lom_editor.exe`。

### 3. 连接《活侠传》

菜单「文件 → 安装管理」，选择包含 `Mortal.exe` 的游戏文件夹，点「安装 BepInEx」——
编辑器会自动下载安装兼容的 BepInEx 6 与游戏内运行时，并写入 Steam 普通启动修复。

### 4. 做第一段剧情

新建剧情 → 添加角色 → 添加对白 → 按 **F5** 试玩。

### 5. 导出

导出得到 `xxx.lommod`，发给别人即可（对方同样需要本工具装的运行时）。

## 截图

<!-- TODO(宣传素材): 至少四张图，建议放 docs/assets/screenshots/：
     ① 主编辑器全貌 ② 剧情流程图 ③ 用户音频/对白语音 ④ 游戏内实际效果。
     取消注释并替换路径：
![剧情编辑器](docs/assets/screenshots/editor.png)
![游戏内效果](docs/assets/screenshots/ingame.png)
![分支与流程图](docs/assets/screenshots/flow_graph.png)
-->

## 为剧情制作设计的工作流

### 从任意位置试玩（F5）

选中一个步骤按 **F5**：编辑器生成独立临时包，游戏到达安全场景后自动从该步骤开始，
进入前自动补上该步骤之前的舞台状态（当前场景、台上人物的站位/表情/朝向）——
从剧情中途进入不会再因"角色不存在"黑屏。临时包不覆盖正式 Mod，读入后自动删除。

### 导出前体检（F6）

按 **F6** 检查：编译错误、断路与不可达步骤、死循环、占位文本、缺失素材、
错误的用户音频引用、"人物未登场就说话/行动"的黑屏风险。双击问题可定位到对应步骤；
「安全自动修复」只处理不改变剧情含义的机械问题（含自动补人物登场），支持撤销。

### 剧情流程图

右侧「流程图」显示真实跳转连线（一对多分支用不同颜色区分），断路、
无法结束的死循环和不可达步骤会用红框与文字同时标出。

### 全局搜索（Ctrl+Shift+F）

跨当前项目的全部章节搜索章节/步骤 ID、台词、人物、表情、语音、图片、变量、Flag、跳转和 `user:` 内容引用。支持类别过滤；双击结果会直接切换到对应章节与步骤。搜索只建立编辑器内索引，不修改剧情数据。

## 用户内容

把本机音频导入「用户内容库」（菜单「文件 → 用户内容库」），得到稳定编号
（如 `user:mohui.battle`），在剧情步骤里按「用户 / 官方」分组选择。剧情只保存编号；
导出时只打入当前 Mod 真正引用的音频，玩家机器不依赖作者本机内容库。

| 内容类型 | 状态 |
| --- | --- |
| 自定义音乐 / 音效 / 环境音 | ✅ 已支持 |
| 角色对白语音 | ✅ 已支持 |
| 自定义人物立绘 / 称号 / 介绍卡 / 体型 | ✅ 已支持 |
| 社区内容库 | ◯ Roadmap |

详细用法见 [用户内容库文档](docs/zh_CN/user_content.md)。

## 安装别人做的 Mod

把 `.lommod` 交给编辑器「文件 → 安装管理」安装并勾选启用即可
（手动路径：`BepInEx/plugins/MortalModHost/mods/`）。
进游戏后点自由场景/标题画面左下角「活侠MOD」按钮（或按 F8），
选择「演出 mod 剧情」或「开始新战役」。游戏内菜单会跟随游戏当前语言。

## 兼容性

| 项目 | 状态 |
| --- | --- |
| Windows 10/11 | ✅ |
| Steam《活侠传》 | ✅（含普通启动修复） |
| BepInEx | 编辑器自动安装 |
| Python | Windows 发行版无需 |
| 修改游戏原文件 | 不需要 |

## 当前版本

**v0.7.0**：自定义角色立绘 · 对白语音归属 · 介绍卡与称号 · 体型滑条 ·
离场清台 · 节点按类型编号。

完整变更见 [Release Notes](https://github.com/mohui666/lom_modkit/releases)。

## Roadmap

- 社区内容仓库（分享/复用用户内容）
- 更多用户内容类型（背景等）

## 文档

| 文档 | 内容 |
| --- | --- |
| [文档索引](docs/README.md) | 语言导航与读者向导 |
| [用户内容库](docs/zh_CN/user_content.md) | 自定义音频 / 对白语音用法 |
| [Mod 包格式契约](docs/zh_CN/mod_format.md) | 包结构、43 种节点、编译约定、运行时行为 |
| [AI / CLI 手册](docs/zh_CN/ai_cli.md) | story_api 命令行与 Python API |
| [多语言](docs/zh_CN/i18n.md) | 界面与文档的 i18n 架构 |

## For Developers

### 架构

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

### 源码目录

- `compiler/`（`lomc`）— JSON 剧情 → 游戏原生 Lua 编译器
- `editor/` — PySide6 图形编辑器；`editor/story_api.py` 为 AI/脚本受控接口（Python API + CLI）
- `runtime/MortalModHost/` — BepInEx 游戏内插件
- `tools/` — 从解包产物提取编辑器数据/素材的脚本
- `data/` — 编辑器数据（`editor_data.json`，schema 3）
- `samples/` — 示例 mod（demo_mod、showcase、showcase2 全节点演示 2.0、snack_case《点心大盗疑案》、probe）

### 从源码运行

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

### 构建与测试

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

打包 Windows 发行版：`cd editor && .venv/Scripts/python build_exe.py`
（产物在 `editor/dist/lom_modkit/`，含 `lom_editor.exe` 与 `story_api_cli.exe`）。

游戏内调试：任意场景按 **F7** 切换「禁用原版剧情」会话级开关（不持久化）；
复测已读变黄时用编辑器「试玩 → 重置剧情已读状态」。

## FAQ

**Q：Steam 点「开始」后没有「活侠MOD」按钮、F8 没反应？**
在编辑器「文件 → 安装管理」里点「修复 Steam 无法加载」，然后从 Steam **普通启动**（不要管理员）。

**Q：需要装 Python 吗？**
不需要。Windows 发行版是独立 exe。只有从源码运行/开发才需要 Python 3.10+ 与 .NET（构建插件）。

**Q：Mod 会修改我的游戏文件或存档吗？**
不会修改官方脚本与文本表。「开始新战役」使用隔离存档槽（`mod_<modid>`），不覆盖你的正常存档。

**Q：做好的 Mod 可以发给别人吗？**
可以。导出的 `.lommod` 自包含（含引用的音频/图片），对方用本工具装好运行时即可游玩。

## 许可与声明

MIT 许可（[LICENSE](LICENSE)）。粉丝自制工具，与游戏开发商无关，不包含游戏本体任何文件。

- 游戏机制调研基于对官方脚本的实证分析（1814 个剧情脚本）；反编译源码因版权原因不随仓库公开。
- `data/editor_data.json` 由 `tools/extract_editor_data.py` 从解包产物生成；仓库不包含解包产物与游戏文件。
- 示例 mod 仅演示工具能力，不含游戏原始剧情内容。
