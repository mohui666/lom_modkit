# 用户内容库（User Content Library）

> 语言：简体中文（本文） · [繁體中文](../cht/user_content.md) · [日本語](../ja/user_content.md) · [한국어](../ko/user_content.md)

本地、离线、按 Mod 自包含。没有账号、没有在线市场、没有云同步。

## 作品内容库

进入「素材管理 → 作品素材」（或 `⌘L` / `Ctrl+L`），在同一页面管理当前项目的角色、音频、图片，以及 1.4 新增的物品、消耗品、功法、决斗援助与自创招式。每项有独立边框和行间距，名称在左、类型在右，选中项加重边框；同名但不同 ID 的资源加稳定数字区分，搜索不改变编号，只改变界面显示。选中后编辑属性、替换素材、预览图片或试听音频，下方可查看引用位置。

有效修改随整个项目保存，不需要再点一次“应用修改”。格式未完成时保留当前输入，并提示修正或「撤销未完成的输入」；删除前仍检查项目引用。

## 怎么导入

1. 打开「素材管理 → 作品素材」，点「导入素材…」。
2. 选择图片、角色、音乐、音效、配音、物品、消耗品、功法、决斗援助或自创招式，再选择文件。物品、消耗品、功法和援助选择 PNG/JPEG 图标；自创招式可多选 PNG 动画帧，首帧兼作图标，导入后在当前页面填写玩法属性。
3. 确认显示名称并点「导入」。名称初值取文件名，可以改成中文；内部 ID 根据作品自动生成，无需先填写编号。
4. 角色可以一次选择多张立绘，亦可继续追加：第一张作为默认立绘。符合规则的文件名会作为表情编号；中文、数字开头或重复名称会分配合法且不冲突的编号，无需重命名原文件。不同目录中的同名文件会在包内区分保存；也可导入后在「指定表情」处调整。
5. 配音导入后在详情的「绑定人物」中选择说话人，再到对白步骤选择该语音。

剧情引用仍使用稳定编号，例如：

```text
user:mohui.battle
user:mohui.luoxue
```

自动编号来自当前作品 ID 的安全形式，并避开已有内容。显示名称可以重复；重新导入会创建另一项，不会按名称覆盖旧内容。要替换现有文件，先选中内容，再点「添加或替换素材文件…」。

## 支持哪些格式

| 格式 | 说明 |
| --- | --- |
| `.ogg` | Vorbis。第一次播放可能有很短的加载延迟。 |
| `.wav` | PCM 8/16 位或 32 位 float。常见 WAV 可立即播放。 |

不支持 mp3 / flac / aac。音频大小上限 20MB。立绘支持 `.png` / `.jpg` / `.jpeg`，单张不超过 8MB。

以上是内容格式支持；编辑器试听还取决于本机播放器。Windows 的 WAV 试听使用系统 SoundPlayer，OGG 需要 FFplay，可用 `LOM_FFPLAY` 指定路径。点「停止试听」停止当前播放；播放失败会显示错误，不能仅凭启动了试听就判断文件可播放。此处与游戏内 Host 的播放实现分开。

## ID 是什么

剧情里保存的是引用，不是文件路径。

- 官方曲目 / 音效：继续写原来的名字，例如 `普通_001`、`鈴鐺_001`。
- 用户内容：必须是 `user:<命名空间>.<名称>`，只含小写字母、数字、下划线。

显示名称可以是中文（「决战曲」）；内部 ID 不能含空格、中文或路径。

## 用户内容保存在哪里

新导入的素材先属于当前项目。保存项目后位于项目目录：

```text
assets/user/audio/<id>/content.json
assets/user/audio/<id>/音频文件
assets/user/character/<id>/
  content.json
  normal.png
  happy.png
assets/user/image/<id>/
  content.json
  moon.jpg
```

需要跨项目复用时，在选中内容的「共享库与导出」中点「保存到共享库」。共享库默认位于 Windows 的 `%APPDATA%/lom_modkit/repository/`，或 macOS 的 `~/Library/Application Support/lom_modkit/repository/`。它与项目素材分开；导出 `.lommod` 后，玩家不需要你的共享库目录。

旧人物介绍图 / 结局插图的 `assets/文件名.png` 路径继续兼容。新背景、CG、Overlay 统一引用 `type=image`，不分别创建 BackgroundStore / CGStore / OverlayStore。

## Story 保存的是什么

音乐 / 音效步骤的 `name`：

