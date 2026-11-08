# Python API 接口参考

适用于包含对象 API 的新构建；完整教程见 [对象创作指南](ai_cli.md)，命令参数见 [CLI 参数](cli_parameters.md)。Python 3.10+，只用标准库。SDK 必须从使用中的 exe 导出，不能复用已退役的旧 Python 编辑器模块。

```powershell
.\lom-editor.exe --cli sdk -o .\authoring --json
```

脚本在 authoring 目录运行，或把该目录加入 sys.path。`from lom_modkit import ...`。

## 节点与章节

| 接口 | 参数与返回 |
| --- | --- |
| `Say/Show/Choice/...(*, id=None, **fields)` | 所有 70 类节点均有构造函数，type 按下划线转首字母大写；必填字段、类型、枚举与 `schema nodes` 完全对应，未知字段报错 |
| `NODE_TYPES[type](...)` | 按原始 type 动态构造同一种对象 |
| `Node.from_dict(node)` | 从现有节点对象构造，执行同样的参数检查 |
| `node.to_dict()` | 返回独立 JSON 对象 |
| `node.replace(**fields)` | 返回重新检查过的新节点 |
| `node.action()` | 生成 Parallel 子动作，删除流程 id；不允许 goto 或不支持的动作类型 |
| `Option(*, text, goto)` | Choice 选项对象 |
| `Chapter(*, id, title, nodes, start=None, **metadata)` | 默认 start 为首节点 ID；检查节点 ID、重复 ID 与入口。localization、mood、_editor 等沿用章节字段 |

所有顶层节点必须有 id；Parallel 子动作不填 id。Python 关键字字段用尾下划线：`Move(from_="M", to="R", character="player")`。章节 ID 允许短横线，节点 ID 只用字母、数字、下划线。不要从 UI 百分比猜存储单位，查 schema 和格式文档。

```python
chapter = Chapter(id="main", title="开场", nodes=[
    Say(id="s", mode="narrative", text="夜雨停了。"),
    End(id="e", next_script="chapter2"),
])
```

## 工程与执行

```python
Project(*, id, name, chapters, author="作者", version="1.0.0", entry=None,
        mode="story", target=None, scenario=None, assets=None, manifest=None)
```

entry 默认首章；mode=story/addon/sandbox/vanilla_patch，最后一种需要 target。manifest 用于补充现有作品字段，显式参数优先。新作品由 Rust 生成独立 campaign_id。

| 接口 | 行为 |
| --- | --- |
| `Project.open(cli, path)` | 原生读取目录、JSON、Lua、MOD；保留身份、资源与原来源 |
| `project.stories` / `.manifest` / `.recipe` | 可修改的章节映射、作品配置、完整工程请求 |
| `project.write(path)` / `.to_dict()` | 写请求 JSON／返回请求对象；请求不是供编辑器打开的最终工程 |
| `CLI(executable)` | 接受 lom-editor.exe 或独立 lomc；自动为编辑器入口加 --cli |
| `cli.build(project, output)` | 体检、临时编译打包重开，通过后保存工程并再次重开；返回 checks 和 issues |
| `cli.export(project_path, output)` | 编译并导出 .lommod |
| `cli.run(command, *args)` | 调用命令，自动加 --json；返回 JSON 对象 |
| `cli.operation(command, **request)` | 调用编辑器扩展操作，路径转绝对值；见 CLI 参数文档 |
| `cli.open_editor(project, output=None, script=None, node=None)` | 可选导入为新工程，再打开真实界面并定位；返回 pid |

`Project.open` 的 recipe 通常依靠 source 保留资源，没有 assets 键也正常。删除章节、scenario 或资源后另存新目录。不要更换旧作品的 campaign_id。assets 可传路径映射，也可传下面的素材对象列表。

## 事件与任务

