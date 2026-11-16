# MortalModHost Runtime

MortalModHost 是 BepInEx 6 / Harmony 宿主，负责加载 `.lommod`、把编译后的 Lua 接入《活侠传》现有 Fungus/LuaManager 流程，并承载自定义媒体、战役入口、作品存档隔离和游戏原生视频生成。

当前源码 Host 版本为 **1.6.2**。`addon/sandbox` 作品由 `ScenarioConfig/ScenarioEngine/ScenarioRuntime` 负责配置、纯调度与游戏适配；事件、跨事件任务和统一结算由独立事件配置管理，章节也可直接调整时间、属性、好感等状态。从 v1.3.4 起，编辑器、CLI 和 Host 使用相同的发行版本号；旧作品的最低兼容版本条件不变。完整契约见 [事件与任务](../../docs/chs/events_and_tasks.md)，版本更新见 [GitHub Releases](https://github.com/mohui666/lom_modkit/releases)。

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

## 游戏内开发工具

Host 1.6.2 的 F8「MOD 工坊」提供原版事件目录、可读触发条件、前后任务参照及显式授权的日期／章节跳转。普通「原版＋MOD」进度和允许原版事件的隔离测试可使用；不重建早期历史或强制命中事件。工具语言由 `General.ToolsLanguage` 独立保存，空值跟随游戏，其他值为 `chs/cht/ja/ko`，不改变作品对白语言。操作见 [F8 工坊](../../docs/chs/software_usage.md#游戏内-mod-工坊与开发工具)与[开发跳转](../../docs/chs/events_and_tasks.md#游戏内开发者跳转)。

决斗专用背景按原版 `CombatLevel.BackImage` 引用读取，接受目录中的 `combat:<Sprite.name>`；旧场景背景映射继续保留。同名决斗人物结合剧情立绘关联解析，临时关卡不继承原版事件、胜负或条件结果旗标。作者字段与最低版本见 [格式契约](../../docs/chs/mod_format.md)。

## Rust 计算模块

`rust/lom-host-native` 承担批量字节与文本计算：身份完整性校验、流式 SHA-256、JSON 结构／字符串扫描、长 JSON 字符串转义、原版剧情补丁与来源分类、事件候选排序／池分组、事件存档 UTF-8／Base64、对白可见文本元素计数、PCM16 转换、重采样混音及流式 Vorbis 解码。包路径词法检查也在 Rust 中执行，重复／大小写冲突与文件归属仍使用原托管规则。JSON 扫描直接写入可精确表示的 32 位整数及标准四位 ASCII 十六进制转义；负零、其他数值与特殊 HexNumber 写法仍按原顺序调用当前 C# 运行时转换。来源分类直接读取已验证 tape，不把 double 再格式化成 JSON。候选资格、概率、随机调用、结算与存档顺序仍由 C# 管理；包、存档和 Lua 格式不变。

`NativeHostKernel` 将 x64／x86 组件作为资源嵌入同一个 AnyCPU Host，首次使用时按程序集 MVID 与进程架构解包至临时目录，比对资源原字节后从绝对路径加载。各模块共用一次加载，借用固定原始数组同步调用，不缓存完整性校验结果。解析结果与流式解码器使用明确的原生所有权并在 finally／Dispose 释放；解码输入复制一次，rewind 复用输入与不可变码表。沿用可选原生组件的构建契约：组件不可用时使用既有 C# 计算；已加载组件缺少入口、拒绝输入或计算失败时直接报错，不用 C# 重试成功。身份检查频率与短路顺序不变，不恢复画面水印。

对白计数按游戏 Mono 的原 StringInfo 组合规则工作，首次根据当前 BCL 构建只读 Unicode 分类／大小写表，后续扫描不创建每字符字符串。Vorbis 保留首个逻辑流、帧数、尾部裁切、循环及损坏页前缀规则；不同解码实现的浮点舍入可能产生细小 PCM 差异。音频设备、Unity 对象、剧情钩子与播放生命周期继续由 C# 接入。

`ReadTextPatch` 仅在 LeanLocalization 确实刷新后恢复 MOD 台词，不在每帧无变化调用中全表注册；包的合并文本与 Story 前缀只在 Rebuild／语言变化时重建。显式 Apply 仍写入当前 LeanTranslation 对象并更新独立动态玩法文本。

同一个只读包加载中，逻辑内容摘要与 Story／Lua 条目摘要共用一次解压读取，Rust 在一次调用中更新两个 SHA-256 上下文。条目摘要只在整包内容摘要通过后用于本次逐行绑定核对；每行声明、错误先后与完整覆盖检查保留，不跨加载保存验证结论。ZIP 文本按已受限的声明长度分配缓冲，同时保留实际 EOF 与大小限制。

事件章节边界检查每章只解析一次，后续按原事件与 LIFO 顺序检查结尾摘要；未到达章节的结尾字段错误仍保持延迟。调度仅在一次 Select 内复用相同周期值的格式化字符串，每次 Context 属性读取及其顺序照常执行。槽归属使用等价的无分配字符串检查，原槽命名及历史解析规则不变。这些部分以实测收益选择 C#／Rust 边界。

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
.\scripts\build-host-native.ps1
dotnet build runtime/MortalModHost/MortalModHost.csproj -c Release -p:GameDir="C:\path\to\LegendOfMortal"
```

原生构建复用已有 Rust 工具链，需要 `x86_64-pc-windows-msvc` 与 `i686-pc-windows-msvc` 标准库；仅缺失时运行 `rustup target add <目标>`。脚本默认输出 `out/host-native/{x64,x86}/lom_host_native.dll`，可用 `-Architectures x64` 构建单一架构。Host 用 `-p:HostNativeDirectory=<目录>` 指定其他组件目录；没有原生资源的构建仍可使用原 C# 计算。原生代码没有游戏引用，ABI 与返回状态约定见 [计算入口](../../rust/lom-host-native/src/lib.rs)。

纯 C# 契约测试不依赖游戏安装：

```powershell
dotnet run --project runtime/MortalModHost/test/SmokeTest/SmokeTest.csproj -c Release
```

原生组件构建后，运行 `cargo test --locked -p lom-host-native`，以及 SmokeTest 的 `-- --native-modules-only --require-native-integrity` 参数，分别验证 Rust 参考向量和实际跨语言差分；后者在组件未实际加载时直接失败。性能测量应使用游戏自带 Mono，把冷启动、无 profiler 的热循环计时与独立进程的分配计数分开，不能把单个校验函数的倍数当作游戏帧率或视频导出倍数。

Rust 工具端离线检查由 `scripts/test-native.sh` 调用；C# SmokeTest 保持独立入口。Windows 单文件构建使用 [`scripts/build-windows.ps1 -NoArchive`](../../scripts/build-windows.ps1)，正式输出为 `out/windows/lom-editor.exe`；参数及本地依赖见 [Rust 工具端](../../docs/chs/rust.md#构建与启动)。