```json
{ "type": "music", "name": "user:mohui.battle" }
{ "type": "show", "character": "user:mohui.luoxue", "position": "M", "portrait": "normal" }
```

禁止保存 `C:\Users\...\battle.ogg` 或立绘绝对路径。

统一图片的 metadata：

```json
{
  "schema": 1,
  "content_schema": 1,
  "id": "mohui.moon_bg",
  "type": "image",
  "name": "月夜",
  "files": { "main": "moon.jpg" }
}
```

图片只允许 PNG/JPG/JPEG，单张不超过 8MB。引用扫描按节点的 `image` 字段定位章节与步骤；删除仍被引用的图片会被阻止。

## 自定义角色数据格式

```json
{
  "schema": 1,
  "content_schema": 1,
  "id": "mohui.luoxue",
  "type": "character",
  "name": "洛雪",
  "files": { "main": "normal.png" },
  "portraits": {
    "normal": "normal.png",
    "happy": "happy.png"
  }
}
```

剧情里的角色 ID 是 `user:mohui.luoxue`（与音频一样：`user:<命名空间>.<名称>`）。`normal` 必填；其它表情 id 只能是字母开头的 `happy` / `angry` / `sad` 这类英文标识。

可选字段 `title` 是对话上方的短称号（原版对白名牌那种）。`scale` 是体型百分比，默认 100；1.4.3 起支持任意游戏可表示的正数及小数，例如 25、175.5、400，不再限制 50～130。图片按原版完整站位矩形等比适配并从站位脚底缩放，横图保持居中留白；窗口或游戏分辨率变化时保持同一比例。`art_facing` 是立绘原图朝向（`left` 默认 / `right`）；节点 `facing` 再叠加转向。可选 `mood_x`、`mood_y` 为图片内部气泡锚点百分比，均为 0～100 的整数，默认 50、90，跟随图片缩放和朝向。可选块 `intro` 是介绍卡资料（称号/姓名/正文/同目录图片），介绍卡图片缩放同样支持正数小数。`intro` 步骤选「使用自定义角色介绍卡」时只引用角色。

