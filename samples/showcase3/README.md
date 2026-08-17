# 全节点样例 3.0

这是手动实机验收包，构建时按当前 `models.NODE_TYPES` 硬性检查全部 62 种节点。

生成与 CLI 校验：

```powershell
editor/.venv/Scripts/python samples/showcase3/build_showcase3.py
editor/.venv/Scripts/python editor/story_api.py check samples/showcase3/story/main.json --json
editor/.venv/Scripts/python editor/story_api.py compile samples/showcase3/story/main.json --json
editor/.venv/Scripts/python editor/story_api.py pack samples/showcase3 -o samples/全节点样例3.0.lommod --json
```

实际构建可以使用任意安装了项目依赖的 Python。Story 不手写 JSON/Lua：节点由
`story_api` 受控 API 生成，再由同一个 CLI 校验、编译和打包。

实机路径分为五章：演出与用户内容 → Gameplay → Combat（可跳过）→ Battle
（可跳过）→ 死亡画面或安全返回。Battle 的 PlayerDie 沿用原版重试/标题流程，
并不伪造一个可继续的失败分支。

当前 Combat 样例显式选择人物 `artist1`（武师）与独立背景 `center`。Runtime
应即时创建隔离 Combat 配置：人物只决定官方姓名/四帧动画，背景由 `background`
单独决定，血量、属性、技能和行动概率来自节点本身；不得再继承固定 `CL_*`
决斗的姓名、人物、背景、事件或结果。

## 手动测试顺序

1. 从 Steam 普通启动游戏。在标题或自由场景按 `F8`，打开“活侠MOD”。
2. 选择“全节点样例3.0·六十二节点实机验收”并开始演出。
3. 第一章确认用户 BGM、音效、背景、CG、前景图、林灯立绘与语音都能出现；
   人物介绍卡换人后文本应更新。对白区域内部应始终有重复的低透明度
   `MOD / UNOFFICIAL + 包指纹`，右上角还有高对比来源芯片。
4. choice、branch、骰子无论选择/结果为何，都应继续进入第二章，不能只改文字而
   停留在旧节点。
5. 第二章依次观察属性、物品、好感、奖励提示、自定义商店和五类检定。关闭商店
   后应继续；任务、持久变量、activity、自动存档和武学面板不应卡住剧情。
6. 第三章可进入原版 Combat 或跳过。若进入，顶部姓名必须是“武师”，待机、攻击、
   受伤、防御必须全部来自 `artist1` 的原版 Combat 四帧，尺寸和位置保持原版布局；
   背景必须是独立选择的 `center`。不得出现固定壳“唐升”、其他人物动画或
   `StoryViewImage 尚未就绪`。战斗结束后必须回 Story，并显示 Host 判断的
   win/lose 结果和结算提示。
7. 第四章可进入原版 Battle 或跳过。FriendWin/EnemyWin 应回 Story；若触发
   PlayerDie，按原版设计只能重试或回标题，这不算 Mod 分支失败。
8. 终章选择“安全返回 Free”应回自由模式并关闭披露；选择死亡测试应显示带 MOD
   标记的死亡卡并回标题。

若失败，请保留 `BepInEx/LogOutput.log`、编辑器 `crash.log`、最后看到的章节和
节点文案。不要只截黑屏；日志中的 `mod-runtime-error` 会包含 story/node/trace。
