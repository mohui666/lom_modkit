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
- **剧情内容本地化**：同一 Story 可维护简中、繁中、日语、韩语译文，支持默认语言与缺失译文回退；旧项目无需迁移。
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

按 **F6** 检查：编译错误、无出口 SCC、断路与不可达步骤、跨章节跳转与入口、
可能先读后写的 Mod Flag，以及用户内容的类型、图片、立绘、语音、路径、引用、
metadata 和当前项目未使用项。双击问题可定位到对应步骤；未使用内容只提示、不删除。
「安全自动修复」仍只处理不改变剧情含义的机械问题（含自动补人物登场），支持撤销。

需要反馈问题时，用 **帮助 → 导出诊断包** 一键生成 ZIP。诊断包采用固定白名单：
版本、Manifest、项目计数、F6 结果，以及限长并脱敏的编辑器/Host 相关日志；不会复制
剧情正文、用户内容、Mod、存档、私人目录、整份游戏日志或任何游戏文件。

### 剧情流程图

右侧「流程图」显示真实跳转连线（一对多分支用不同颜色区分），断路、
无法结束的死循环和不可达步骤会用红框与文字同时标出。

### 全局搜索（Ctrl+Shift+F）

跨当前项目的全部章节搜索章节/步骤 ID、台词、人物、表情、语音、图片、变量、Flag、跳转和 `user:` 内容引用。支持类别过滤；双击结果会直接切换到对应章节与步骤。选中内容、人物、变量、Flag、章节或步骤后可点「谁引用了它？」查看精确引用位置并继续定位。搜索与引用查询只建立编辑器内索引，不修改剧情数据。

### 批量编辑

「编辑 → 批量编辑」可选择多个节点，批改人物、表情、语音、站位、时长和常用布尔/枚举字段。窗口只开放所有所选节点中字段名与 schema 类型完全一致的交集；不兼容字段自动消失，不做字符串到数值等猜测式转换，整次操作可撤销。

### 节点模板

「编辑 → 节点模板」可把当前单个步骤或一段连续步骤保存到本机模板库，再插入到任意剧情。每次插入都会生成未占用的新节点 ID，并同步重映射模板范围内的 `goto`、选项、条件分支和骰子去向；指向模板外部的跳转保持不变。重名模板不会覆盖，包含本机绝对资源路径的模板会被拒绝。

### 剧情 Section / Group

「编辑 → 剧情分组」可用 Section 包住连续剧情范围，并在 Section 内继续建立 Group。左侧步骤树支持双击或右键折叠、展开和树形导航；节点重命名与删除会同步修复范围锚点。分组只写入 `_editor.sections` 元数据，不重排 `nodes[]`、不改 `start/goto`，同一剧情有无分组编译出的 Lua 完全一致。第一版不把分组当函数或子程序。

### 跨章节复制 / 粘贴

「编辑 → 跨章节复制 / 粘贴」可把来源章节的一段连续节点插入另一章节的任意位置。目标章节内的 ID 冲突会自动消解，复制范围内部的普通跳转、选项、条件与骰子去向会同步重映射，`user:` 内容引用保持原样。指向范围外节点、缺失章节或依赖范围外隐式顺序流转的引用不会被猜测式修改；操作完成后会逐条列出需作者确认的告警，整次复制可撤销。

### Variable / Flag 管理器

「编辑 → 变量 / Flag 管理器」统一列出 Mod 会话 Flag、官方数值 Flag、Checkpoint、Condition 与 Flowchart 变量的读取数、写入数和首次写入位置，并可直接跳转或查找全部引用。对 Mod 会话 Flag 使用真实 CFG 做保守数据流分析，标出确定未使用和“存在路径可能先读后写”；不可达读取不会误报。官方系统值可能被原版代码消费，原生 Lua 也无法可靠解析，因此这两类显示“不可静态判定”，不会猜成未使用。

### Condition Inspector

「编辑 → 条件检查器」把每个 `branch` 展开成可读条件，列出所用属性/Flag、每条 case 与真实兜底目标，并支持定位和引用查询。当前只对 Mod Flag 做可证明分析：若从章节起点到该分支的所有 CFG 路径都先执行同名 `flag` 写入，则标为“恒真”；存在绕过路径、官方 Checkpoint/Condition、属性或官方数值 Flag 一律保留“不可静态判定”，不会把未知 Runtime 状态猜成恒真/恒假。

### Story Path Simulator

「试玩 → 剧情路径模拟器」在全部章节上运行编译器一致的 CFG 分析，集中报告不可达节点、断裂目标、无出口循环、可证明死分支、缺少最终结局，以及错误的 `next_script`、入口和战役触发器。结局分析会沿跨章节链传播，因此 A→B→最终结局不会误报，而 A↔B 闭环会被识别。依赖属性、官方状态或可达 Raw Lua 的结果保持“未知”，绝不伪装成确定模拟。

### Story Test Runner