| 构造函数 | 关键字参数 |
| --- | --- |
| `Condition` | source、key、op、value；来源 time/stat/affinity/flag/game_flag/variable/task；task 用状态 ID 和 eq/ne，其他来源用 Int32 和 eq/ne/lt/le/gt/ge |
| `All(*条件)` / `Any(*条件)` / `Not(条件)` | 组合条件；空 All 为真，空 Any 为假 |
| `Task` | id、name、states、initial、transitions=() |
| `TaskTransition` | from_、to、when |
| `VariableChange` | op=set/add、Int32 value |
| `Outcome` | tasks={任务ID:状态}、variables={变量ID:VariableChange}、actions=[OutcomeAction] |
| `OutcomeAction` | type=stat/affinity/game_flag 时 key/op/value；flag 仅 key；talent 用 key/value=±1；item 用 key/kind/op/value；time 用 year/month/stage |
| `WeightModifier` | when、add |
| `Event` | id、name、script、trigger 必填；position/when/outcomes 可选；once=True、enabled=True、check=stage、priority=0、chance=100、weight=1；可选 pool/weight_modifiers |
| `Scenario` | events、tasks=()、entry_event=None |

每个事件 outcomes 必须有 completed；不提供 outcomes 时自动生成。sandbox 必须且只能有一个 start 事件，与 entry_event、manifest.entry 对应；开场启用、chance=100、无条件。addon 不设置开场事件。Scenario 检查任务／状态引用，工程跨章结果由 Rust 完整检查。

## 翻译、素材与路线测试

| 构造函数 | 参数 |
| --- | --- |
| `Translation` | locale、texts={本地化路径:译文} |
| `Localization` | default_locale=chs、fallback_locale=默认语言、translations=[Translation]；也接受语言到译文表的映射 |
| `ImageAsset` | id、name、main=本地图片 |
| `AudioAsset` | id、name、main、audio_kind=music/sound/env，可选 character |
| `CharacterAsset` | id、name、main、portraits={表情ID:本地图片}，以及 scale/extends/portrait_layouts 等 metadata |
| `CharacterAsset.from_folder` | id、name、folder、prefix=""，以及人物 metadata；只读当前目录，前缀筛选后生成独立表情 |
| `Asset` | type/id/name/main，额外 files={文件名:本地路径} 与类型字段；支持全部七种内容类型，包括 item/martial/support/skill |
| `ChoiceStep` | node、option（0 基下标） |
| `RouteTest` | name、story，choices=()、reaches_node=()、reaches_ending=True、flags/variables/initial_flags/initial_variables |

素材对象的 `.ref` 是 user:namespace.name，供节点引用；`Project(assets=[...])` 自动生成内嵌 content.json 与资源路径映射。高级 metadata 及嵌套玩法字段沿用 [用户内容](user_content.md)，Rust 负责语义校验。翻译路径用 `cli.run("texts", chapter_path)` 查询；默认语言不重复放入 translations。

```python
chapter = Chapter(..., _editor={"tests":[
    RouteTest(name="问路", story="main", choices=[ChoiceStep(node="choose", option=0)],
              reaches_node=["finish"], flags={"asked":True})
]})
```

同一 choice 的多条动作按顺序消费。原始测试配置使用 assert，未知 expect 或拼错字段会被拒绝。战斗、骰子和 scenario 调度不由离线路线引擎执行；unsupported 不是通过。

## 报错与验证边界

```python
from lom_modkit import CLIError
try:
    cli.build(project, output)
except CLIError as error:
    for issue in error.result.get("issues", []):
        print(issue["story_id"], issue["node_id"], issue["message"])
    for message in error.result.get("errors", []):
        print(message)
    raise
```

构造参数问题抛 TypeError/ValueError，文件缺失抛 FileNotFoundError；Rust 命令失败抛 CLIError，含 result 和 returncode。不要捕获后仍声称成功。build 不自动执行游戏；open_editor 会开窗口，playtest/capture-effects 会执行明确的游戏操作。

复杂可运行示例和一键重跑方法见 [创作指南](ai_cli.md#复杂样例与重跑)。
