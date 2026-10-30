# MortalModHost Runtime

MortalModHost 是 BepInEx 6 / Harmony 宿主，负责加载 `.lommod`、把编译后的 Lua 接入《活侠传》现有 Fungus/LuaManager 流程，并承载自定义媒体、战役入口和强制非官方来源披露。

当前源码 Host 版本为 **1.3.5**。`addon/sandbox` 作品由 `ScenarioConfig/ScenarioEngine/ScenarioRuntime` 负责配置、纯调度与游戏适配；事件、跨事件任务和统一结算由独立事件配置管理，章节也可直接调整时间、属性、好感等状态。1.1.4 修复旧 Host 误拒绝合法节点、冲突包占位和战役列表提示，实机验证范围见 [v1.3.2 更新日志](../../RELEASE_NOTES_v1.3.2.md)。从 v1.3.4 起，编辑器、CLI 和 Host 使用相同的发行版本号；旧作品的最低兼容版本条件不变。完整契约见 [事件与任务](../../docs/chs/events_and_tasks.md)。

## 接口原则

- 能调用原版系统的节点使用已经由反编译确认、并由 Host 构建/SmokeTest 覆盖的真实方法签名；完整映射以 `docs/chs/mod_format.md` 为准。
- `enemy`、`battle_skill` 与高层 `combat` / `battle` 只调用原版战斗 API。结果所有权绑定包 id + 完整 SHA-256 和剧情 id；`battle_result` 只能读取匹配所有者的真实 win/lose。Combat 观察 `GameOver(bool)`，Battle 只观察 finish=true 的 FriendWin/EnemyWin。Runtime 不包含自定义 Battle Engine，不伪造 draw/escape，也不改写 PlayerDie(false)。
- 自定义角色和媒体是独立 Mod Runtime 对象，不伪造或覆盖原版 Addressables 条目。
- `persistent_var` / `persistent_check` 读取原版 `SaveSystem.CurrentSlot`，状态写入 Host 管理的、与实际 MOD 手动隔离槽一一对应的 sidecar（`mod_campaign_<campaign_id>` 或 `_sNNN`）；原版手动/自动存档成功后才原子落盘，不改 GameSave schema，也拒绝普通/F5 官方槽。
- 标题入口临时复用原版 `LoadGamePanel/LoadSlotPanel/AutoSaveSlotPanel/RecentSaveSlotPanel`：先选择战役，再进入该战役自己的 001～020 手动栏位和三类自动槽。001 映射到 `mod_campaign_<campaign_id>`，002～020 映射到 `_sNNN`；空手动栏位可新建，已有栏位可覆盖/读取。三类自动槽追加 `_auto*`；旧 `mod_<manifest.id>` 槽不探测、不迁移。`SaveUniverseData` 只记录最后一个原版槽，因此原版 001～020、`auto*` 和“继续游戏”指针均不会被 MOD 覆盖。
- Manifest、路径、包大小、哈希和脚本注册名在 Runtime 再次验证，不能只信编辑器导出的包。

改游戏功能必须先查反编译接口，见仓库根目录 [`AGENTS.md`](../../AGENTS.md) 与 [`docs/chs/decompiled_api.md`](../../docs/chs/decompiled_api.md)。已确认签名与禁止项见 [`research/gameplay_api.md`](../../research/gameplay_api.md)。游戏更新后只读反编译受影响的当前类型与方法，不运行程序集哈希校验。整份游戏反编译 `.cs` 不要提交。

## Fail-closed 边界

Mod 演出一旦开始即保持来源会话污染标记，只有真正进入官方 Title / Free 才解除。全局来源载体始终存在；对白框正文下方居中显示高可见度半透明单行标识，避免遮字并抵抗只裁对白框的截图，死亡/结局/人物介绍卡另有局部标识。必需披露表面无法创建或恢复、Lua 加载/执行失败、或补丁不完整时，Host 停止 Mod 协程并返回 Free；不会继续无标播放。包指纹和画面水印用于文件/画面来源核对，不是官方签名或作者认证。

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

Rust 工具端离线检查由 `scripts/test-native.sh` 调用；C# SmokeTest 保持独立入口。Windows 完整目录构建使用 [`scripts/build-windows.ps1 -NoArchive`](../../scripts/build-windows.ps1)，参数及本地依赖见 [Rust 工具端](../../docs/chs/rust.md#构建与启动)。
