# 用户内容库（User Content Library）

> 语言：简体中文（本文） · [繁體中文](../cht/user_content.md) · [日本語](../ja/user_content.md) · [한국어](../ko/user_content.md)

本地、离线、按 Mod 自包含。没有账号、没有在线市场、没有云同步。

## 作品内容库

点工具栏「用户内容」（或 `⌘L` / `Ctrl+L`），在同一页面管理当前项目的角色、音频和图片。列表显示名称与类型；同名但不同 ID 的资源加数字区分，只改变界面显示。选中后编辑属性、替换素材、预览图片或试听音频，下方可查看引用位置。

有效修改随整个项目保存，不需要再点一次“应用修改”。格式未完成时保留当前输入，并提示修正或「撤销未完成的输入」；删除前仍检查项目引用。

## 怎么导入

1. 打开「用户内容」，点「导入素材…」。
2. 选择图片、角色、音乐、音效或配音，再选择文件。
3. 确认显示名称并点「导入」。名称初值取文件名，可以改成中文；内部 ID 根据作品自动生成，无需先填写编号。
4. 角色可以一次选择多张立绘：第一张作为默认立绘，其余表情使用文件名。表情文件名应以英文字母开头，仅含字母、数字和下划线；也可导入后在「指定表情」处补充。
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

可选字段 `title` 是对话上方的短称号（原版对白名牌那种）。可选字段 `scale` 是体型百分比（50–130，默认 100，脚底对齐站位）；大约 80 接近原版小师妹。可选字段 `art_facing` 是立绘原图朝向（`left` 默认 / `right`）；原版立绘朝左，节点 `facing` 再在这张原图上翻。可选块 `intro` 是介绍卡资料（称号/姓名/正文/同目录图片），在角色页「介绍卡」里编辑。`intro` 步骤选「使用自定义角色介绍卡」时只引用角色，不把正文再抄进节点。

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

用「文件 → 打开 JSON / Mod 包…」打开包时，包内素材直接载入当前项目。它们不会自动登记到共享库，需要复用时再主动保存到共享库。

## 独立分享：Content Pack v1

先把所选内容保存到共享库，再按 `F1` 打开工具页，展开「共享内容库与 .lomcontent」。填写内容 ID、版本、作者与许可，点「导出共享内容包」。内容 ID 可从素材「高级属性 → 复制引用」取得，填写时去掉 `user:` 前缀。导入其他人的内容包则先点「检查待导入的内容包」，核对结果后点「导入此内容包」；之后可「复制此内容到当前项目」。

`.lomcontent` 是离线 ZIP 格式，不连接或上传到任何服务器，包含：`content-pack.json`、`files/` 和 `package-content.sha256`。

`content-pack.json` 固定记录 `content_pack_format=1`、content id/type、SemVer 版本、作者、许可证、规范化 metadata，以及每个声明文件的大小和 SHA-256。导入时会检查安全路径、总大小、类型/ID、metadata、文件清单、逐文件哈希和逻辑内容哈希，全部通过后才原子安装。

内容 ID 在 audio / character / image 三种类型之间全局唯一。若 `user:<id>` 已存在，导入会明确报告冲突并停止，绝不静默覆盖。Content Pack 的哈希用于传输和构建一致性校验，不是作者签名或官方认证。

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

## 运行时行为

- 官方名字：原版 Wwise，行为与以前完全相同。
- `user:`：只从**当前正在演出的那个 .lommod** 里找。另一个 Mod 登记了同名 ID 也不会串音。
- 自定义音频用 Windows `waveOut` 播放，不走 Unity `AudioSource`，也不走 Wwise。本游戏主混音是 Wwise，Unity 播了经常没声。
- 音量大致跟随游戏的主音量 × 音乐/音效滑条；不是 Wwise RTPC，不能做到完全一致。
- 自定义 fadeout 是输出音量渐弱（随后仍会按节点等待）。
- 切到自定义音乐时会先停官方背景乐；官方 `StopMusic` 本来就会把环境音一起清掉。
- 回标题、进自由/死亡/结局时自定义音频（含对白语音）会立刻停；官方再播一首 BGM 时会先停自定义 BGM，避免两轨叠在一起。
- 自定义角色不注册进原版 Addressables。`show` / `say` / `hide` / `move` / `face` / `focus` / `offset` / `shock` / `dim` / `rotate` 在编译时改走 `mod_char_*`，由 `CustomCharacterRuntime` 在官方舞台画布上自建 Image。体型按 `scale` 从脚底缩放；朝向按 `art_facing`（默认朝左）再叠节点 `facing`。
- `offset` 累加舞台坐标，`rotate` 转到绝对 Z 角度，`dim` 使用当前官方舞台的压暗颜色和过渡时长，`shock` 在结束后恢复原位置；官方角色路径完全不变。
- `affinity` 仍不支持自定义角色：它会写官方 `CharacterData` 好感系统，不是纯演出，而 `user:` 角色没有官方好感数据槽。长期状态请使用 Mod 隔离变量，不要伪造官方角色 id。
- 切场景、换脚本时会销毁自定义立绘 GameObject 与 Sprite，避免残留。
- 可用 `samples/showcase3/` 一并验收自定义音频、角色、图片与剧情节点。
