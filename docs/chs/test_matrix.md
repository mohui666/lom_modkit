# 测试入口与验证边界

本版变化见 [1.6.7 GitHub Release](https://github.com/mohui666/lom_modkit/releases/tag/v1.6.7)。仓库不保存版本 notes；测试事实和未覆盖范围记录在本页，发布正文不设置验证或已知限制栏目。

## 1.6.2 发布验证记录

2026-11-16，发布合并提交 `e79d434`。以下为该版本发布时的检查记录，文档修订不另计一次运行：

- Rust workspace：1012 项通过、16 项默认忽略；x86 原生模块另跑 27 项通过。Host 纯托管与实际要求原生组件的离线回归分别通过，保留两条路径的实际加载结果。
- 决斗背景兼容：已发布 1.5.9／1.5.18 fixture 打开、编辑、保存／导出及重开通过；重新计算包摘要仍无法掩盖无关 Lua 替换。11 章、73 节点类型样例的完整性及 Story／Lua 对照通过，使用专用背景时最低 Host 为 1.6.2。
- 游戏内决斗：未修复 Host 复现 `girl5` 多套动画身份冲突。最终 DLL 的隔离游戏自动回归覆盖普通／入魔小梅、人物别名、其他变装人物和自定义人物共六场，117 项断言通过，包括四帧载入、作者数值、原生攻击、胜负返回剧情及官方资源未改写；28 张专用背景引用均可解析，后山背景另核对实际游戏画面。
- 图片与附件：28 张专用背景实际提取和第二次缓存复用均通过。正式单 exe 为 40,046,592 字节，公开链接完整下载的 SHA-256 与成品一致；内置 Host、x64／x86 原生资源与所测组件一致。

决斗检查为独立客户端中的程序驱动流程，四类存档路径隔离，显示设置结束后恢复；未完成物理鼠标交互验收。隔离启动出现的 1 次 `SteamAPI_Init()` 失败单独记录，最终回归未发现其他游戏或 Host 错误。本版未构建或实测 Mac。F8 工具的目录、四语显示和导航检查与决斗回归分别记录，不能相加冒充 Rust 单元测试数量。

## 1.6.0 历史验证记录

2026-11-15：Rust workspace 954 项通过、15 项默认忽略；Release 配置 Runtime 离线回归通过；最终 Windows 单 exe 导出的 SDK 9 项通过。另用实际发布的 1.5.20 编译器产包，完成打开 → 修改 → 保存／导出 → 重开，核对原文件、素材与作品身份保持。无关 Lua 替换被拒绝的证据来自另行执行的 Rust／Runtime 安全回归（含旧 fixture），不是对这个新 1.5.20 输入做攻击测试。这些结果分别统计，不把预览状态或重复运行累加为测试数。本次未构建或实测 Mac。

Windows 精简版后台原生视频短样例通过：720p／30 FPS，290 帧、9.666667 秒，H.264＋AAC 48 kHz 双声道，预设第二分支执行正确；实际抽帧确认无可见水印、颜色和方向正常，音轨非静音。实际编码子进程使用内置组件目录中的精简 FFmpeg，与构建候选逐字节一致。四类 SaveSystem 路径均进入任务独立目录，154 次桌面观察未见可见或前台 Worker，任务结束退出并清除临时 Worker/core，原输入保持不变。总耗时 17.709 秒，running→encoding 4.994 秒；这些时间仅代表本机这段短片。

相同版本 FFmpeg 按原生导出所需组件裁剪，编码器从 105,423,872 字节降至 10,276,366 字节，正式单 exe 从 75,793,408 字节降至 38,814,720 字节。相同 3 秒合成输入的 720p30／1080p30／1080p60 均通过实际解码、90／90／180 帧、尺寸、时长和非静音音轨检查；两阶段总耗时分别变化 −6.62%、+2.21%（11.5 ms）、−3.71%，峰值内存更低。每档只测一次短片，不将这些波动写成所有工程的速度保证。

本机 Ryzen 9 7940HX／32 个逻辑处理器，发布版 1.5.20→1.6.0 原生编辑器对照：每项独立偏好、同一预览图片库、无特效库，客户区 1280×760，预热后采样 5 秒，表中为最后约 2 秒。旧版用 recent 路径与窗口标题确认工程，新版另确认只有一个对应标签；全部测试进程正常退出。

| 场景 | WS MiB 前→后 | private MiB 前→后 | 单核空闲 CPU 前→后 |
| --- | --- | --- | --- |
| 空工程 | 153.51→157.81 | 156.24→159.47 | 5.49%→0.75% |
| showcase | 195.18→194.55 | 196.42→195.65 | 3.99%→采样 0 |
| 2 万节点＋64 MiB 输入 | 327.10→325.62 | 332.10→329.87 | 3.01%→采样 0 |

CPU 百分比以一个逻辑核为 100%；采样 0 表示短窗口内没有测得处理器时间增长，不保证持续零占用。每场景每版本仅一次短观测，不能据此评估交互、多个活动工程或视频生成时的整体占用。另有包含特效库的 headless 冷开检查，不能与这组 GUI 空闲指标混作同一条件。同一 2 万节点＋64 MiB 样本的新标签准备耗时另测由约 1.14 秒降至 0.40 秒，也仅适用于这次本机输入与缓存条件。

## 验证层次

| 层次 | 当前入口 | 覆盖与限制 |
| --- | --- | --- |
| Rust 格式 | `cargo fmt --all -- --check` | 源码格式，不证明运行正确 |
| Rust workspace | `cargo test --locked --workspace`；发行配置另加 `--release` | 编译、包格式、受控创作 API、内容管理、编辑状态、表单、保存／恢复、模态与语言等单元／集成回归 |
| 样例预览 | `cargo run --locked -p lom-editor -- --smoke-preview samples/showcase3` | 逐节点校验、编译与预览状态模拟；明确返回未渲染，preview_states 不代表图片数 |
| 舞台出图 | 编辑器 `--cli render-preview`／`render-previews` | 实际绘制纯编辑器舞台和对照图，报告缺失素材；不运行游戏、不继承原版前置章 UI，也不等于人工看图 |
| 跟踪试玩 | `playtest`／`playtest-event` 后查询 `playtest-status` | 区分请求、Host 收件、Lua／节点和事件入口证据；状态查询成功不代表画面验收 |
| 样例生成与包检查 | `lomc` 的 `build_showcase3` example 和 `inspect` | 源 JSON 校验、73 类型覆盖、打包、包内源码／Lua 一致性 |
| Mac 应用构建 | `sh scripts/build-macos.sh` | 原生程序、应用资源、签名检查与内置预览烟测；GUI 仍需另外打开检查 |
| C# 离线回归 | `dotnet run --project runtime/MortalModHost/test/SmokeTest/SmokeTest.csproj --configuration Release` | 使用离线测试工程；不启动 Unity，也不验证真实游戏生命周期 |
| Windows 原生工具 | `scripts/build-windows.ps1 -NoArchive` | 使用正式 Host DLL 构建；构建、自动化与实际 UI 结果分开记录 |
| Windows 单 exe | 发布版 `lom-editor.exe --standalone-smoke <项目> <游戏目录> <空输出目录>` | 使用内嵌组件完成项目导出重开、完整游玩包导出及 13 项音频播放／停止／取消；组件缓存必须与旧安装隔离 |
| 游戏原生视频 | `render-video` 或编辑器“生成视频” | 原生画面、音轨、预设分支及后台隔离需实测；不能用 PNG 预览或请求已发送代替成片检查 |
| Rust 资源读取 | `lom-assets` 与 `vanilla_routes` 定向测试；本机游戏对照另行显式执行 | 合成音频包与路线 fixture 不含官方素材；全量游戏图片、动画、剧情及 45 分支对照不进入仓库，不等于游戏运行验收 |
| 游戏实机 | [Showcase 手动流程](../../samples/showcase3/README.md#游戏手动检查) | 按具体场景记录结果，本版范围见当前能力 |

受控 API 对照测试内部的旧版案例不能再加到 Rust 测试总数上重复计数；不同阶段重跑、预览状态、C# 冒烟、构建与 UI 操作也分别记录。旧 JSON／Lua fixture 是历史对照数据，当前测试不启动已退役的 Python 编辑器或编译器。

## 常用命令

在仓库根目录执行。`sh scripts/test-native.sh` 串联格式、workspace 测试、预览、生成器和包检查。下列独立步骤便于区分结果：

```sh
cargo fmt --all -- --check
cargo test --locked --workspace --release
cargo run --locked -p lom-editor -- --smoke-preview samples/showcase3
cargo run --locked -p lomc --example build_showcase3 -- out/showcase3-check
cargo run --locked -p lomc -- inspect out/showcase3-check/showcase3.lommod --json
sh scripts/build-macos.sh
```

样例生成应使用尚未存在的输出目录。当前生成器创建新 Project，不能凭命令中的输出路径认领已有章节文件；若 `out/showcase3-check` 已有生成内容，改用新的目录名。固定使用 `out/showcase3-native` 的 `scripts/test-native.sh` 也受此限制，不应通过删除未知文件或绕过所有权检查继续。

## 需要外部依赖的案例

用 `cargo test --locked --workspace -- --list --ignored` 查看当前忽略清单。下列用例按需准备依赖后单独执行。

- 官方加载器 ZIP 安装测试：需 `LOM_BEPINEX_ARCHIVES` 指向固定版本的官方归档，仅写临时合成安装目录。

- `ffmpeg_real_video_extraction`：需 `LOM_FFMPEG` 指定可执行程序，并显式选择该 ignored 测试。
- 历史截图水印语料测试：需 `LOM_WATERMARK_CORPUS` 指向事先生成的外部语料。
- 公开版本的实际更新下载测试，需要网络和已发布附件。

手工性能测试使用合成大项目，单独运行并记录面板耗时。缺少依赖、默认忽略与实际失败分别记录。

## GUI 与 CI

当前 [CI 配置](../../.github/workflows/ci.yml) 仅保留 Ubuntu 上的 `Runtime tests`，运行 C# 离线测试。Rust 格式、测试、预览与样例生成按上面的本地命令验证。macOS CI 任务已移除；CI 不承担 Windows 或游戏实机验证，也不替代本机窗口的视觉检查。

维护者操作与恢复目录说明见 [维护者手册](maintainers.md)。
