# 用户内容库（User Content Library）

本地、离线、按 Mod 自包含。没有账号、没有在线市场、没有云同步。

## 怎么导入

1. 编辑器菜单 **文件 → 用户内容库**。
2. 点「导入音频…」，选择本机 `.ogg` 或 `.wav`（单条不超过 20MB）。
3. 填写显示名称、命名空间、内部名称和用途（音乐 / 音效 / 环境音）。
4. 得到稳定编号，例如导入 `battle.ogg` 后：

```text
user:mohui.battle
```

命名空间默认取本机用户名的安全形式，可改。同一编号再导入会被拒绝，避免悄悄覆盖。

## 支持哪些格式

| 格式 | 说明 |
| --- | --- |
| `.ogg` | Vorbis。第一次播放可能有很短的加载延迟。 |
| `.wav` | PCM 8/16 位或 32 位 float。常见 WAV 可立即播放。 |

不支持 mp3 / flac / aac。文件大小上限 20MB。

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
```

这只是编辑器仓库。换电脑、只拷 `.lommod` 的玩家不需要这个目录。

人物介绍图 / 结局插图仍在 `%APPDATA%/lom_modkit/assets/`，剧情里写 `assets/文件名.png`，与本系统分开，以免破坏已有 Mod。

## Story 保存的是什么

音乐 / 音效步骤的 `name`：

```json
{ "type": "music", "name": "user:mohui.battle" }
```

禁止保存 `C:\Users\...\battle.ogg`。

## 导出后还依赖本机内容库吗？

不依赖。导出只复制**当前剧情真正引用**的用户音频进 `.lommod`：

```text
assets/user/audio/mohui.battle/content.json
assets/user/audio/mohui.battle/battle.ogg
```

没被引用的导入音频不会打进包。引用缺失、类型不对、文件坏了：导出直接失败。

## 如何分享 Mod

把导出的 `.lommod` 发给别人即可。对方安装后由游戏插件从包内播放。对方电脑上没有你的用户内容库也能听。

用编辑器「导入 Mod」打开别人的包时，包内用户音频会登记进你的本地仓库，方便继续改。

## 删除资源有什么限制

用户内容库里可以删。如果当前项目的某个音乐/音效步骤还在用这条内容，删除会被阻止，并指出是哪一章哪一步。

## 对白语音

`say` 可加可选字段 `voice`，值必须是用户内容引用：

```json
{ "type": "say", "character": "player", "text": "师兄，早。", "voice": "user:mohui.line_01" }
```

没有 `voice` 的对白与以前完全一样。有则进入这句时停掉上一句语音并播放，玩家点下一句或剧情结束/中断时停止。普通音效节点不会打断对白语音。

在编辑器对白步骤里选「对白语音」，或点「导入…」；「清除」去掉绑定。

## 运行时行为

- 官方名字：原版 Wwise，行为与以前完全相同。
- `user:`：只从**当前正在演出的那个 .lommod** 里找。另一个 Mod 登记了同名 ID 也不会串音。
- 自定义音频用 Windows `waveOut` 播放，不走 Unity `AudioSource`，也不走 Wwise。本游戏主混音是 Wwise，Unity 播了经常没声。
- 音量大致跟随游戏的主音量 × 音乐/音效滑条；不是 Wwise RTPC，不能做到完全一致。
- 自定义 fadeout 是输出音量渐弱（随后仍会按节点等待）。
- 切到自定义音乐时会先停官方背景乐；官方 `StopMusic` 本来就会把环境音一起清掉。
- 本版本不实现自定义角色立绘运行时；仓库的 `type` 已预留 `character`。
- 可用 `samples/audio_test/` 做验收：自己导入 `user:test.bgm` / `user:test.sfx` / `user:test.env`。
