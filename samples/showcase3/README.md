# 全节点样例 3.0

这是全节点创作样例，也是后续手动实机验收的输入包。当前源数据含 6 章、151 个流程节点，以及同时执行组内的 5 个动作；Rust 生成器按 `authoring.json` 检查全部 66 种节点覆盖。

第一章含美颜赵活登场、对白及恢复普通外观；当前完整样例另含事件结果演出章，清单要求 Host ≥1.3.6。
2026-10-04 上一轮 Mac 验收记录包括 133 / 133 节点预览检查；整个 Rust workspace 为 179 项通过、0 失败、2 项默认跳过。这不是游戏实机结果。该段为 2026-10-04 历史记录；2026-10-19 的同时执行组与本轮离线验证见文末。

在仓库根目录生成并检查，输出目录须尚未存在：

```sh
cargo run --locked -p lomc --example build_showcase3 -- out/showcase3-check
cargo run --locked -p lomc -- inspect out/showcase3-check/showcase3.lommod --json
```

[`source.json`](source.json) 保存独立创作源；修改样例先更新它。当前入口是 [`rust/lomc/examples/build_showcase3.rs`](../../rust/lomc/examples/build_showcase3.rs)，不再运行旧 Python 生成器。生成器校验节点覆盖，复制本样例素材，写出 JSON 和包含编译 Lua 的 `showcase3.lommod`；不会在受跟踪 `story/*.json` 旁散落 Lua。

如果 `out/showcase3-check` 已有生成文件，请改用新的输出目录。显式输出模式创建新 Project，不能认领已有章节文件。无参数模式会先打开当前样例工程，再按已有文件归属刷新受跟踪 JSON；维护前应审阅 `source.json`，维护后核对 diff，保留原素材及用户文件，不整目录覆盖。

在 Mac 查看样例可用 `cargo run --locked -p lom-editor -- out/showcase3-check`，或构建后从 `out/LoM Modkit Rust.app` 打开该目录。当前为浅色 Rust 三栏外壳，C# Host 与 3／2／1 格式契约保留；Swift／Liquid Glass 不属于已完成的样例验收。

下列实机步骤只是保留的人工检查清单，只有任务明确包含游戏测试时才执行。本次迁移没有执行。

实机主路径仍分为五章；第六章 `event_presentation` 是独立事件结果演出参考，不会自动接在主路径后。主路径为：演出与用户内容 → Gameplay → Combat（可跳过）→ Battle
（可跳过）→ 死亡画面或安全返回。Battle 的 PlayerDie 沿用原版重试/标题流程，
并不伪造一个可继续的失败分支。

当前 Combat 样例使用包内自定义人物 `user:showcase.lin_deng`（林灯，只有
normal/happy，没有战斗四帧）和独立背景 `center`。雷达与性情/内功等按 100
填写。待机、攻击、受伤必须是同一张立绘且停在官方待机中心，不得贴底或切状态
上移；详情滑条按 CombatStat 的 100 上限显示，不能被玩家 GameStat.Max≈50
截断。Battle 样例为各阵营自带人数、标题「丐帮围攻」、双方基础血量，并附加
樊啸天 / 南宫深。

2026-10-17 的 x64 适配沿用本样例现有 `combat` 字段，未新增创作参数。新版 Host 已处理原版 `CombatStat` 移除的三个旧字段；后续实机复验第三章时，应确认不会出现 `HealthAddition`／`BlockAbsorbAddition`／`IgnoreNormalMode` 的缺字段异常，人物数值仍来自本章配置。本次构建与包检查不代替这项实机验收。

## 手动测试顺序

1. 从 Steam 普通启动游戏。在标题或自由场景按 `F8`，打开“活侠MOD”。
2. 选择“全节点样例3.0·六十六节点实机验收”并开始演出。
3. 第一章确认用户 BGM、音效、背景、CG、前景图、林灯立绘与语音都能出现；
   人物介绍卡换人后文本应更新。对白区域内部应始终有重复的低透明度
   `MOD / UNOFFICIAL + 包指纹`，右上角还有高对比来源芯片。
4. choice、branch、骰子无论选择/结果为何，都应继续进入第二章，不能只改文字而
   停留在旧节点。
5. 第二章依次观察属性、物品、好感、奖励提示、自定义商店和五类检定。关闭商店
   后应继续；任务、持久变量、activity、自动存档和武学面板不应卡住剧情。
6. 第三章可进入原版 Combat 或跳过。若进入，顶部姓名必须是“林灯”，四态都是
   同一张立绘且位置不变；打开详情时内力/体力/性情等应能显示到 100，滑条不得
   在 50 就顶满。背景仍是独立的 `center`。战斗结束后必须回 Story。
7. 第四章可进入原版 Battle 或跳过。准备画面标题为「丐帮围攻」；我方应有樊啸天，
   双方血量为设定值。FriendWin/EnemyWin 应回 Story；PlayerDie 按原版只能重试
   或回标题。
8. 终章选择“安全返回 Free”应回自由模式并关闭披露；选择死亡测试应显示带 MOD
   标记的死亡卡并回标题。

若后续授权的游戏测试失败，请保留 `BepInEx/LogOutput.log`、编辑器错误提示或终端输出、最后看到的章节与节点文案。不要只截黑屏；日志中的 `mod-runtime-error` 会包含 story/node/trace。当前 Rust 编辑器不以旧版 `crash.log` 路径作为必备日志入口，也可从“文件 → 导出诊断报告…”导出脱敏报告。

工具验证范围见 [测试矩阵](../../docs/chs/test_matrix.md)，游戏接口边界见 [当前能力](../../docs/chs/current_capabilities.md)。

## 2026-10-19 同时执行组

第一章新增 `parallel_moves`：赵活与林灯同时移动并播放铃声，等待较长的 0.9 秒移动完成后继续。`parallel_arrival` 放在盖黑幕、切背景之后，同时摆好两个人，再由原有 `transition2` 揭幕。组内动作通过原有表单编辑，不需要写 Lua。

编辑器播放也能显示黑幕持续盖住 `parallel_scene_setup` 与 `parallel_arrival`，直到 `transition2` 揭幕。人物在黑幕之前不会穿透显示；转场时长为编辑器示意，不代表游戏内的实际时长。

本轮重新生成并检查样例，运行 Rust 工作区与 MoonSharp 合成演出测试、构建 Mac 应用；未启动游戏，也未在 Windows 上运行。前文的 2026-10-04 数据是历史记录。

## 2026-10-20 人物外观与表情

第一章新增 `xiaomei_demon_show`／`xiaomei_demon_talk`／`xiaomei_demon_half`／`xiaomei_demon_hide`，使用当前游戏真实的 `girl5_9`（入魔小梅）及其 `normal`、`special1`、`special2`，用于检查人物 → 外观 → 表情的编辑和预览。项目保持原版人物 ID 和 v3 包格式，不新增 Host 接口。

## 2026-11-01 换表情、气泡与旁白对齐

第一章的 `portrait_official` 和 `portrait_user` 在没有对白的情况下切换官方／用户人物表情，分别使用 0.5 秒淡出和 1.5／1 秒淡入；`say3` 演示靠左旁白，`mood_on`／`mood_off` 演示气泡开关。新控件要求 Host 1.3.6。

本轮重新生成全部 66 类节点并检查包；实机测试使用独立定向剧情，观察了普通／居中旁白靠左留白、默认布局恢复、官方与用户人物换表情，以及 player/shock 气泡开启／关闭。未重跑本全节点包中的 Combat、Battle 等完整路径。
