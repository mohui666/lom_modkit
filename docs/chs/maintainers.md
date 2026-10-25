# 维护者手册

当前工具端由 Rust 编译器、核心库与 egui 编辑器组成；C# `MortalModHost` 保留游戏接入。日常编辑见 [软件使用](software_usage.md)，构建和 CLI 见 [Rust 工具端](rust.md)，验证层次见 [测试矩阵](test_matrix.md)。旧 Python／Qt／PyInstaller 入口已经退役，不作为本手册操作入口。

2026-10-04 本轮仅核对文档、源码和脚本，未运行下列命令。上一轮 Mac 验收的 179 项测试和 133 项预览结果详见 [迁移验收表](rust_migration.md)。Windows 构建、Windows 测试与游戏实机测试均未在此次迁移中执行。

## 编辑器入口与设置

在仓库根目录执行 `sh scripts/build-macos.sh`，产物为 `out/LoM Modkit Rust.app`；也可用 `cargo run --locked -p lom-editor -- <项目目录>` 启动源码版。应用内含原生 `lom-editor` 与 `lomc`，无需 Python 或 Qt。

当前为浅色 Rust 通用外壳和三栏布局。Mac 界面语言在屏幕顶部系统应用菜单的“设置…”中选择，快捷键 `⌘,`；非 Mac 使用窗口内“设置…”或 `Ctrl+,`。四种语言按各自原文显示，选择即时生效，不把项目标为未保存。Swift／Liquid Glass 外壳尚未完成。

## 保存、撤销与恢复

项目保存使用“文件 → 保存全部章节”或 `⌘S`（非 Mac 为 `Ctrl+S`）。素材和离线测试的有效草稿一起保存；无效输入会保留并提示，需修正或主动撤销未完成输入。撤销／重做不会绕过这个检查。

编辑器保存时保留原有章节文件名和无关文件。删除已保存素材或撤销已保存章节后，保存只清理有已保存快照证明、且磁盘内容仍匹配的旧文件；外部改动或归属不明的文件会阻止该清理。另存目录与包导出不清理原来源。

未保存修改时，每 30 秒写入本实例的恢复目录：