1.4.6 新增可选 `portrait_reference`（基准表情 ID）和 `portrait_layouts`（表情 ID → `{ "x":0, "y":0, "scale":100 }`）。偏移按 1920×1080 立绘画布计算，向右／上为正；缩放为正数百分比。校准值随作品保存、导出，原图片不变；使用这些字段要求 Host 1.4.6。自定义人物的气泡锚点相对基准图计算。操作见 [立绘与气泡对齐](software_usage.md#立绘表情与气泡对齐146)。

## 为原版人物增加表情

1. 在「素材管理 → 作品素材」点「为原版人物添加立绘…」，选择原版人物，例如夏灵犀。
2. 选择 `tired.png`、`sleep.png` 等图片，文件名会作为表情 ID。导入后可在「指定表情」下拉框选择、预览或继续添加图片。
3. 在人物登场、对白或人物换表情步骤中继续选择原版人物，再选新增的 `tired`／`sleep`。原版姓名、人物 ID 和其他表情保留。

扩展元数据仍放在 `assets/user/character/<内容ID>/content.json`，增加 `extends`：

```json
{"schema":1,"id":"demo.xia_extra","type":"character","name":"夏灵犀补绘","extends":"special825","files":{"main":"tired.png"},"portraits":{"tired":"tired.png","sleep":"sleep.png"},"scale":175.5}
```

扩展不会自动覆盖 `normal`；显式添加同名表情才替换该表情。每个人物在同一项目内使用一份扩展，可往其中继续追加图片。扩展只作用于当前 MOD，随 `.lommod` 打包，退出 MOD 后清理。新导出的人物素材使用 Host 1.4.3 的完整站位适配，最低 Host 版本会自动提高；原有的「已测试 Host 版本」声明仍需作者按实际情况填写。

`game-previews` 和 `preview_map.json` 是原版图片的本地预览缓存。手工添加其中的路径不会注册新的游戏表情，也不会携带图片导出；请通过作品素材导入。原版书页插图使用「CG」步骤的事件图类型，选择插图或自定义图片。

## 导出后还依赖本机内容库吗？

不依赖。导出只复制**当前剧情真正引用**的用户内容进 `.lommod`：

```text
assets/user/audio/mohui.battle/content.json
assets/user/audio/mohui.battle/battle.ogg
assets/user/character/mohui.luoxue/content.json
assets/user/image/mohui.moon_bg/content.json
assets/user/image/mohui.moon_bg/moon.jpg
assets/user/character/mohui.luoxue/normal.png
assets/user/character/mohui.luoxue/happy.png
```

没被引用的导入内容不会打进包。角色被引用时会带上它定义过的全部表情。引用缺失、类型不对、表情不存在、文件坏了：导出直接失败。

## 如何分享 Mod

把导出的 `.lommod` 发给别人即可。对方安装后由游戏插件从包内播放。对方电脑上没有你的用户内容库也能听。

Windows 顶栏或「文件 → 导出与发布 → 导出完整游玩包…」可一键生成 ZIP，包含作品、自有素材、Host、x86/x64 加载组件、安装助手和游玩说明。首次导出会下载缺少的加载组件并缓存；玩家解压后双击「安装MOD.cmd」即可离线安装，无需编辑器。玩家仍需本机已有正版游戏。游玩包不携带原版游戏或提取的预览资源，更新同一作品会保留原包名和启停状态。

用「文件 → 打开 JSON / Mod 包…」打开包时，包内素材直接载入当前项目。它们不会自动登记到共享库，需要复用时再主动保存到共享库。

## 独立分享：Content Pack v1

先把所选内容保存到共享库，再打开「素材管理 → 共享内容库」。填写内容 ID、版本、作者与许可，点「导出共享内容包」。内容 ID 可从素材「高级属性 → 复制引用」取得，填写时去掉 `user:` 前缀。导入其他人的内容包则先点「检查待导入的内容包」，核对结果后点「导入此内容包」；之后可「复制此内容到当前项目」。

`.lomcontent` 是离线 ZIP 格式，不连接或上传到任何服务器，包含：`content-pack.json`、`files/` 和 `package-content.sha256`。

`content-pack.json` 固定记录 `content_pack_format=1`、content id/type、SemVer 版本、作者、许可证、规范化 metadata，以及每个声明文件的大小和 SHA-256。导入时会检查安全路径、总大小、类型/ID、metadata、文件清单、逐文件哈希和逻辑内容哈希，全部通过后才原子安装。

内容 ID 在所有用户内容类型之间全局唯一。若 `user:<id>` 已存在，导入会明确报告冲突并停止，绝不静默覆盖。Content Pack 的哈希用于传输和构建一致性校验，不是作者签名或官方认证。

`dependencies` 是去重排序的直接内容 ID 列表。导入确认页会逐项对照本地内容库，并明确列出缺失项；缺失只产生警告，不阻止保存这个内容包。第一版不会联网下载、递归安装、自动选择版本或进行依赖求解，作者应把需要的 `.lomcontent` 分别交给玩家。

## 删除资源有什么限制

「删除项目内容（可撤销）」删除当前项目中的这项内容，不等于删除共享库条目。若项目仍引用该资源，删除会被阻止。保存回原项目后只清理已确认归属且未被外部修改的旧素材文件；外部改写会报告冲突。

## 对白语音

`say` 可加可选字段 `voice`，值必须是用户内容引用：

```json
{ "type": "say", "character": "player", "text": "师兄，早。", "voice": "user:mohui.line_01" }
```

没有 `voice` 的对白与以前完全一样。有则进入这句时停掉上一句语音并播放，玩家点下一句或剧情结束/中断时停止。普通音效节点不会打断对白语音。

语音仍是独立的 `audio` 资源，不写进角色的 `content.json`。音频 metadata 可有可选字段 `character`，只表示编辑器里的管理归属：

```json
{
  "schema": 1,
  "content_schema": 1,
  "id": "mohui.line_01",
  "type": "audio",
  "name": "师兄早",
  "audio_kind": "sound",
  "files": { "main": "line_01.wav" },
  "character": "user:mohui.luoxue"
}
```

- 语音在同一个作品内容库里作为音频条目管理，可试听、修改名称、选择或解除「绑定人物」。
- 角色详情编辑名称、称号、立绘朝向、介绍卡和各类素材文件；没有独立的“语音”页签。
- 旁白 / 系统语音可以不写 `character`。旧音频没有这个字段也继续可用。
- 也可以把用户语音关联到官方人物 id（如 `player`），不会为此生成用户角色对象。
- 对白步骤的语音下拉按当前人物筛选已绑定的音频。未关联音频仍可使用合法的 `user:` 引用，通过「手动输入」填写。
- 删除语音仍走原来的引用检查：若某句 `say.voice` 还在用，删除会被阻止。
- 打包只收集剧情真正引用的音频。角色下挂了很多未使用语音，也不会打进 `.lommod`。

在对白步骤的「对白语音」下拉中选择音频，选「无语音」去掉该句绑定。角色或道具立绘被选为说话人、填写了台词，都不会自动生成配音；没有选择语音时只显示文本。人物目录包含原版用于演出的道具立绘，并非每个条目都是可参与好感、决斗或战役的角色。

对白人物可直接在输入框搜索名称，从下方结果点选；取消搜索不改变已有绑定。同名编号不会写入人物 ID 或台词。「默认」表情和「无语音」分别移除该句的 `portrait`、`voice` 可选字段。人物的显示名称来自内容 metadata；缺省对话模式仍显示姓名，不需要额外写入 `mode`。

## 1.4 自定义物品、功法、援助与招式

这些内容沿用同一个 `user:命名空间.名称` 引用和 `.lomcontent` 共享流程。仅复制图标不会产生玩法定义；需导入对应类型并填写专用表单。引用这些内容的作品导出时会将 `min_host_version` 提升到 **1.4.0**，玩家需要更新游戏中的 Host。

旧项目若明确填写了低于 1.4.0 的「已测试 Host 版本」，新增这些内容后导出会提示核对版本信息，不会自动修改作者的测试声明。新建事件作品的版本填写默认使用当前工具版本，作者仍应按实际验证情况调整。

| 类型 | 编辑项 | 在剧情中使用 |
|---|---|---|
| 物品 `item` | 名称、说明、图标、杂物／贵重品类别、基础价格 | 物品节点、物品检定、奖励、自定义商店；事件结果的物品动作也支持 |
| 功法 `martial` | 名称、说明、图标、价格，1～20 级的修炼点数、属性条件、升级加成 | 物品节点选「书籍」，再选择导入的功法；取得后使用游戏书籍修炼界面 |
| 决斗援助 `support` | 名称、说明、头像、触发时机、概率、次数、1～16 项效果 | 决斗节点的「我方自定义援助」「对手自定义援助」列表 |
| 消耗品 `item/category=consumable` | 使用场景、属性或恢复效果 | 物品节点、检定、奖励、事件结果；背包／决斗道具入口主动使用；`use_item` 步骤仅用于剧情／养成类 |
| 主动招式 `skill` | 动作类型、基础威力、基础耗气、命中率、冷却、敌方概率、命中后效果及可选 PNG 动画 | 决斗节点的我方／对手自创招式列表，每方至多 8 项 |

物品与功法价格为 0～999999 的整数。功法每级需要 1～999999 点修炼点数；条件表示对应原版玩家属性至少达到指定值，加成为该级升级时增加的属性值，每组最多 32 项，同组不能重复属性。属性必须从当前玩家属性目录选择。

援助触发时机为回合开始 `round_start`、行动之后 `after_action`、受击之后 `after_hit`，概率 0～100%，次数 1～99。一次触发依次执行全部效果并消耗一次。效果支持造成伤害 `damage`、恢复血量 `heal`、恢复气力 `stamina`，目标为持有援助的一方 `self` 或对手 `enemy`，数值为 1～999999 的整数。主动招式也可配置这些命中后效果。

消耗品 `use_in=field` 默认用于剧情／养成，`effects` 为 1～16 项 `{ "type":"stat", "stat":"mental", "value":3 }`，属性变化为非零整数，范围 -999999～999999。`use_in=combat` 仅用于决斗，效果为 `{ "type":"heal", "value":35 }` 或 `{ "type":"stamina", "value":8 }`，数值 1～999999，固定作用于玩家，不能设置 `target`。使用会扣除一件；决斗使用占用本回合行动，并保留原版休息回气。没有数量、暂停、场景不符或无需恢复时不会使用。数量随存档保留。原版商店不展示消耗品，商店表单会明确拒绝该类别。

招式的 `action` 为 `attack/weapon/ultimate`，`power` 为 0～999999，`stamina_cost` 为 0～9999，`accuracy` 与 `ai_chance` 为 0～100，`cooldown` 为 0～99。威力、耗气与命中率会继续经过原版伤害、互拼、克制与招架规则，不是最终固定扣血或强制命中。招式可选 `animation`，格式为 `{ "frames":["start.png","hit.png"], "fps":12, "hit_frame":1, "scale":100 }`。帧仅允许同目录真实 PNG，1～120 张；帧率 1～60；JSON 的命中帧从 0 开始，必须落在帧列表中；大小 50～200%。图片逐帧在角色处播放，在命中帧实际结算伤害，结束后恢复原角色。未配置动画时使用原版动作。

编辑器导入招式时可一次选择多个 PNG，详情页可以追加、排序、移除、指定命中帧和播放预览。动画原图统一朝左绘制：敌方保持原图朝左，我方播放时自动水平翻转朝右；样例图帧遵循此约定。`samples/showcase3` 中的 `showcase.clear_mind`、`showcase.mending_pill` 与 `showcase.lantern_strike` 分别演示养成消耗品、决斗消耗品与四帧招式。

自定义杂物、功法与援助分别保存到 `assets/user/item/<id>/`、`assets/user/martial/<id>/`、`assets/user/support/<id>/`。共同字段为 `schema:1`、`content_schema:1`、`id`、`type`、`name`、`files.main`，可有 `description`。例如一项功法：

```json
{
  "schema": 1, "content_schema": 1,
  "id": "my_mod.lantern_breath", "type": "martial", "name": "灯下调息诀",
  "description": "静观灯火，调息运气。", "files": {"main": "icon.png"}, "cost": 480,
  "levels": [
    {"points": 10, "requirements": [], "bonuses": [{"stat": "internal", "delta": 1}]},
    {"points": 20, "requirements": [{"stat": "internal", "min": 1}], "bonuses": [{"stat": "internal", "delta": 2}]}
  ]
}
```

发放与配置方式：

```json
{"id":"give_manual","type":"item","kind":"book","item":"user:my_mod.lantern_breath","count":1}
{"id":"duel","type":"combat","character":"player","player_supports":["user:my_mod.friend_aid"],"win":"done","lose":"done"}
```

物品 `category` 默认为 `misc`，也可为 `special`；`cost` 默认为 0。功法 `levels` 必填，每级的 `requirements`、`bonuses` 默认空列表。援助 `effects` 必填，`trigger` 默认 `round_start`、`chance` 默认 100、`uses` 默认 1。界面新建的内容提供可编辑示例值。

这些内容由所属作品包提供，Host 按当前作品解析引用，并在读档前准备对应定义。已有原版 ID 继续沿用原有接口。示例见 `samples/showcase3/source.json` 中的信牌、调息诀与林灯援助；自定义物品和功法的类别必须与节点、检定或商店所选类别一致。

## 音频与演出运行时行为

- 官方名字：原版 Wwise，行为与以前完全相同。
- `user:`：只从**当前正在演出的那个 .lommod** 里找。另一个 Mod 登记了同名 ID 也不会串音。
- 自定义音频用 Windows `waveOut` 播放，不走 Unity `AudioSource`，也不走 Wwise。本游戏主混音是 Wwise，Unity 播了经常没声。
- 音量大致跟随游戏的主音量 × 音乐/音效滑条；不是 Wwise RTPC，不能做到完全一致。
- 自定义 fadeout 是输出音量渐弱（随后仍会按节点等待）。
- 切到自定义音乐时会先停官方背景乐；官方 `StopMusic` 本来就会把环境音一起清掉。
- 回标题、进自由/死亡/结局时自定义音频（含对白语音）会立刻停；官方再播一首 BGM 时会先停自定义 BGM，避免两轨叠在一起。
- 自定义角色不注册进原版 Addressables。`show` / `say` / `hide` / `move` / `face` / `focus` / `offset` / `shock` / `dim` / `rotate` 默认编译为 `mod_char_*`，新增人物布局字段使用 `mod_character_layout`；人物由 `CustomCharacterRuntime` 在官方舞台画布上自建 Image。体型按 `scale` 从脚底缩放；朝向按 `art_facing`（默认朝左）再叠节点 `facing`。
- `offset` 默认累加舞台坐标。Host 1.4.4 起可填 `absolute=true`，把 x/y 设为相对当前站位锚点的绝对偏移；坐标按 1920×1080 舞台换算，向右/上为正。可选 `scale` 为体型百分比，100 沿用素材默认体型，留空保留当前缩放，支持正数小数。显式重新登场恢复默认体型；转身和移动保留缩放。`rotate` 转到绝对 Z 角度，`dim` 使用当前舞台的压暗颜色和过渡时长，`shock` 在结束后恢复原位置。
- `affinity` 仍不支持自定义角色：它会写官方 `CharacterData` 好感系统，不是纯演出，而 `user:` 角色没有官方好感数据槽。长期状态请使用 Mod 隔离变量，不要伪造官方角色 id。
- 切场景、换脚本时会销毁自定义立绘 GameObject 与 Sprite，避免残留。
- 可用 `samples/showcase3/` 一并验收自定义音频、角色、图片与剧情节点。
