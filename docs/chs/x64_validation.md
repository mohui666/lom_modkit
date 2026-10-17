# x64 适配验证记录

2026-10-17，基于已合并 Windows Rust 修复的 `dad0030`，为游戏 64 位更新适配加载器及 Host。保留 `package_format=3`、`story_schema=2`、`content_schema=1`。

## 当前证据

| 层次 | 结果 |
| --- | --- |
| Windows 环境 | Windows 11 x64 Build 26300；Rust/MSVC 1.97.1、VS 2022、.NET SDK 8.0.426 |
| 当前游戏 | Steam Build 25987745；Mortal.exe / UnityPlayer.dll 均为 AMD64；本次游戏日志版本为 release_1.0.6000.3 |
| 接口核对 | 对当前安装的 Mortal.Combat.dll 只读反编译 CombatStat，确认 Reset() 存在，HealthAddition / BlockAbsorbAddition / IgnoreNormalMode 已移除；现场核对 CharacterIntroPanel.Show(string) 与 Show(string,int) 两个重载；没有执行游戏程序集哈希验证 |
| Windows 工作区 Release 测试 | 203 通过、0 失败、3 默认跳过；7 个新架构回归包含在总数中 |
| Host | 使用当前游戏程序集构建，0 错误、0 警告；离线 SmokeTest 输出 PASS |
| Windows 发布包 | 通过 scripts/build-windows.ps1 构建 ZIP；从 ZIP 解压的原生编辑器已启动并打开 Showcase 项目 |
| 预览 | 5 章、133 / 133 节点、63 类节点；仅为预览覆盖 |
| 当前游戏目录安装 | 用同一源码的 game_tools 安装入口执行匹配的官方 x64 ZIP、Steam 修复和 ZIP 内 Host 更新；安装诊断 healthy=true，无错误项，steam_fix_applied=true |
| 升级残留 | 实机发现 version_alt.dll 为 x86；修复后 version.dll / version_alt.dll 均为 x64，旧文件保留备份 |
| 数据保留 | 安装阶段 65 份 Save 文件不变；游戏运行阶段 62 份不变，3 份试玩/全局文件有写入，另新增 1 份隔离自动战斗存档；无文件丢失。正式 Host DLL 与最终 ZIP 内文件完全一致 |
| Mac / CI | Mac 原生应用重新构建、签名与 133 节点预览通过；GitHub Rust 与 Runtime CI 通过 |

退出游戏后已先归档测试产生的文件，再恢复 3 份被本次试玩写入的既有文件和对应 preview sidecar；新增的隔离战斗存档移入测试证据目录。最终 65 / 65 份基线文件与测试前逐字节一致。Universe JSON 的本次变化仅为时间戳与新增一条 MOD 已读对白，没有原版进度改动。

安装过程保留原 BepInEx、Host 与代理备份，Host 自带上一版回滚。构建时 VS 环境将 Host 输出放到 `bin/x64/Release/net48`，已通过构建脚本的 `-RuntimeDirectory` 指定实际输出；该目录名不代表 Host 被编成 x64，项目明确设置 AnyCPU。

## Windows 游戏实机冒烟

从 ZIP 解压的编辑器选择 Combat 样例，点击「试玩」（F5 对应入口）后，由 Steam 启动当前游戏：

- BepInEx 日志确认 `Process bitness: 64-bit (x64)`、MortalModHost 加载、Harmony 全部挂载。进入 `mod_campaign_lom_modkit_preview` 隔离试玩；请求文件和临时包被正常消费。
- Combat 使用用户角色「林灯」及独立 `center` 背景；界面显示敌方 333/333 生命、111/111 气力和第 1 回合。原版「捅人」操作后进入第 2 回合，敌方生命减少至 228/333。随后使用实机原有 `lom_assistant` 的「直接胜利」完成结算，Host 恢复玩家属性并返回 Story，再进入 Battle 章节。
- Battle 正确生成双方各 3 人，准备界面显示「丐帮围攻」。点击「出征」后实际开战，自然胜利；返回 Story 显示 `Battle 结果：FriendWin`。本场没有调用「直接胜利」。
- 经游戏菜单回到标题，MOD 入口恢复可见，再正常退出游戏。最终 `BepInEx/LogOutput.log` 与 `Player.log` 均无 Error / Exception。

首次启动曾在 `Player.log` 发现 `CharacterIntroPanel.Show` 的 `AmbiguousMatchException`，当时 Host 未就绪，未记作通过。依据当前安装的 `Mortal.Core.dll` 现场反编译，把预检和 Harmony 特性改为精确匹配 `Show(string)` 后重建 ZIP，再完成上述实测。首次失败日志单独保留。

当前游戏缺少三个旧 CombatStat 字段，因此 HarmonyX 会输出三条 `AccessTools.Field` 警告；这是可选旧字段探测，Host 会跳过不存在的字段，本次 Combat 已正常运行。没有将警告计为零。

尚未逐项重跑全部 63 类节点、角色介绍卡演出、失败结果分支、存档重载或旧 x86 游戏实机测试。本记录不作这些项目的通过结论。

3 个默认跳过项分别需要官方 BepInEx 压缩包、FFmpeg 实际视频输入和外部水印语料。实际 ZIP 的匹配/错误位数/篡改检查已在此前 Mac 定向测试通过，该结果不并入 Windows 的 203 项计数。

## 复跑入口

```powershell
cargo test --locked --workspace --release
dotnet build runtime/MortalModHost/MortalModHost.csproj -c Release
dotnet run --project runtime/MortalModHost/test/SmokeTest/SmokeTest.csproj -c Release
powershell -File scripts/build-windows.ps1 -RuntimeDirectory <正式Host输出目录>
out/windows/lom_modkit/lom-editor.exe --smoke-preview samples/showcase3
```

## 产物与证据

最终发布包：`lom_modkit-v1.2.0_windows_x64.zip`，SHA-256：

```text
aa36b3cd01bd414e468255cffd8719f9014d4fb972014440b23b05f2f907860c
```

Showcase 使用当前 Rust 生成器重新生成：5 章、63 类节点；`inspect` 返回 `ok=true`、`integrity_verified=true`、`source_lua_match=true`。预览覆盖 133 节点。

Windows 独立验证目录为 `C:\Users\mohui666\Documents\ChatGPT\lom-x64-release-20261017`；证据保存在 `out/x64-validation`，包括构建与测试日志、首轮失败及最终游戏日志、安装诊断、存档前后清单。Mac 发布副本与截图保存在项目 `out/x64-adaptation`。没有提交游戏 DLL、反编译源码、提取资源或私人存档。
