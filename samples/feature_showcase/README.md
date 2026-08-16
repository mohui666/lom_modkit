# Feature Showcase：灯路

这是一个可直接通过 `lomc pack` 编译的自包含样例，不依赖本机用户内容库。它有意保持一章、28 个节点，集中演示：

- 官方角色 `player` 与原创自定义角色 `user:showcase.lin_deng`；
- `normal` / `happy` 两张透明立绘与绑定到角色的对白语音；
- 自定义循环 BGM、一次性 SFX、背景、全屏 CG 和透明 Overlay；
- choice、Mod 会话 flag、隔离战役存档内的数值 flag/variable 分支；
- chs / cht / ja / ko Story 本地化；
- 使用官方 EndGamePanel 版式的自定义结局卡。

构建：

```powershell
cd compiler
python -m lomc pack ../samples/feature_showcase -o ../samples/feature_showcase.lommod
```

不要提交命令生成的 `.lommod`。仓库只保存源项目及正式样例资产。

## 资产来源

人物两种表情、驿站背景、远行 CG 和提灯 Overlay 均为本阶段通过 Codex 内置 imagegen 生成的原创样例美术，没有复用《活侠传》或其他游戏素材。最终提示词要求原创 wuxia visual-novel art、无文字/Logo/水印；人物和 Overlay 明确要求透明 alpha。

三个 WAV 是项目资产，不是人工试玩录音。BGM、SFX 和绑定到角色对白的短人声式 cue 均由 `build_audio_assets.ps1` 中的确定性 PCM 合成器生成，不依赖系统 TTS 或第三方录音。脚本默认拒绝覆盖现有文件，只有显式 `-Force` 才会重建。已提交 WAV 是实际打包输入，玩家无需运行生成脚本。

该样例只展示当前已经过编译器与 Runtime 契约支持的能力。`FEATURE_SHOWCASE_ROUTE_SCORE` 使用现有 `game_flag` / `branch source=flag_value`，并通过 `campaign.new_game=true` 进入 Mod 隔离存档；它不是尚未实现的任意键值变量 API。
