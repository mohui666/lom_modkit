# 复杂剧本委托：雾渡夜灯

本文件是供 Python 对象 API 制作的原创剧本与验收要求。目标是验证新对话只依靠公开接口文档，也能写完整 Python 文件，一次生成可编辑工程和 MOD。不要复制全节点样例的章节或生成代码。

## 故事

秋雨后，雾渡镇的引航灯一夜之间熄了三次。新任守灯人沈砚被指控偷走灯油，赶船的旅人带着一封被水泡过的账单进镇。玩家须在潮涨前查明真相，也可以选择不介入。没有战斗，用调查、对话、选择、旗标和并行演出来表现冲突。

角色：旅人使用官方 player；沈砚使用自定义人物 user:mist.shen，至少 normal/happy/angry 三种表情。测试美术可由 prepare_assets.py 生成几何示意，不能包含官方提取素材。人物动作必须先登场；移动后沿用表情，震动和气泡同时出现。

## 章节与剧情

### main：雾渡来信

雨夜渡口。叙述“这座镇子靠一盏灯认得回家的方向，今夜灯却又灭了。”沈砚抱着灯架出现，衣袖沾满黑油。旅人：“灯灭了三次，你却还在这里修它？”沈砚：“灯是我看着的，错也该由我担。只是账上的油，不该只剩这一点。”展开信件插图或书页，让读者看到“九月初三，灯油十二坛；实际入库，八坛。”随后收起。

选项 accept_job：0“留下，替他查账。”；1“潮要涨了，今夜先走。”接受进入 market；拒绝进入 ending_left。记录的旗标只属于本段演出，跨事件的状态写 scenario 任务与结果。

### market：未打烊的集市

可循环调查的三选项 market_action：0“问摆渡人昨夜的钟声。”；1“检查药铺背后的油坛。”；2“带着线索去旧桥。”

摆渡人说“第一遍钟响时灯还亮着，第三遍才灭。守灯人那时在我船上，替我绑断了的桨。”设置 heard_bell 旗标。调查油坛发现账单夹在坛底，沈砚说“原来不是我记错了。”展示插图，设置 kept_letter 旗标。两段各自回到 market_action，再进入 crossing。旅人能先去任意调查，也能两项都不查。不得因循环把问询后的节点错误串进另一条选项。

### crossing：潮过旧桥

旧桥震动，桥下有条空船被绳索拴住。沈砚先用 angry 表情质问自己的莽撞，再移动到另一站位，表情应保持。用 Parallel 让同人物 Shock 与 Mood(icon=shock) 同时发生，且能在编辑器预览拖动组内气泡调整位置。

选项 bridge_action：0“先松船绳，给镇里留条退路。”；1“带着线索直上灯塔。”前者设置 boat_freed，后者不设。两条都进入 tower。移动＋气泡、音效＋插图淡入另各示范一组合法并行动作，禁止同资源冲突。

### tower：灯下对质

管事只在旁白中出现，不借用猜测的官方角色编号。旅人出示调查结果。若 heard_bell 与 kept_letter 都有：有证据可以揭穿挪用；否则只够帮沈砚熬过今夜。给出最终选择 tower_action：0“先护住镇上的人。”；1“当众说出账本上的名字。”

护人且 boat_freed 为真，去 ending_saved；未松船绳则去 ending_left。揭穿且两项证据齐全，去 ending_exposed；证据不足则去 ending_left。使用结构化 Branch，不用 raw 或伪造 Lua。保留对每次缺少证据的具体解释对白。

### ending_saved：一船灯火

船在桥断前接住避雨的人。沈砚：“账可以明日再算，人不能明日再救。”旅人：“你记着这句话，也别忘了账。”以自定义 CG／背景、音乐淡出与人物退场结束，End(event_result=rescued)。

### ending_exposed：灯照旧账

账本摊在灯下，巡夜人找到藏起的四坛油。沈砚：“我以为熬过今夜就好。”旅人：“把今夜说清楚，明夜才不会重来。”沈砚转为 happy 表情，灯重新亮起，End(event_result=exposed)。

### ending_left：潮声照旧

拒绝调查、救人无船或指控无证据三条路线都能到此，但上一章须有不同解释。旅人离开，沈砚继续修灯。旁白“天亮后镇上仍会说起昨夜，只是每个人说的都不一样。”End(event_result=left)。

### epilogue：翌日回信

这是独立后续事件，不是开场强制续章。任务已 resolved，且回信尚未领取时可触发。沈砚归还修好的灯架，旅人说“这回的账，你自己收好。”展示自定义信件，End(event_result=completed)。结果把任务置为 thanked、变量 letters_received 加一。不发放猜测的官方物品或修改玩家属性。

### daily_bridge / daily_lamp：可重复日常

同地点 Center、同优先级、同 check=action 的 30:70 事件池；任务 thanked 后才可触发。一个讲修桥，一个讲添灯，各有短对话与表情、音效或气泡。End 回报 completed，结果给自有变量 daily_visits 加一。不要改原版时间或任务。

## 工程要求

1. sandbox，自有 campaign_id 由核心生成；10 个章节，至少 100 个顶层节点、至少 22 种节点类型。增加的演出要符合剧情，不堆无意义等待或重复文字。
2. 两次连续选择、调查循环、跨章跳转、嵌套旗标条件、三个结局、独立后续事件、可重复地点事件池。
3. 场景／立绘／音乐／音效／插图采用自有测试素材；使用 Show/Portrait/Move/Offset/Face/Dim/Focus/Hide/HideAll/Say/Choice/Branch/Flag/Parallel/Shock/Mood/Music/Sound/Background/Overlay/CustomCg/Cg/Transition/Wait/End 等适合的对象。对可用枚举查 schema，不猜字段。
4. scenario 至少两项任务：investigation（active/resolved/thanked/abandoned），harbor（unsafe/safe）；开场结果 rescued/exposed 令前者 resolved，left 令 abandoned；rescued 令 harbor safe，其余维持 unsafe。配置必须含 completed 结果。
5. main 中至少一句和一个选项有 cht/ja 翻译，schema/ID/路径不翻译；用 texts 查询路径。
6. 至少 6 条可通过的离线测试：完整调查＋松船＋护人、完整调查＋揭穿、证据不足＋揭穿、完整调查但不松船＋护人、开场拒绝、先查油坛再问摆渡人的逆序调查。actions.choices 的 option 是 0 基下标，同名 choice 的重复记录按顺序消费。
7. 以完整 Python 文件实现。主入口导出 SDK → import 构造函数 → Project/Chapter → cli.build → cli.export；脚本支持指定编辑器路径、SDK 路径和新输出目录。所有可重跑输出使用新目录，不能删除已有结果来掩盖失败。
8. 校验全部通过后，把原工程与包分别交给实际编辑器 --smoke-preview；至少做一次 Project.open → 修改对白 → 另存 → 导出 → 重新打开，并确认资源和 campaign_id 保留。

## 独立制作的交付

先读 docs/chs/ai_cli.md。允许查询 CLI schema nodes/catalog/forms，但不得复制其他对话的生成器实现。把你的代码、请求 JSON、工程、MOD 和简短验证结果放到自己唯一的 out/python-api-independent-* 目录。不要改仓库源代码、CLI 文档、其他对话的产物，也不要提交或发布；发现 API 或文档问题在结果中写清复现、预期、实际。不要安装游戏 Host、运行 F5 或修改真实游戏／存档。

编译、编辑器读写、离线模拟与游戏实机分开报告。随机音频、气泡运动、scenario 调度和游戏表现不能仅凭离线断言宣称实机通过。