「试玩 → 剧情测试运行器」可用 JSON 声明 `initial.variables/flags`、按节点编号选择的 `actions.choices`，以及“到达节点/结局、变量值、Flag 值”断言，无需启动游戏。执行器支持确定的顺序流、`goto`、choice、Mod Flag、已给初值的 stat/官方数值 Flag、branch 和跨章节 `end.next_script`。骰子、Raw Lua、未给初值的官方状态及其他无法静态执行的节点返回 `UNSUPPORTED`，不会伪造 PASS。测试定义保存在 `_editor.tests`，不改变编译语义。

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

### 如何辨认玩家制作剧情

运行时会对所有 Mod 剧情强制显示两层来源信息：固定的
**「玩家制作 MOD｜非官方内容」**警示，以及经过清洗的作品名、作者自报信息和 16 字符包指纹。
标识同时出现在屏幕右上角、对白框、死亡/结局/人物介绍卡上，经过 Loading、GameOver、End
等场景仍会保持；只有真正回到官方 Title / Free 枢纽才解除。

这项披露没有 Lua 或配置开关。独立守护对象会在 Update、渲染提交前和摄像机预渲染前复验标识，
并额外绘制固定 IMGUI 非官方章；即使 Mod 销毁插件宿主或全部 Canvas，也会重建标识。标识无法恢复时，
运行时会停止 Mod 协程、用独立安全遮罩封住画面并返回 Free；演出中关闭插件总开关也会延迟到官方枢纽。包指纹取最终 `.lommod` 原始字节的
SHA-256 前 16 个十六进制字符，可用来核对具体文件，但**不是官方签名，也不代表作者身份经过认证**。
任何 manifest 自报的 `official`、`verified`、`sha256` 字段均不会授予官方身份。

纯客户端机制无法阻止修改 DLL 或对截图逐像素修图；没有上述标识的网络截图只能视为来源不可验证，
不能据此认定是官方剧情。
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
| [来源水印协议 v1](docs/zh_CN/watermark_protocol.md) | 跨 Python/C# 的 payload、ECC、强制画面嵌入与版本规则（不是签名或 DRM） |
| [截图来源水印检测器](docs/zh_CN/watermark_detector.md) | 离线检测 PNG/JPG，输出置信度、协议、Mod 哈希与 CRC/ECC 状态 |
| [视频来源水印检测器 v1](docs/zh_CN/watermark_video_detector.md) | FFmpeg 抽帧与多帧相关累积；真实 OBS/H.264 状态单独标注 |
| [编辑器自动恢复](docs/zh_CN/editor_recovery.md) | 脏项目每 30 秒写独立原子恢复副本，绝不覆盖正式项目文件 |
| [项目模板](docs/zh_CN/project_templates.md) | 空项目、线性对白、分支、自定义人物与用户内容五种普通 Story 起点 |
| [项目统计](docs/zh_CN/project_statistics.md) | 只读表格汇总 Story、节点、资源、语音覆盖、不可达与未使用资产 |
| [语音覆盖](docs/zh_CN/voice_coverage.md) | 按总计、Story、人物统计配音并定位每个未配音对白节点 |
| [Editing / Release 体检](docs/zh_CN/release_preflight.md) | F6 日常检查与 Ctrl+F6 发布严格检查，按风险分级而非全升 error |
| [Release Builder](docs/zh_CN/release_builder.md) | 本地完成 Manifest/SemVer 校验、Release 体检、打包与整包 SHA-256，不自动安装或发布 |
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
- 新导出的包显式声明 `package_format`、`story_schema`、`content_schema`；三端统一拒绝未知或冲突版本，旧 `format: 1` 包继续兼容读取。
- 编辑器会把缺少显式声明的旧 v1 Story / 用户内容迁移到当前格式；覆盖前保留 `*.pre-migration-v1.bak` 原始字节，写入使用同目录原子替换，迁移失败不会破坏源文件。
- manifest 可声明最低/已测试 Host 版本及要求/已测试游戏版本；Host 在注册脚本前按真实 `Application.version` 给出明确拒载或兼容性警告，旧包无字段时不受影响。
- 打包输出使用稳定条目顺序、JSON、ZIP 时间戳/权限和 Lua，并附 `package-content.sha256`；同一工具链相同输入可逐字节复现，逻辑内容哈希不依赖 ZIP 压缩元数据。
- 「文件 → 检查 Mod 包」可只读查看陌生 `.lommod` 的 Manifest、Story、Lua、Texts、资源/用户内容、大小与逐文件哈希，并报告格式、兼容性、逻辑哈希和资源引用问题；检查不会导入或执行包内内容。
- 用户内容库可把单个角色、音频或图片导出为离线 `.lomcontent` Content Pack；包内记录稳定 ID、类型、SemVer、作者、许可证、规范 metadata、文件大小/哈希和直接依赖，导入时校验全部内容、提示本地缺失依赖并拒绝任何跨类型 ID 冲突；不静默覆盖、不自动下载、不做依赖求解。
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

# 可选：离线截图来源水印检测器
python -m pip install -r compiler/requirements-detector.txt
PYTHONPATH=compiler python -m lomc detect-watermark screenshot.png --json

# 可选：需要另外安装 FFmpeg
PYTHONPATH=compiler python -m lomc detect-watermark-video capture.mp4 --json
```

### 构建与测试

```bash
# 编译器测试（193 例）
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
