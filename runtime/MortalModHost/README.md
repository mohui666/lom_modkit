# MortalModHost Runtime

MortalModHost 是 BepInEx 6 / Harmony 宿主，负责加载 `.lommod`、把编译后的 Lua 接入《活侠传》现有 Fungus/LuaManager 流程，并承载自定义媒体、战役入口和强制非官方来源披露。

## 接口原则

- 能调用原版系统的节点使用已经由反编译确认、并由 Host 构建/SmokeTest 覆盖的真实方法签名；完整映射以 `docs/chs/mod_format.md` 为准。
- `enemy`、`battle_skill`、`goto_scene Combat/Battle` 与高层 `combat` / `battle` 只调用原版战斗 API。结果所有权绑定包 id + 完整 SHA-256 和剧情 id；`battle_result` 只能读取匹配所有者的真实 win/lose。Combat 观察 `GameOver(bool)`，Battle 只观察 finish=true 的 FriendWin/EnemyWin。Runtime 不包含自定义 Battle Engine，不伪造 draw/escape，也不改写 PlayerDie(false)。
- 自定义角色和媒体是独立 Mod Runtime 对象，不伪造或覆盖原版 Addressables 条目。
- `persistent_var` / `persistent_check` 读取原版 `SaveSystem.CurrentSlot`，但状态写入 Host 管理的 `mod_campaign_<campaign_id>` sidecar；原版 `SaveGameData` 成功后才原子落盘，不改 GameSave schema，也拒绝普通/F5 官方槽。
- 标题入口临时复用原版 `LoadGamePanel/LoadSlotPanel/AutoSaveSlotPanel/RecentSaveSlotPanel`：先选择战役，再进入该战役自己的主槽、三类自动槽和新建入口。MOD 手动槽为 `mod_campaign_<campaign_id>`，三类自动槽追加 `_auto*`；旧 `mod_<manifest.id>` 槽不探测、不迁移。`SaveUniverseData` 只记录最后一个原版槽，因此原版 001～020、`auto*` 和“继续游戏”指针均不会被 MOD 覆盖。
- Manifest、路径、包大小、哈希和脚本注册名在 Runtime 再次验证，不能只信编辑器导出的包。

改游戏功能必须先查反编译接口，见仓库根目录 [`AGENTS.md`](../../AGENTS.md) 与 [`docs/chs/decompiled_api.md`](../../docs/chs/decompiled_api.md)。已确认签名与禁止项见 [`research/gameplay_api.md`](../../research/gameplay_api.md)。`tools/verify_gameplay_api.py` 对本机安装只读复验；游戏更新后旧哈希不会被静默接受。整份游戏反编译 `.cs` 不要提交。

## Fail-closed 边界

Mod 演出一旦开始即保持来源会话污染标记，只有真正进入官方 Title / Free 才解除。全局来源载体始终存在；对白框正文下方居中显示高可见度半透明单行标识，避免遮字并抵抗只裁对白框的截图，死亡/结局/人物介绍卡另有局部标识。必需披露表面无法创建或恢复、Lua 加载/执行失败、或补丁不完整时，Host 停止 Mod 协程并返回 Free；不会继续无标播放。包指纹和画面水印用于文件/画面来源核对，不是官方签名或作者认证。

## 构建与测试

项目目标为 .NET Framework 4.8。正式构建需要本机游戏程序集；在仓库根目录执行：

```powershell
dotnet build runtime/MortalModHost/MortalModHost.csproj -c Release -p:GameDir="C:\path\to\LegendOfMortal"
```

纯 C# 契约测试不依赖游戏安装：

```powershell
dotnet run --project runtime/MortalModHost/test/SmokeTest/SmokeTest.csproj -c Release
```

仓库级离线矩阵由 `tools/test_matrix.py` 统一调用。Windows 发行包的完整构建和安全覆盖规则见仓库根目录 README 的“构建与测试”。
