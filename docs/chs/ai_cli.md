# Python 对象 API 与 Rust CLI

本文对应 1.6.0。查签名与字段：[Python 接口参考](python_api.md)；查命令输入输出：[CLI 参数参考](cli_parameters.md)；AI 工作流：[lom-modkit-authoring skill](https://github.com/mohui666/lom_modkit/blob/v1.6.0/skills/lom-modkit-authoring/SKILL.md)。SDK 导出后本文为根目录 `README.md`，两份参考文档与它同目录，skill 位于 `skill/lom-modkit-authoring/`，可复制到自己的 Codex skills 目录。

推荐用户和 AI 使用 [Python 节点构造函数](#主要入口python-构造函数)批量制作剧本：每个节点是对象，构造时检查参数，生成失败抛出含完整诊断的异常。安装 Python 3.10+ 即可，无第三方包依赖。底层 Rust 核心负责校验、保存为编辑器工程、编译和导出；需要其他工具接入时也能直接使用下面的 Rust CLI。包／章节／内容契约保持 **3 / 2 / 1**。

## 直接使用 exe

Windows 正式附件只有内置组件的 `lom-editor.exe`；下面的 CLI、SDK 导出、舞台出图和后台视频均从这个 exe 调用。`request.json` 是工程生成说明：

```json
{
  "manifest": {"id":"demo", "name":"山门初见", "entry":"main"},
  "stories": {
    "main": {
      "story_schema":2, "id":"main", "title":"第一章", "start":"s",
      "nodes":[
        {"id":"s", "type":"say", "mode":"narrative", "text":"山雨初歇，远处传来钟声。"},
        {"id":"e", "type":"end"}
      ]
    }
  }
}
```

```powershell
.\lom-editor.exe --cli project request.json -o .\作品 --json
.\lom-editor.exe --cli preflight .\作品 --profile release --json
.\lom-editor.exe --cli export .\作品 -o .\作品.lommod --json
.\lom-editor.exe --cli inspect .\作品.lommod --json
```

生成 `作品/manifest.json` 和 `作品/story/main.json`，编辑器可打开目录或 MOD 包继续编辑。普通步骤依数组顺序执行，分支和 `goto` 使用节点 ID。请先使用新输出目录；不会自动覆盖不属于当前来源的工程。

批量多章节示例见 [samples/cli-authoring/request.json](https://github.com/mohui666/lom_modkit/blob/v1.6.0/samples/cli-authoring/request.json)，包含两个章节、选项分支和跨章跳转。AI 可一次提交整个 stories 集合，不必逐节点启动命令。源码目录中可执行 `lomc project samples/cli-authoring/request.json -o out/cli-story --json`。

源码构建：`cargo build --locked --release -p lomc`。把本文的 `lom-editor.exe --cli` 换成 `target/release/lomc.exe` 即可使用独立 CLI；其他平台为 `lomc`。两种入口使用同一个 Rust 编译器。

## AI 查询接口

不要猜字段、人物编号、表情或音效名：

```powershell
.\lom-editor.exe --cli schema nodes -o nodes.json --json
.\lom-editor.exe --cli schema catalog -o catalog.json --json
.\lom-editor.exe --cli schema forms -o forms.json --json
.\lom-editor.exe --cli schema defaults -o defaults.json --json
.\lom-editor.exe --cli schema project --json
```

目录可用 `schema catalog --filter "雨"` 先筛选；写出大文件时加 `-o catalog.json --summary --json`，文件保留完整内容，标准输出只返回简短计数。`schema nodes` 等大结果也支持这个组合，`--summary` 必须配合 `-o`。图像和布局证据用下文的 `asset-query` 查询，目录中的编号本身不能证明素材外观。

| 入口 | 内容 |
| --- | --- |
| `schema nodes` | 所有节点的必填／可选字段、类型、枚举和通用字段；以此为接口契约 |
| `schema catalog` | 编译器内嵌的官方人物、表情、背景、音乐、音效等清单 |
| `schema forms` | 编辑器中文名称、字段说明、表单参数；界面百分比可能不同于存储数值 |
| `schema defaults` | 编辑器草稿默认值；空必填值须补全，高级类型以 nodes 为准 |
| `schema project` | 工程请求字段说明 |

给 AI 的任务可以写：

> 阅读 ai_cli.md，查询 schema nodes/catalog，用节点对象制作剧本。通过 project 生成可编辑工程，按体检的章节／节点位置修正错误，再 export 和 inspect。保留原工程与存档身份，不猜测官方资源 ID。

编排演出前先读 skill 的[演出节奏与站位](https://github.com/mohui666/lom_modkit/blob/v1.6.0/skills/lom-modkit-authoring/SKILL.md#演出节奏与站位)，按剧情处理位移、动作等待和人物距离；工程校验不代替这些演出判断。

完整嵌套条件、选项、奖励和原版来源信息见 [Mod 格式参考](https://github.com/mohui666/lom_modkit/blob/v1.6.0/docs/chs/mod_format.md)。原版整章来源信息不能凭空编造。

## 工程请求字段

| 字段 | 作用 |
| --- | --- |
| `source` | 可选：已有工程目录、章节 JSON、Lua 或 `.lommod`；沿用原生读取和兼容检查 |
| `mode` | 新建可选 `story`（默认）、`addon`、`sandbox`、`vanilla_patch`；使用 source 时不传 |
| `target` | 新建 vanilla_patch 的原版章节编号 |
| `manifest` | 合并作品顶层配置，保留来源未修改的字段和 campaign_id；新建自动生成独立 campaign_id |
| `stories` | 章节 ID 到完整章节对象的映射；若提供则**替换整个章节集合**，省略则保留来源 |
| `scenario` | 完整事件／任务对象；省略保留来源或新建默认值，null 表示移除 |
| `assets` | 包内 `assets/` 路径到本地来源文件路径的映射；合并来源资源 |
| `remove_assets` | 明确移除的包内 assets 路径数组 |

相对 `source` 和资源文件路径相对于请求 JSON；输出 `-o` 相对于命令工作目录。`project` 输出必须是工程目录，打包使用 `export`。删除章节、scenario 或资源后须另存新目录；原目录不会被删除，也不会留下似乎已删除却会重新读入的旧文件。

保存前执行完整工程体检，并实际编译、打包、用编辑器的包读取器重开临时包；通过后才写输出，再用编辑器工程读取器重开输出。成功响应的 checks 列出这些已执行步骤，不仅检查 JSON 语法。写入沿用编辑器的路径和文件归属检查；磁盘 IO 失败不承诺整个目录原子回滚。旧作品源文件、节点、资源、未知元数据和存档身份通过原生 Project 读取保存；已有包的 Lua 校验仍然执行。

`project-read SOURCE --json` 返回 `project` 工程请求及单独的资源路径列表；`-o` 保存完整响应。修改后作为 `project` 输入时取其中的 `project` 对象。增删章节时同步修改入口和跨章引用。

素材涵盖人物、扩展表情、图片、音频、帧动画、自定义物品等，仍使用现有 `content.json`，例如：

```json
{"assets":{
  "assets/user/image/demo.bg/content.json":"./content.json",
  "assets/user/image/demo.bg/bg.png":"./bg.png"
}}
```

此片段合并到工程请求。具体字段见 [用户内容](https://github.com/mohui666/lom_modkit/blob/v1.6.0/docs/chs/user_content.md)、[自定义玩法](https://github.com/mohui666/lom_modkit/blob/v1.6.0/docs/chs/custom_gameplay.md)。事件和任务使用 [事件任务格式](https://github.com/mohui666/lom_modkit/blob/v1.6.0/docs/chs/events_and_tasks.md)；翻译放入章节 `localization`，离线测试放入 `_editor.tests`。所有现有节点和嵌套动作均能直接用对象表达。

## 主要入口：Python 构造函数

先导出随 exe 内嵌的标准库 SDK；运行脚本需要 Python 3.10+，编辑器和底层 Rust CLI 本身不需要 Python：

```powershell
.\lom-editor.exe --cli sdk -o .\python-authoring --json
cd .\python-authoring
python .\example.py C:\Tools\lom-editor.exe C:\Stories\mountain
```

导出 `lom_modkit/__init__.py`、`authoring.py`、`presentation.py`、配套 `contract.json`、简单示例、`complex_story.py`、配套自有素材生成器和剧本委托、三份指南与 AI skill。目标文件存在则拒绝覆盖，升级请导出到新目录。Python 需要自己已有的 3.10+ 解释器，SDK 不安装第三方依赖。

```python
from lom_modkit import CLI, Chapter, End, Project, Say, Show

cli = CLI(r"C:\Tools\lom-editor.exe")
chapter = Chapter(id="main", title="第一章", nodes=[
    Show(id="enter", character="artist1", position="M", portrait="normal"),
    Say(id="greeting", character="artist1", text="山路难行，且歇歇脚。"),
    End(id="finish"),
])
project = Project(id="my_story", name="山路", author="作者", chapters=[chapter])
cli.build(project, r"C:\Stories\山路工程")
cli.export(r"C:\Stories\山路工程", r"C:\Stories\山路.lommod")
```

每个节点类型均有对应构造函数，名字按下划线分词后首字母大写：`say → Say`、`custom_cg → CustomCg`、`native_branch → NativeBranch`；也可 `NODE_TYPES["say"](...)`。参数均为关键字，必填字段不自动填空。`help(Say)` 和 `inspect.signature(Say)` 可查询签名。

```python
from lom_modkit import Choice, Move, Option, Parallel, Shock, Mood

choice = Choice(id="choose", options=[
    Option(text="问路", goto="ask"), Option(text="休息", goto="rest"),
])
# Python 关键字加尾部下划线，序列化后恢复为 from。
move = Move(id="walk", character="artist1", from_="M", to="R", duration=1.0)
group = Parallel(id="surprise", actions=[
    Shock(character="artist1", duration=0.5).action(),
    Mood(enabled=True, character="artist1", icon="shock").action(),
])
```

同时动作不带 id/goto，`.action()` 检查支持的类型并产生子对象。资源冲突由 CLI 检查。`Node.from_dict(data)` 还原已有节点，`.to_dict()` 得到 JSON，`.replace(text="新文本")` 重新检查参数后返回新对象。

构造时检查未知字段、必填项、基本类型、枚举、重复 ID 和入口。复杂条件、跳转目标、资源引用、并发冲突和游戏规则由 CLI 校验；构造成功不表示已经通过完整体检。

`Chapter(id=..., title=..., nodes=[...], start=None, **metadata)` 默认入口为首节点，元数据支持 mood/localization/_editor 等现有字段。`Project` 支持 `author`、`version`、`entry`、`mode`、`target`、`scenario`、`assets`、`manifest`；assets 的本地文件转成绝对路径，manifest 的同名项以显式参数为准。`project.write(path)` 保存请求 JSON。

可选 `NodeIds`／`Staging` 处理编号、登退场、动作组合和声明过的站位恢复，具体签名见 [演出助手](python_api.md#可选演出助手)。它们返回原有 Show／Hide／Parallel 对象，不推测游戏当前状态、人物脚底或素材裁切。

```python
old = Project.open(cli, r"C:\Stories\旧作品.lommod")
old.stories["main"]["nodes"][0]["text"] = "修改后的开场。"  # 先确认节点类型和字段
old.manifest["version"] = "1.0.1"
cli.build(old, r"C:\Stories\修改版")
```

`Project.open` 保留原配置、资源及来源；可修改或替换 `old.stories`、`old.manifest`，完整工程请求在 `.recipe`。`CLI.run(...)` 支持所有 CLI 命令，失败抛 `CLIError`，`.result` 保存完整诊断，`.returncode` 为退出码。

`Project.open` 的 `.recipe` 使用 `source` 保留原资源，通常没有内联的 `assets` 映射；不要因它没有 assets 键就认为素材丢失。导出会规范化 metadata 的 JSON 排版并省略已知默认值；PNG/WAV 可比较字节，JSON 应比较解析内容和默认值含义。

## 事件、任务、翻译与素材构造函数

不必手写事件配置的嵌套结构：

```python
from lom_modkit import (Task, Event, Scenario, Condition, All, Outcome,
                       VariableChange, Localization, Translation, ImageAsset)

task = Task(id="delivery", name="送信", states=["active", "completed"], initial="active")
scenario = Scenario(entry_event="opening", tasks=[task], events=[
    Event(id="opening", name="开场", script="main", trigger="start"),
    Event(id="delivery_event", name="交信", script="delivery_chapter", trigger="position",
          position="Center", when=Condition(source="task", key="delivery", op="eq", value="active"),
          outcomes={"completed": Outcome(tasks={"delivery": "completed"},
                     variables={"visits": VariableChange(op="add", value=1)})}),
])
translations = Localization(default_locale="chs", translations=[
    Translation(locale="cht", texts={"greeting.text": "山路難行。"}),
    Translation(locale="ja", texts={"greeting.text": "険しい山道だ。"}),
])
picture = ImageAsset(id="demo.letter", name="来信", main="letter.png")
# Chapter(..., localization=translations)
# Project(..., mode="sandbox", scenario=scenario, assets=[picture])
# 节点使用 picture.ref，即 user:demo.letter。
```

示例中的 `delivery_chapter` 和 `greeting` 必须实际存在。每个开场事件自动生成空条件 `All()`，必须启用、概率为 100，不设置前置条件。完整上下文校验仍在 build 中完成。

| 构造函数 | 关键字参数／结构 |
| --- | --- |
| `Condition` | source、key、op、value；source=time/stat/affinity/flag/game_flag/variable/task，op=eq/ne/lt/le/gt/ge；task 仅 eq/ne 且值为状态 ID，其他值为 Int32 |
| `All(*条件)` / `Any(*条件)` / `Not(条件)` | 条件组合；All() 为真，Any() 为假 |
| `Task` | id、name、states、initial、transitions=()；检查重复状态与初始状态 |
| `TaskTransition` | from_、to、when；转换状态须在任务 states 中 |
| `VariableChange` | op=set/add、Int32 value |
| `Outcome` | tasks={任务ID:状态}、variables={变量ID:VariableChange(...)}、actions=[OutcomeAction(...)]；均可省略 |
| `OutcomeAction` | type=stat/affinity/game_flag 时传 key/op/value；flag 仅 key；talent 传 key/value=±1；item 传 key/kind/op/value；time 传 year/month/stage |
| `Event` | id/name/script/trigger 必填；trigger=start/auto/position/story_entry；position、when、outcomes 可选；story_entry 必填 target，可选 from_（序列化为 from）；其他触发不能填 target/from_；once=True、enabled=True、check=stage、priority=0、chance=100、weight=1；可填 pool 和 weight_modifiers |
| `WeightModifier` | when、add（-100000～100000） |
| `Scenario` | events、tasks=()、entry_event=None；sandbox 设置 entry_event，addon 不设置；检查任务引用与重复 ID |
| `Translation` | locale=cht/ja/ko 等、texts={可翻译路径:文字} |
| `Localization` | default_locale=chs、fallback_locale=默认语言、translations=[Translation(...)]；也接受语言到文字表的映射 |
| `ImageAsset` | id、name、main=本地图片路径 |
| `AudioAsset` | id、name、main、audio_kind=music/sound/env、可选 character |
| `CharacterAsset` | id、name、main、portraits={表情ID:本地图片路径}；额外设置如 scale、extends、portrait_layouts 与现有 metadata 相同 |
| `Asset` | 通用素材：type、id、name、main，以及该类型的 metadata 参数；覆盖 image/audio/character/item/martial/support/skill，玩法细则由 Rust 校验 |

素材对象通过 `Project(assets=[...])` 传入。`main` 是本地文件，`.ref` 是供节点引用的 `user:` 名称。人物可 `CharacterAsset.from_folder(id="demo.actor", name="人物", folder="portraits", prefix="shen_")`：只读该目录匹配前缀的图片，去掉前缀后生成表情 ID，优先使用 normal，其余同名加后缀。不会修改源图片。高级素材的附加文件可通过 `Asset(files={包内文件名:本地路径}, ...)` 提供。

底层工程请求的 assets 值除了本地路径，还支持 `{"json": {...}}` 内嵌 JSON metadata；只接受 .json 包内路径。构造函数使用此形式生成 content.json，无需作者手写临时文件。构造函数与字典可以混用，均接受同一 Rust 校验。

## 离线路线测试：完整格式

推荐使用构造函数，避免把预期写错字段后遗漏检查：

```python
from lom_modkit import ChoiceStep, RouteTest

test = RouteTest(name="问路后抵达", story="main",
    choices=[ChoiceStep(node="choose", option=0)],
    reaches_node=["finish"], reaches_ending=True,
    flags={"asked": True})
chapter = Chapter(id="main", title="第一章", nodes=nodes,
                  _editor={"tests": [test]})
# 保存后 cli.run("test", project_dir)
```

`RouteTest` 另支持 initial_flags、initial_variables 和断言 variables。上面的测试需对应真实节点与旗标。原始 JSON 同样可用：

```json
{"name":"问路后抵达","story":"main",
 "initial":{"variables":{},"flags":{}},
 "actions":{"choices":[{"node":"choose","option":0}]},
 "assert":{"reaches_node":["finish"],"reaches_ending":true,"flags":{"asked":true}}}
```

它放在章节 `_editor.tests` 数组中。story 必填；option 从 0 开始，同一 choice 的多条动作按顺序消费，适用于调查循环。`assert.reaches_node` 为一个 ID 或 ID 数组；没有章节名限定，建议验收节点使用有辨识度的 ID。可检查 reaches_ending、variables、flags。配置中的未知字段（包括误写的 expect）会报错。`unsupported` 表示离线引擎没有执行该类型，不能视为通过；随机骰子、战斗和 scenario 调度等仍需作者实机确认。

## 资源、位置调整与试玩操作

编辑器的 Rust CLI 提供以下操作；独立 lomc 只提供前述编译／工程命令。先查询契约：

```powershell
.\lom-editor.exe --cli automation-schema --json
.\lom-editor.exe --cli preview request.json --json
```

这些命令接收一个请求 JSON，相对路径按请求文件所在目录解析。Python 可统一调用 `cli.operation("命令", **参数)`，会把 project/game/runtime/output 路径转为绝对路径，再发送给 Rust，失败仍抛 CLIError。

| 命令 | 请求字段与作用 |
| --- | --- |
| `extract-assets` | game、output：只读本机游戏，output 作为缓存父目录，结果返回新素材库路径 |
| `extract-story` | game，可选 target：原版章节清单或指定章节源码／译文 |
| `extract-routes` | game：原版章节路线索引 |
| `extract-audio` | game、kind=music/sound/env、name、output=新 .wav 路径；提取并解码一段音频 |
| `asset-query` | 可选 library/project/game、kind/query/id/portrait/position、分页和缩略图参数；返回实际图片、透明边界和有来源的布局资料 |
| `preview` | project、script、node：返回离线预览最终状态及能力说明，明确未渲染图片 |
| `render-preview`／`render-previews` | project、单帧 script/node 或 frames、output；生成纯舞台 PNG，批量另含对照图和清单，复用同一隐藏图形会话 |
| `render-video` | project、game、output；同步生成游戏原生 MP4，默认独立桌面后台 Worker，按自动播放时长及预设选项推进 |
| `open-editor` | project，可选 output/script/node：读取工程、MOD 或 Lua；output 提供时另存为编辑器工程目录，再打开新窗口，可定位章节和节点 |
| `preview-adjust` | project、script、node、target、x、y、output；可选 scale（默认100）、effect_scale=[横缩放,纵缩放]；复用编辑器拖动的修改规则并保存工程 |
| `prepare-playtest` | project、script、output，可选 node（默认章节入口）：生成隔离演出试玩包，不安装或启动游戏 |
| `playtest` | project、script、game，可选 node/runtime：安装／更新 Host、发布试玩请求并返回 request_id，不把发出请求当作已进入剧情 |
| `playtest-status` | game、request_id：查询一次当前可读的执行证据，不启动游戏或自动轮询 |
| `prepare-event-playtest`／`playtest-event` | project、event_id、output 或 game；前者仅生成隔离包，后者走真实事件调度；story_entry 默认从原版前置章播放 |
| `capture-effects` | game，可选 runtime/names：调用现有游戏采样流程，启动游戏并生成粒子预览 |

target 为 `{"kind":"mood","id":"人物ID"}`，也可 kind=character/overlay（id 为 slot）/cg；effect 使用 `{"kind":"effect","node":"特效源节点ID","action":组内序号}`，action 可省略。气泡偏移与人物偏移以舞台原始单位保存；图片 x/y 为百分比，scale 为百分比；effect_scale 对应有符号的两轴缩放。目标定位、路径和冲突规则与编辑器一致。

```python
state = cli.operation("preview", project="作品", script="crossing", node="shock_and_mood")
images = cli.query_assets(library="本机预览库", kind="character", query="小师妹", limit=6)
cli.render_previews("作品", [
    {"script":"crossing", "node":"shock_and_mood"},
    {"script":"crossing", "node":"after_move", "time":0.4},
], "关键帧", library="本机预览库")
cli.operation("preview-adjust", project="作品", script="crossing", node="shock_and_mood",
    target={"kind":"mood", "id":"user:mist.shen"}, x=36, y=24, output="调整版")
cli.operation("prepare-playtest", project="调整版", script="crossing",
    node="shock_and_mood", output="试玩.lommod")
# 导入包到新工程并打开界面，保留原包。
cli.open_editor("作品.lommod", output="导入工程", script="crossing", node="shock_and_mood")
```

直接 CLI 打开：请求文件写 `{"project":"作品.lommod","output":"导入工程"}`，然后 `lom-editor.exe --cli open-editor request.json --json`。不传 output 时直接打开原工程或包，不自动保存。读取失败或定位无效不会打开窗口；成功响应包含新窗口进程 pid。

素材结果中的锚点是舞台 Transform 锚点，不是可见脚底；`framing=unknown` 时需打开缩略图或原图确认半身／全身。`rendering_performed` 与 `visual_review_performed` 分别说明是否真的绘制、是否有人看过；`preview_states` 只计离线模拟状态。纯舞台图不执行原版前置章，不能检查原版遗留的菜单或 UI。

`playtest` 返回 request_id 后，用 `cli.playtest_status(game, request_id)` 查询一次。sent／game_started、chapter_entered／node_entered 分别对应发布／进程与实际脚本／节点证据，restart_required 需退出游戏后重发。事件请求另区分原版前置进入、事件 ID 获选、事件 Lua／节点进入与 event_not_triggered；准备章不会充当所测事件成功。

`story_entry` 的 `playtest-event` 默认 `entry_mode="predecessor"`，用显式 source_script 或事件 from 作为真实原版起点，沿原版选项和跳转进入事件；不强跳 target 或额外清 UI。显式 boundary 只检查入口边界。完整参数、时钟与条件规则见 [CLI 参数](cli_parameters.md#原版前置章节与事件衔接试玩)。

`render-video` 默认 1080p30、auto_speed=game，预设选项序号从0开始，未指定默认首个可用选项；可选720p、60 FPS和前台模式。它直接取游戏合成帧与原生音轨，后台任务使用独立 Host／临时包／存档，不要求手动录屏。普通试玩与视频均关闭可见、隐藏水印，包身份和存档隔离不变。CLI 在 stderr 输出进度，SDK 方法等待最终成片；不会自动替代画面验收。

`prepare-playtest` 不推进 scenario 任务或发奖励。`playtest/playtest-event/render-video/capture-effects` 会操作游戏，只在作者明确调用时执行；不会因 build/export 自动启动。视频遇到当前不支持的战斗、商店等人工交互会明确停止，不能跳过后声称完整生成。

## 复杂样例与重跑

[《雾渡夜灯》完整 Python 文件](https://github.com/mohui666/lom_modkit/blob/v1.6.0/samples/python-api-complex/build_story.py)由独立新对话按[剧本委托](https://github.com/mohui666/lom_modkit/blob/v1.6.0/samples/python-api-complex/BRIEF.md)制作，随后更新为使用新增的配置构造函数。10 章、184 个顶层节点、24 类节点；包含调查循环、跨章旗标、三个结局、4 个事件、2 项任务、30:70 日常池、自定义图像／人物／音频、cht/ja 译文和 8 条离线路线。美术为自有几何示意，音频为合成测试音。

在仓库根目录，指定尚不存在的 SDK 和输出目录：

```powershell
python samples/python-api-complex/build_story.py --editor C:\Tools\lom-editor.exe --sdk out\mist-sdk --output out\mist-run
```

脚本导出 SDK、生成原生工程与包、运行体检／路线断言／预览冒烟，再修改对白、另存、导出和重开核对资源与存档身份。保留已失败结果，重跑请换输出目录。`--smoke-preview` 检查真实编辑器的读取、编译和预览状态，不能等同于鼠标拖动或游戏实机验收。静态跨章旗标预警和缺少未要求语言的预警须由作者判断，不以“消除预警”为由伪造剧情数据。

只持有 exe 时，先用 sdk 命令导出，再运行导出目录内的 `complex_story.py --editor 编辑器路径 --sdk 新SDK目录 --output 新输出目录`，也能完成这份复杂样例，无需源码仓库。目录中的 README 即本页。

## 校验与其他命令

自动化统一传 `--json`，输出为 UTF-8；成功退出 0，参数、校验或 IO 失败退出 1，水印未检出为 2。诊断包括 `errors` 或带 `story_id/node_id/message` 的 `issues`。

```sh
lomc check PROJECT/story/main.json --json
lomc compile PROJECT/story/main.json -o OUTPUT/main.lua --locale cht --json
lomc texts PROJECT/story/main.json --json
lomc analyze PROJECT --json
lomc test PROJECT --json
lomc statistics PROJECT --json
lomc preflight PROJECT --profile editing --json
lomc preflight PROJECT --profile release --json
lomc export PROJECT -o OUTPUT/work.lommod --json
lomc inspect OUTPUT/work.lommod --json
lomc release PROJECT -o OUTPUT/release --json
```

`texts` 输出可翻译路径。`test` 读取 `_editor.tests`，unsupported 不等同通过。`compile` 只生成 Lua，不能替代整工程导出和资源检查；`release` 只生成本地发布文件。`export` 支持原生可读的工程和包，原 `pack DIRECTORY` 入口保留。

check／preflight／test 与 `--smoke-preview` 的离线校验不能代替 Windows 游戏实机试玩或人工看图。需要图片时显式调用 render-preview(s)，需要原版衔接时调用 playtest-event，成片使用 render-video；这些是不同的执行与验证范围。

## 原受控操作兼容

`author request.json --json` 接受 `{"op":"操作名","params":{...}}`，返回 `{"ok":true,"result":...,"after":...}`；`-o` 保存完整响应，输入不改写。

| 操作 | 参数；操作已有章节时均传 story |
| --- | --- |
| new_story | story_id、title、可选 mood；生成待补正文的草稿 |
| new_node | node_type、node_id；生成默认草稿 |
| get_node / list_nodes | 前者另传 node_id |
| add_node | node_type、fields、可选 after |
| update_node | node_id、fields |
| delete_node / set_start | node_id |
| rename_node | node_id、new_id；重写结构化跳转 |
| move_node | node_id、delta（±1） |
| add_say | text、可选 character/mode/portrait/after |
| add_scene / add_choice | view / options（2～4 个 [text,goto]） |
| add_dice | maximum、header、bands、bonus 等 |
| add_death | death_id、text、可选 title/next（Title） |

`edit story.json --operations operations.json [-o edited.json] --json` 批量操作，每项省略 story，共享上一步结果；失败不写回。不带 -o 时改写原章节；不接受 new_story/new_node。旧操作沿用表单检查，完整原生字段使用对象与 project。

其他既有命令包括 new-story、migrate、content-inspect、content-import、play-bundle、install-play-bundle、detect-watermark、detect-watermark-video。完整参数见 `--help`、[Rust 工具端](https://github.com/mohui666/lom_modkit/blob/v1.6.0/docs/chs/rust.md) 及各专题。检测器继续支持旧版媒体，当前输出不嵌入水印。1.6.0 顶层 `--help`／`--version` 直接输出，不打开窗口；查询旧版时使用 `--cli --version`。版本变化见 [GitHub Release](https://github.com/mohui666/lom_modkit/releases/tag/v1.6.0)，仓库不保留独立版本 notes。