| 平台 | 恢复目录 |
| --- | --- |
| macOS | `~/Library/Application Support/lom_modkit/rust/recovery/<PID>-<时间戳>/` |
| Windows | `%APPDATA%\lom_modkit\rust\recovery\<PID>-<时间戳>\` |
| 其他桌面平台 | `~/.local/share/lom_modkit/rust/recovery/<PID>-<时间戳>/` |

目录包含 `manifest.json`、`story/*.json`、包内素材及 `session.json`。恢复写入不覆盖原项目；有效但尚在输入中的素材名称等草稿会应用到恢复副本，不改动活动输入。无效素材草稿仍留在编辑器，状态栏说明恢复副本尚未包含它，其他有效项目修改继续保全。

后续恢复写入只清理本实例已登记、当前已移除的章节与素材，避免删除后恢复时重新出现；未知文件不参与这项清理。启动时仅列出带 `session.json` 且所有者不再活跃的恢复目录，可恢复、移除副本或稍后处理。恢复会保留原项目来源、章节路径和目录布局，并标为未保存；请检查后再保存或另存。

这不是旧版 `snapshot.json`／`os.replace` 流程，也不承诺整个目录在多文件写入失败时一次回滚。

## 章节、模板与素材

- 章节旁“＋”先选择新建或复制，再填写章节名称。标识自动生成；复制保留完整剧情内容及显式跳转目的地。内部标识改名位于“编辑 → 高级”，会同步相关引用。
- “文件 → 从模板新建”含空项目、线性对白、分支剧情、自定义人物展示、用户内容展示、自由模式剧情六种模板。`user:template.*` 是待替换占位引用，导出前换成真实素材。
- 打开“素材管理 → 作品素材”，再点“导入素材…”；先选类型与文件，再确认可读名称。标识自动生成，取消不会导入，现有内容不会被同名文件静默覆盖。

## 检查、统计与发布

`F6`／“剧情工具 → 检查项目”执行章节校验与编译检查，不等于完整作品体检。事件／任务配置、Manifest 与发布信息还需下面的 Editing／Release 体检。完整 Editing／Release 体检在“发布 → 发布体检与打包”中分别点击“编辑体检”或“发布体检”；当前没有单独的 `Ctrl+F6` Release 快捷键。

「发布」页面还提供统计、打包和包检查；迁移与诊断从「剧情工具」进入：

| 区域 | 操作 |
| --- | --- |
| 统计与配音覆盖 | 汇总节点、引用素材和未使用资源；按章节、人物检查缺少配音的对白 |
| 发布体检与打包 | 展示安全修复建议、可撤销地应用建议、生成发布目录 |
| 包检查与水印检测 | 检查 `.lommod`，只读查看 JSON／Lua、条目大小与摘要；检测图片或视频水印 |
| 迁移与诊断 | 迁移所选 JSON；导出脱敏诊断 ZIP |

统计不能代替编译或发布体检。填写了配音引用不代表该文件有效；引用合法性仍由体检和打包检查。报告可定位到对应章节或步骤。

“生成发布目录”在本地校验 Manifest、执行 Release 体检并生成 `.lommod`、SHA-256 校验文件和说明。它不自动安装、启动游戏或上传。校验和只证明字节一致，不是作者签名。视频水印检测需要可用的 FFmpeg。

## Windows 游戏接入与 Runtime 回滚

当前事件／任务作品要求包含 Scenario 的 Host 1.1.3，新 Windows 构建应同时提供 `MortalModHost.dll` 与 `NVorbis.dll`。选择完整编辑器旁的 `runtime` 目录；`changed:false` 仅表示所选源与已装文件一致，不代表互联网最新版本。

当前源码的一级入口为“游戏与 MOD”。首次选择包含 `Mortal.exe` 的目录后，点击“一键配置游戏”：自动下载匹配架构的官方加载器、配置 Steam 启动、安装随工具分发的 C# 宿主并检查结果。缺少宿主文件时会明确提示，可在“高级维护”选择同时含 `MortalModHost.dll`、`NVorbis.dll` 的完整目录。Mac 显示平台说明及素材、发布入口，不展示不可用的安装控件。

Runtime 更新在 `BepInEx/plugins/MortalModHost/.runtime_rollback/` 保留已有 DLL 的备份与记录；安装写入失败时尝试恢复原文件。全新安装没有上一版。回滚不应改动游戏程序、Managed、存档或第三方插件。在线配置与“高级维护 → 使用官方离线压缩包配置…”均只接受代码固定版本、架构和摘要匹配的官方压缩包。

游戏升级为 64 位后，使用更新后的编辑器执行：

1. 退出游戏，在该区域选择游戏目录，确认显示 `x64`。
2. 点击“一键配置游戏”，或在“高级维护”选择 [BepInEx 官方 build 692](https://builds.bepinex.dev/projects/bepinex_be) 的对应离线 ZIP。x64 使用 `BepInEx-Unity.Mono-win-x64-6.0.0-be.692+851521c.zip`；旧版 x86 游戏使用对应的 x86 包。安装保留已有配置、插件和 MOD。
3. 配置结束后查看安装检查结果，确认页面显示“配置就绪”。独立的 Steam 修复、宿主更新及回滚入口保留在“高级维护”。修复会选择匹配游戏位数的 Doorstop 和系统 `VERSION.dll`，替换旧 `version.dll`、`version_alt.dll`，备份后移走 `winhttp.dll`，避免同时注入。不能只替换 Host 而保留 32 位代理。

替换的原生 DLL 保存在同目录的 `.lom_bak`；如该备份已存在且内容不同，会另存带摘要的备份，不覆盖早先备份。x64 系统库来自 `System32`（32 位编辑器通过 `Sysnative` 访问）；x86 来自 `SysWOW64` 或经位数确认的 `System32`。写入前校验所有待安装原生 DLL 的 PE 架构，位数错误时不修改配置。

安装检查接受 x86 与 x64；残留的错误位数代理会报告 `loader_architecture_mismatch`，需按上述顺序修复。C# Host 使用 .NET Framework 4.8 / AnyCPU，`.lommod` 的 **3 / 2 / 1** 契约不变。托管 DLL 的 PE 头显示 x86 不等于只能运行在 32 位进程，不能用原生代理检查规则误判 Host。

此次游戏更新还移除了 `CombatStat` 的三个旧字段，因此也需更新 `MortalModHost.dll`。新 Host 对旧版存在的字段继续清零，对现版已移除的字段不再直接引用；详见 [受影响接口核对](../../research/gameplay_api.md#2026-10-17-x64-更新核对)。

以上安装说明是保留的代码能力。2026-10-25 的 Windows 构建及编辑器检查见 [整合验证](windows_event_task_merge_20261025.md)，该次没有启动游戏或执行真实安装；F5 试玩请求也不属于 Mac 预览验收。

## 原生离线验证入口

以下均在仓库根目录执行；本次文档维护没有运行它们：

```sh
sh scripts/test-native.sh
dotnet run --project runtime/MortalModHost/test/SmokeTest/SmokeTest.csproj --configuration Release
```

前者依次做 Rust 格式检查、workspace 测试、样例预览、Rust 样例生成和包检查；生成目录固定为 `out/showcase3-native`，现有生成文件会受归属检查限制，重复验证可使用 [测试矩阵](test_matrix.md) 中的独立步骤和新输出目录。后者是 C# 离线 SmokeTest，不需要启动游戏，也不能证明 Unity 中的时序或交互正确。
