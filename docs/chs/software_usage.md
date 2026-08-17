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

一对一原版 Combat。`character` 只决定姓名和四类动画；`background` 从官方
`views` 独立选择。HP、气力、内力、内功、其余能力、天赋、行动概率都由节点
填写，不从人物预设带入。儒学/佛学/道学/形意/战术由决斗技能写入，不要当独立
属性填。无独立四帧时回退该人物 normal 立绘，并钉在官方待机中心。

## 战役（Battle）

多人原版 Battle。每个附加阵营单独设人数，该方总人数 = 各阵营人数 + 具名角色。
不要再填 `friend_people` / `enemy_people`。可选：

- `title`：写入准备画面 `ReadyPanel` 那一行官方标题；
- `friend_health` / `enemy_health`：本次实例克隆 `HealthData` 后的基础血量；
- `friend_characters` / `enemy_characters`：已核实可生成的官方人物。

地图、中立路人和技能预设不向作者暴露。`PlayerDie` 按原版只能重试或回标题。

## 读档隔离

标题「开始 MOD 战役」复用原版读档面板，先选战役再进该战役自己的槽。
手动槽 `mod_campaign_<campaign_id>`；自动槽追加 `_auto` / `_auto_free` /
`_auto_battle`。原版「继续游戏」不会进 MOD。

## 来源标记

演出全程有非官方标识。水印协议与检测见 [来源水印](watermark.md)。
