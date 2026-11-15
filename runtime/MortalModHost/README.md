# MortalModHost Runtime

MortalModHost 是 BepInEx 6 / Harmony 宿主，负责加载 `.lommod`、把编译后的 Lua 接入《活侠传》现有 Fungus/LuaManager 流程，并承载自定义媒体、战役入口、作品存档隔离和游戏原生视频生成。

当前源码 Host 版本为 **1.6.0**。`addon/sandbox` 作品由 `ScenarioConfig/ScenarioEngine/ScenarioRuntime` 负责配置、纯调度与游戏适配；事件、跨事件任务和统一结算由独立事件配置管理，章节也可直接调整时间、属性、好感等状态。从 v1.3.4 起，编辑器、CLI 和 Host 使用相同的发行版本号；旧作品的最低兼容版本条件不变。完整契约见 [事件与任务](../../docs/chs/events_and_tasks.md)，版本更新见 [GitHub Releases](https://github.com/mohui666/lom_modkit/releases)。

## 接口原则

- 能调用原版系统的节点使用已经由反编译确认、并由 Host 构建/SmokeTest 覆盖的真实方法签名；完整映射以 `docs/chs/mod_format.md` 为准。
- `enemy`、`battle_skill` 与高层 `combat` / `battle` 只调用原版战斗 API。结果所有权绑定包 id + 完整 SHA-256 和剧情 id；`battle_result` 只能读取匹配所有者的真实 win/lose。Combat 观察 `GameOver(bool)`，Battle 只观察 finish=true 的 FriendWin/EnemyWin。Runtime 不包含自定义 Battle Engine，不伪造 draw/escape，也不改写 PlayerDie(false)。
- 自定义角色和媒体是独立 Mod Runtime 对象，不伪造或覆盖原版 Addressables 条目。
- `persistent_var` / `persistent_check` 读取原版 `SaveSystem.CurrentSlot`，状态写入 Host 管理的、与实际 MOD 手动隔离槽一一对应的 sidecar（`mod_campaign_<campaign_id>` 或 `_sNNN`）；原版手动/自动存档成功后才原子落盘，不改 GameSave schema，也拒绝普通/F5 官方槽。
- 标题入口临时复用原版 `LoadGamePanel/LoadSlotPanel/AutoSaveSlotPanel/RecentSaveSlotPanel`：先选择战役，再进入该战役自己的 001～020 手动栏位和三类自动槽。001 映射到 `mod_campaign_<campaign_id>`，002～020 映射到 `_sNNN`；空手动栏位可新建，已有栏位可覆盖/读取。三类自动槽追加 `_auto*`；旧 `mod_<manifest.id>` 槽不探测、不迁移。`SaveUniverseData` 只记录最后一个原版槽，因此原版 001～020、`auto*` 和“继续游戏”指针均不会被 MOD 覆盖。
- Manifest、路径、包大小、哈希和脚本注册名在 Runtime 再次验证，不能只信编辑器导出的包。

改游戏功能必须先查反编译接口，见仓库根目录 [`AGENTS.md`](../../AGENTS.md) 与 [`docs/chs/decompiled_api.md`](../../docs/chs/decompiled_api.md)。已确认签名与禁止项见 [`research/gameplay_api.md`](../../research/gameplay_api.md)。游戏更新后只读反编译受影响的当前类型与方法，不运行程序集哈希校验。整份游戏反编译 `.cs` 不要提交。

## 包身份与画面水印

1.6.0 的普通试玩和视频生成均停用全部可见与隐藏画面水印，不创建底部标记、面板标记或隐藏图形载体。`ModDisclosure.Tick` 继续检查实际包身份和会话状态，停用水印不表示图形载体已维护成功；包校验、Lua 执行检查和存档归属规则仍保留。Mod 来源会话在真正进入官方 Title / Free 时结束。

`detect-watermark` 与 `detect-watermark-video` 保留用于读取旧媒体，不表示当前成片仍嵌入来源标记；相关协议见 [旧媒体来源检测](../../docs/chs/watermark.md)。包指纹不是官方签名或作者认证。

## 原生视频与试玩状态

编辑器「生成视频」、`--cli render-video` 和 Python `cli.render_video(...)` 使用同一流程，输出游戏本体画面与音轨的 MP4。默认后台模式由编辑器启动独立桌面的专属 worker，不切换当前桌面；worker 使用独立 Host、临时演出包和存档目录，完成、失败或取消后由任务回收。它复用已安装游戏及加载器，不复制整套游戏，仍使用 CPU、GPU 与编码资源。

worker 最早初始化时建立 `SaveSystem.GetUserFolder`、`GetSaveDataFile`、`GetLegendDataFile`、`GetBackupDataFile` 四条路径的重定向，实际调用确认均位于本任务 `worker/profile` 后写入 `worker-ready.json`，才接收视频请求。初始化失败立即退出本 worker，不继续无隔离运行；普通游戏与试玩不启用这组 worker 补丁。

视频默认 1080p、30 FPS，也可选择 720p、60 FPS；支持当前章节、整个作品或当前步骤起播，按游戏自动播放时长执行预设剧情选项。取帧与编码使用有界缓冲并行衔接。对白、选项和演出可自动推进，需要人工操作的战斗、商店或交互面板会停止并返回具体位置。参数、结构化进度及输出约定见 [CLI 视频命令](../../docs/chs/cli_parameters.md#游戏原生视频)。

SDK 试玩请求附带 `request_id` 时，`PlaytestStatusRuntime` 记录 Host 接收、章节进入和节点进入等真实执行观察；`--cli playtest-status`／`cli.playtest_status(...)` 查询一次当前状态，不自行轮询。状态记录不能代替人工检查画面或证明完整路线已通过。

## 构建与测试

项目目标为 .NET Framework 4.8 / AnyCPU，禁用 Prefer32Bit；同一托管 DLL 用于 x86 和 x64 Mono。BepInEx／Doorstop 原生加载器必须匹配 `Mortal.exe` 位数，升级步骤见 [维护者手册](../../docs/chs/maintainers.md#windows-游戏接入与-runtime-回滚)。正式构建需要本机游戏程序集。

还需已有的本地提取映射 `data/assets/_probe/view_map.json` 作为嵌入资源；该文件不随 Git 分发，缺少时构建会报 CS1566（无法读取 `MortalModHost.view_map.json`）。

在仓库根目录执行：

```powershell
dotnet build runtime/MortalModHost/MortalModHost.csproj -c Release -p:GameDir="C:\path\to\LegendOfMortal"
```

纯 C# 契约测试不依赖游戏安装：

```powershell
dotnet run --project runtime/MortalModHost/test/SmokeTest/SmokeTest.csproj -c Release
```

Rust 工具端离线检查由 `scripts/test-native.sh` 调用；C# SmokeTest 保持独立入口。Windows 单文件构建使用 [`scripts/build-windows.ps1 -NoArchive`](../../scripts/build-windows.ps1)，正式输出为 `out/windows/lom-editor.exe`；参数及本地依赖见 [Rust 工具端](../../docs/chs/rust.md#构建与启动)。
