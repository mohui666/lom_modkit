# 用户内容库（User Content Library）

> 语言：简体中文（本文） · [繁體中文](../zh_TW/user_content.md) · [日本語](../ja/user_content.md) · [한국어](../ko/user_content.md)

本地、离线、按 Mod 自包含。没有账号、没有在线市场、没有云同步。

## 怎么导入

1. 编辑器菜单 **文件 → 用户内容库**。
2. 点「导入音频…」或「导入角色…」。
3. 音频：选择本机 `.ogg` 或 `.wav`（单条不超过 20MB），填写显示名称、命名空间、内部名称和用途。
4. 角色：填写显示名称与编号，至少选择 `normal` 默认立绘，可再加 `happy` / `angry` 等表情 PNG。
5. 得到稳定编号，例如：

```text
user:mohui.battle
user:mohui.luoxue
```

命名空间默认取本机用户名的安全形式，可改。同一编号再导入会被拒绝，避免悄悄覆盖。

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

开发时在本机：

```text
%APPDATA%/lom_modkit/repository/audio/<id>/
  content.json
  音频文件
%APPDATA%/lom_modkit/repository/character/<id>/
  content.json
  normal.png
  happy.png
```

这只是编辑器仓库。换电脑、只拷 `.lommod` 的玩家不需要这个目录。

人物介绍图 / 结局插图仍在 `%APPDATA%/lom_modkit/assets/`，剧情里写 `assets/文件名.png`，与本系统分开，以免破坏已有 Mod。

## Story 保存的是什么

音乐 / 音效步骤的 `name`：

```json
{ "type": "music", "name": "user:mohui.battle" }
{ "type": "show", "character": "user:mohui.luoxue", "position": "M", "portrait": "normal" }
```

禁止保存 `C:\Users\...\battle.ogg` 或立绘绝对路径。

## 自定义角色数据格式

```json
{
  "schema": 1,
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

不依赖。导出只复制**当前剧情真正引用**的用户音频进 `.lommod`：

```text
assets/user/audio/mohui.battle/content.json
assets/user/audio/mohui.battle/battle.ogg
assets/user/character/mohui.luoxue/content.json
assets/user/character/mohui.luoxue/normal.png
assets/user/character/mohui.luoxue/happy.png
```

没被引用的导入内容不会打进包。角色被引用时会带上它定义过的全部表情。引用缺失、类型不对、表情不存在、文件坏了：导出直接失败。

## 如何分享 Mod

把导出的 `.lommod` 发给别人即可。对方安装后由游戏插件从包内播放。对方电脑上没有你的用户内容库也能听。

用编辑器「导入 Mod」打开别人的包时，包内用户音频会登记进你的本地仓库，方便继续改。

## 删除资源有什么限制

用户内容库里可以删。如果当前项目的某个音乐/音效步骤，或某句对白的 `say.voice` 还在用这条内容，删除会被阻止，并指出是哪一章哪一步。

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
  "id": "mohui.line_01",
  "type": "audio",
  "name": "师兄早",
  "audio_kind": "sound",
  "files": { "main": "line_01.wav" },
  "character": "user:mohui.luoxue"
}
```

- 自定义角色详情分三个页签：基础信息、立绘、语音。
- 在「语音」页导入会自动关联当前角色；也可试听、重命名、解除关联、删除。
- 旁白 / 系统语音可以不写 `character`。旧音频没有这个字段也继续可用。
- 也可以把用户语音关联到官方人物 id（如 `player`），不会为此生成用户角色对象。
- 对白步骤的语音选择器：人物对白只能选已绑定到当前说话人的语音；旁白只能选未关联角色的语音。未绑定的人物语音不能选。
- 删除语音仍走原来的引用检查：若某句 `say.voice` 还在用，删除会被阻止。
- 打包只收集剧情真正引用的音频。角色下挂了很多未使用语音，也不会打进 `.lommod`。

在编辑器对白步骤里选「对白语音」，或点「导入…」（若当前有说话人，会自动归属到该角色）；「清除」去掉绑定。

## 运行时行为

- 官方名字：原版 Wwise，行为与以前完全相同。
- `user:`：只从**当前正在演出的那个 .lommod** 里找。另一个 Mod 登记了同名 ID 也不会串音。
- 自定义音频用 Windows `waveOut` 播放，不走 Unity `AudioSource`，也不走 Wwise。本游戏主混音是 Wwise，Unity 播了经常没声。
- 音量大致跟随游戏的主音量 × 音乐/音效滑条；不是 Wwise RTPC，不能做到完全一致。
- 自定义 fadeout 是输出音量渐弱（随后仍会按节点等待）。
- 切到自定义音乐时会先停官方背景乐；官方 `StopMusic` 本来就会把环境音一起清掉。
- 回标题、进自由/死亡/结局时自定义音频（含对白语音）会立刻停；官方再播一首 BGM 时会先停自定义 BGM，避免两轨叠在一起。
- 自定义角色不注册进原版 Addressables。`show` / `say` / `hide` / `move` / `face` / `focus` 在编译时改走 `mod_char_*`，由 `CustomCharacterRuntime` 在官方舞台画布上自建 Image。体型按 `scale` 从脚底缩放；朝向按 `art_facing`（默认朝左）再叠节点 `facing`。
- 官方角色路径完全不变。`offset` / `shock` / `dim` / `rotate` / `affinity` 第一版还不支持自定义角色。
- 切场景、换脚本时会销毁自定义立绘 GameObject 与 Sprite，避免残留。
- 可用 `samples/audio_test/` 验收音频；`samples/character_test/` 验收自定义角色（自带两张占位 PNG）。
