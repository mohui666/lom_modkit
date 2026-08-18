# 软件使用

编辑器工作流。节点字段以编辑器「帮助 → 文档」和 [mod_format](mod_format.md) 为准。
改决斗/战役实现前先读 [反编译接口](decompiled_api.md)。

## 基本流程

1. 新建或打开项目，在左侧组织章节和步骤。
2. 中间表单编辑当前节点；下拉显示可读名称，保存仍写稳定 ID。
3. 右侧预览 / 流程图 / `F6` 体检；发布前 `Ctrl+F6`。
4. `F5` 生成隔离试玩包；「导出 Mod」生成正式 `.lommod`。
5. 音频、图片、自定义角色从「用户内容」导入，引用写成 `user:<id>`。

模板、统计、语音覆盖、恢复副本、发布构建、安装诊断见 [维护者手册](maintainers.md)。

## 决斗（Combat）

一对一原版 Combat。`character` 只决定对手姓名和四类动画；`background` 从官方
`views` 独立选择。对手 HP、气力、六维、评语、技能、行动概率由节点填写；只填写
对手最大血量/气力时，会以该最大值满血/满气开场，填写初始值才会覆盖它。
`player_*` 是赵活官方 `GameStat` 基准值，被动走 `FinalValue`。血量先按官方
`_playerTotalHealth` 从体力等换算；若填了 `player_max_health`，再把这个基准加在
换算结果上，而不是盖掉加成。`player_talents` 只改点名的决斗技能。不写存档槽，
战后写回战前基准。标题存档先选战役再进 001～020。
儒学/佛学/道学/形意/战术由决斗技能写入。无独立四帧时回退 normal 立绘。

## 战役（Battle）

多人原版 Battle。每个附加阵营单独设人数，该方总人数 = 各阵营人数 + 具名角色。
不要再填 `friend_people` / `enemy_people`。可选：

- `title`：写入准备画面 `ReadyPanel` 那一行官方标题；
- `friend_health` / `enemy_health`：本次实例克隆 `HealthData` 后的基础血量；
- `friend_characters` / `enemy_characters`：已核实可生成的官方人物。

地图、中立路人和技能预设不向作者暴露。`PlayerDie` 按原版只能重试或回标题。

## 读档隔离

标题「开始 MOD 战役」复用原版读档面板：左侧剧情 / 自由 / 战役三类自动栏
（空栏为官方 `System/NoData`），右侧 001～020 与原版一样；已有档可读取，标题页的空栏可为当前 MOD 开新周目。
游戏内菜单和战役暂停页的「读取」默认进入当前 MOD 的次级存档页，同样显示该 MOD 的 001～020 栏位；空栏只读不可开新周目。点自动栏读该战役隔离槽
`mod_campaign_<id>_auto*`，不会载入原版 `auto_battle`。原版「继续游戏」不会进 MOD。

## 来源标记

演出全程有非官方标识。水印协议与检测见 [来源水印](watermark.md)。
