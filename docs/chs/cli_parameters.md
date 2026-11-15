# CLI 命令与参数

本文对应 1.6.0。主入口 `lom-editor.exe --cli COMMAND ... --json`，不需要 Python；Windows 正式附件只有内置组件的 exe。独立 `lomc` 提供下表的核心命令；操作类命令由编辑器提供。Python 调用方法见 [接口参考](python_api.md)。SDK 根目录的 `README.md` 对应仓库创作指南，两份参考文件与它同目录。

所有 JSON 输出为 UTF-8，退出码：0 成功；1 参数／校验／执行失败；水印未检出为 2。命令失败输出 `{ok:false,errors:[...]}` 或 `{ok:false,issues:[...]}`。issues 带 severity/code/story_id/node_id/message。--json 可放命令前后。自动化不要解析默认中文文本输出。

## 核心命令

`PATH` 为输入路径；`-o` 和 `--output` 等价。下列命令均支持 --json。

| 命令 | 参数 | 输出／副作用 |
| --- | --- | --- |
| `--help` / `--version` | 无 | 帮助／版本 |
| `sdk` | `-o 新目录` 必填 | 导出配套 Python 包、两份参考文档、示例和 AI skill；拒绝覆盖现有文件 |
| `schema SECTION` | SECTION=nodes/catalog/forms/defaults/project；可选 -o JSON、--summary；catalog 可加 --filter 关键词 | 当前节点契约、目录或工程请求说明；筛选保留完整命中条目 |
| `project REQUEST.json` | -o 工程目录必填 | 读取请求并生成可编辑工程；返回 checks/issues |
| `project-read PATH` | 可选 -o JSON、--summary | `{ok,project:{source,manifest,stories,scenario},assets:[包内路径]}` |
| `export PATH` | -o .lommod必填 | 原生读取工程／包，重新编译并导出 |
| `pack DIRECTORY` | 可选 -o .lommod | 打包标准 story/ 目录工程；export 适合通用工程输入 |
| `inspect PACKAGE` | 无 | 完整性、源码／Lua 一致性和条目预览；不执行 Lua |
| `new-story NAME_OR_PATH` | 可选 --title 标题、-o JSON | 新建章节草稿 |
| `check STORY.json` | 可选 --locale chs/cht/ja/ko | 校验单章 |
| `build/compile STORY.json` | 可选 -o Lua、--locale | 生成 Lua；不带 -o 写同名 .lua |
| `texts STORY.json` | 可选 -o JSON、--summary | `{ok,texts:{本地化路径:原文}}` |
| `author REQUEST.json` | 可选 -o JSON、--summary | 原受控节点 API，返回 result/after；参数见创作指南 |
| `edit STORY.json` | --operations 操作数组JSON 必填，可选 -o JSON、--summary | 批量编辑；无 -o 时写回原文件，操作失败不写回 |
| `analyze PATH` | 无 | 流程、符号、不可达节点等分析 |
| `test PATH` | 无 | 执行章节 `_editor.tests`；任何 fail/unsupported 导致失败退出 |
| `statistics PATH` | 无 | 作品、素材和配音统计 |
| `preflight PATH` | --profile editing/release，默认editing | 工程体检，不修改内容 |
| `release PATH` | -o 输出目录必填 | 本地发布文件；不上传或创建远端版本 |
| `migrate JSON` | --kind story/manifest/content，默认story | 已有迁移入口；可能修改原文件并保留其旧字节 |
| `content-inspect/content-import CONTENT` | 可选 --library 内容库目录 | 检查／导入 .lomcontent；import 写入明确的素材库 |
| `play-bundle PATH` | -o ZIP、--runtime-dir、--loader-x86 ZIP、--loader-x64 ZIP | 生成完整游玩包，参数均需提供 |
| `install-play-bundle DIRECTORY` | --game 游戏目录；省略可定位或询问 | 安装已解压游玩包，会写入游戏目录 |
| `detect-watermark IMAGE` | 无 | 图片来源水印检测 |
| `detect-watermark-video VIDEO` | --ffmpeg 路径、--interval 秒默认2、--max-frames 默认12 | 视频检测，需可用 FFmpeg |

工程请求字段：source（可选旧工程）、mode（新建模式）、target（原版补丁目标）、manifest（顶层合并）、stories（完整替换章节集合）、scenario（完整替换，null移除）、assets（路径或内嵌JSON）、remove_assets（明确移除路径）。完整示例见 [创作指南](https://github.com/mohui666/lom_modkit/blob/v1.6.0/docs/chs/ai_cli.md#工程请求字段)，SDK 中为本地 `README.md`。相对 source/资源路径相对请求文件；-o 相对命令工作目录。

`schema catalog --filter 关键词` 不区分大小写，匹配条目的 ID、名称和其他文本／数字，以及映射目录的条目键；不按目录分类名称筛选。命中时保留整条记录和 schema 编号，无匹配的分类保留空数组／对象。省略 --filter 仍返回完整目录。

`schema/project-read/texts/author/edit` 指定 `-o` 后可加 `--summary`：文件仍写入完整结果，标准输出改为包含 ok、command、output、counts 的简短摘要。schema 摘要另含 section；project-read 计数包含章节、节点和资源数。`--summary` 未指定 -o 时会报错；错误仍返回完整诊断。省略该选项保持旧输出。

```powershell
.\lom-editor.exe --cli schema catalog --filter "雨" -o .\rain-catalog.json --summary --json
.\lom-editor.exe --cli schema nodes -o .\nodes.json --summary --json
.\lom-editor.exe --cli project-read .\作品 -o .\project.json --summary --json
```

## 编辑器操作命令

先执行 `lom-editor.exe --cli automation-schema --json` 查询当前实际契约。以下统一为 `COMMAND REQUEST.json --json`；字段名以 JSON 中的名称为准，不转换为同名短选项。相对路径按请求文件所在目录解析。

| 命令 | 必填字段 | 可选字段 | 行为 |
| --- | --- | --- | --- |
| `open-editor` | project | output、script、node | 读取工程／包／Lua，必要时另存目录，打开新窗口并定位，返回pid |
| `asset-query` | 无 | project、library、game、kind、query、id、portrait、position、limit、offset、thumbnail_size、cache_dir | 按关键词查询素材，返回图片／缩略图、画布、透明边界和有出处的锚点／裁切信息 |
| `preview` | project、script、node | library、game | 返回离线最终预览状态及模拟／渲染／素材能力说明；不生成图片 |
| `render-preview` | project、script、node、output | library、game、width=1280、height=720、time | 直接输出新的纯舞台 PNG，复用编辑器绘制，不打开编辑器或游戏 |
| `render-previews` | project、frames、output | library、game、width=1280、height=720 | 一个隐藏图形会话批量输出舞台 PNG、对照图和帧清单 |
| `render-video` | project、game、output | script、node、scope、width/height、fps、auto_speed、choices、choice_hold、background、runtime、overwrite | 复用游戏原生画面与声音生成 MP4；默认后台，等待成片完成，stderr 输出进度 JSON，stdout 返回最终结果 |
| `preview-adjust` | project、script、node、target、x、y、output | scale=100、effect_scale=[x,y] | 复用预览拖动规则，保存调整后的完整工程 |
| `prepare-playtest` | project、script、output | node=章节入口 | 生成隔离演出包，不写游戏、不推进任务 |
| `prepare-event-playtest` | project、event_id、output | entry_mode、source_script、year、month、stage、values、include_official_events | 生成保留条件和调度规则的隔离事件包，不写游戏；准备完成不代表执行了原版前置 |
| `playtest-event` | project、event_id、game | 同上，另有 runtime=内置Host | 从指定原版前置章实际播放并沿真实调度进入事件，或显式测试入口边界；返回可查询的 request_id |
| `playtest` | project、script、game | node=入口、runtime=内置Host | 安装／更新试玩所需Host，返回 request_id 及确认阶段；需要重启时不发送给旧 Host |
| `playtest-status` | game、request_id | 无 | 查询一次状态、来源证据及游戏进程是否仍运行，不启动游戏或持续轮询 |
| `extract-assets` | game、output | 无 | 只读游戏；在output父目录内创建新预览库，返回 output_dir |
| `extract-story` | game | target | 原版章节清单或指定章节源码／译文 |
| `extract-routes` | game | 无 | 原版路线索引 |
| `extract-audio` | game、kind、name、output | 无 | kind=music/sound/env，输出新的 .wav 文件 |
| `capture-effects` | game | runtime=内置Host、names=[特效名] | 启动游戏采样；不传names沿用现有完整捕获流程 |

```json
{"project":"作品.lommod","output":"导入工程","script":"crossing","node":"shock_and_mood"}
```

```powershell
.\lom-editor.exe --cli open-editor open.json --json
```

output 省略时直接打开原输入，不自动保存。打开目标不存在、读取失败或指定章节／节点无效时不启动窗口。`open-editor` 成功表示已创建界面进程，不能替代对窗口内容的后续检查。

位置调整 target：character/mood 用 `{kind,id:人物ID}`；overlay 用 `{kind,id:插图slot}`；cg 用 `{kind:"cg"}`；effect 用 `{kind:"effect",node:特效源节点ID,action:组内0基下标}`，非组内省略action。x/y/scale 与编辑器存储单位相同，气泡和人物偏移为舞台单位，图片位置为百分比。effect_scale 是有符号的两轴缩放。

```json
{"project":"作品","script":"crossing","node":"shock_and_mood",
 "target":{"kind":"mood","id":"user:mist.shen"},"x":36,"y":24,"output":"调整版"}
```

`playtest/playtest-event/render-video/capture-effects/install-play-bundle` 有游戏副作用；普通 build/export/preview 不会自动执行这些命令。实际音画、随机事件、战斗与存档结果由作者实机验收。

### 游戏原生视频

```json
{"project":"作品","game":"C:/Games/LegendOfMortal","output":"成片.mp4",
 "scope":"project","auto_speed":"game","choices":{"main/route":1},"choice_hold":1.5}
```

```powershell
.\lom-editor.exe --cli render-video video.json --json 2> video-progress.jsonl
```

参数和编辑器「生成视频」共用后端。默认 scope=chapter，script 为作品入口章节；scope=project 从作品入口播放并沿章节连线继续，不能另填 script/node。scope=step 必须指定 script 和 node，从该步骤开始并继续沿连线播放；node 不用于 chapter 范围。默认 1920×1080、30 FPS；width/height 须同时指定，只支持 1280×720 或 1920×1080，fps 为30或60。auto_speed 为 game/slow/standard/fast，game 沿用游戏自动播放速度。choices 为 `章节ID/选项步骤ID → 从0开始的选项序号`，未指定的选项默认选首个可用项；choice_hold 为选项停留秒数，范围0～30、默认1.5。

output 为 `.mp4`，父目录须已存在；默认拒绝覆盖，明确填 overwrite=true 才允许成片完成后原子替换。失败不改写已有文件。路径相对请求 JSON 所在目录，runtime 省略时用内置 Host。

background 默认 true：在独立桌面中启动游戏原生渲染进程，不弹出窗口、不切换当前游戏，任务结束后自动退出；使用任务内的临时 Host、演出包和独立存档目录。后台模式只接受当前编辑器配套 Host，复用游戏文件与 BepInEx 核心，不复制整套游戏。仍消耗 CPU、GPU 与编码资源，运行时保持命令行进程即可。显式 background=false 使用原有前台游戏流程，需保持游戏运行，请勿最小化或调整游戏窗口。

Windows 正式 exe 内置专用于原生导出的精简 FFmpeg，保留 H.264、AAC、MP4、原始画面输入、PCM 混音与转换，以及汇编优化和线程；旧媒体检测命令 `detect-watermark-video` 使用 `--ffmpeg` 指定或 PATH 中的外部程序。画面读取支持时使用异步 GPU 回读，再由有界帧池按序交给编码线程；慢编码会限制上游，不丢帧或无限增加缓存。原生 Wwise 与作品自定义音轨按同一帧时钟生成并合成，不能以最终文件存在推断音轨有效。当前普通试玩、前台和后台视频均不嵌入可见或隐藏水印；旧媒体检测器和 Host 身份／存档校验仍保留。

剧情选项由预设自动处理，战斗、商店及交互面板等限制与界面入口一致。仅固定的临时演出包参与渲染，原工程不改写；原版前置流程衔接仍用 playtest-event 单独检查。

进度以 `{"event":"video_progress","progress":...}` 的 JSON 行写到 stderr，包含任务编号、状态、帧数和时长等阶段信息。stdout 只返回最终 JSON；成功含 output、job_id、frames、seconds、width/height、fps、background 和 renderer=native-game，成片落盘后才成功。visual_review_performed=false 表示尚未替作者观看验收。

### 纯舞台图与关键帧对照

```json
{"project":"作品","library":"本机预览库","output":"关键帧",
 "frames":[{"script":"main","node":"greeting"},{"script":"main","node":"shock","time":0.4}]}
```

用 `render-previews` 执行以上请求。输出目录须为空；按请求顺序生成 `0001.png` 等图片，`contact-sheet.png` 按行从左至右排列，`frames.json` 对应章节／步骤、时间、缺失图片与特效。单张 `render-preview` 的 output 为尚不存在的 `.png`。尺寸为 16:9，最小 320×180，单边不超过4096；一次最多200帧。批量共用素材缓存，不逐帧启动编辑器。time 为该步骤动画的秒数；省略时采用静态终态，循环气泡／特效从首帧显示。依赖图形后端可用的本机环境。

这里输出的是**编辑器舞台预览**，不是游戏原生画面。`rendering_performed=true` 表示实际生成了图片，`visual_review_performed=false` 表示没有替作者人工看图；`complete=false` 会列出本帧缺失的图片／特效，不能当作完整画面验收。`preview` 和 `--smoke-preview` 明确返回 rendering_performed=false、rendered_images=0；保留的 preview_states 仅是模拟状态数量。

导出的 width/height 直接作为本次舞台投影分辨率，不继承界面保存的预览尺寸。界面预览分辨率偏好不增加剧情节点字段，也不改变原工程内容；纯舞台图仍不模拟真实前置章节遗留的 UI。

### 素材与试玩结果含义

`asset-query` 的 kind 为 all/character/view/picture；query 按名字、编号、表情和分类匹配，id／portrait 为精确筛选。默认每页20项，最多100项；缩略图默认256像素，最多1024。canvas 是实际图片画布，visible_bounds 是 alpha>0 的矩形（左上角原点）；原 Sprite 几何信息缺失时返回 null 并说明出处。stage_transform_anchor 是舞台变换锚点，**不代表可见脚底**。半身／全身无明确标注时返回 unknown，必须实际看图确认。旧预览库仍可查询；格式旧、图片缺失和无法判断是否最新会分别报告。可选 game 仅比较提取记录中的素材目录大小和修改时间，没有充分证据时 freshness=unknown。

试玩状态依次可见 sent（已发布）、game_started（进程或Host已观察）、chapter_entered（实际脚本启动）、node_entered（目标节点实际执行）；restart_required 需要退出游戏后重发，failed 包含原因和位置。evidence 区分编辑器发布、进程检查、Host收件、Lua启动与节点追踪，不能把 sent 当作已进入剧情。playtest-status 的 ok 表示查询成功，执行结果看 state；last_confirmed_state 和 updated_at 为最后观察，不是持续直播。游戏退出后会保留历史确认并返回 game_running=false／historical_observation=true。所有状态均不代表人工画面验收。

### 原版前置章节与事件衔接试玩

`Event(..., trigger="story_entry", target="section_01_free_03", from_="section_01_free_start_02")` 表示在真实原版跳转边界调度事件。`from_` 是 Python 参数名，包内字段为 `from`；省略时不限来源。单章 `preview`／`render-preview` 不执行上段原版，其 `assessment.original_context.inherited_ui_simulated=false`，因此不能发现原版留下的菜单、插图或其他界面。

```json
{"project":"作品","event_id":"visit","game":"C:/Games/LegendOfMortal",
 "entry_mode":"predecessor","source_script":"section_01_free_start_02",
 "year":3,"month":5,"stage":1,"values":[],"include_official_events":true}
```

```powershell
.\lom-editor.exe --cli playtest-event event-run.json --json
```

`story_entry` 默认 `entry_mode=predecessor`：`source_script` 省略时用事件的 `from`，两者都没有时要求明确指定原版起点。独立测试状态建立后实际播放该章，原版选项与后续跳转正常执行，不强行送到 target，也不在入口清除原版遗留界面。需要沿原版选项走到目标；这只覆盖所选起点之后的流程，不重建更早的完整存档历史。

`entry_mode=boundary` 保留入口直达检查：设置来源后直接进入 target，不执行前置章，不能作为遗留界面的验收。position／auto 事件默认 boundary，建立状态后仍需真实地点点击／自动调度；它们不支持 predecessor。

时间默认 1 年 5 月上旬；`values` 默认空列表，可显式填 `{source,key,value}`，source 支持 stat/affinity/game_flag/flag/variable/task。不会从条件中自动猜值，也不会改写条件、概率、优先级或竞争事件；前置章对时间和旗标的修改按真实结果保留。predecessor 默认启用原版事件，boundary 默认关闭，可用 include_official_events 明确指定。离线准备使用 `prepare-event-playtest`，将请求的 game 换成 output（新的 `.lommod` 路径），只生成测试包与范围说明。

事件状态额外区分 predecessor_entered（实际原版起点进入）、original_target_entered（原版目标正文已进入）和 event_not_triggered（到了目标但所测事件未进入）。event_entered 表示匹配所测事件 ID 后其 Lua 开始，event_node_entered 表示目标节点收到实际追踪；准备章不会充当事件成功。source_script、event_script/event_node、current_script/current_node 及 context_mode 标明请求与观察的不同位置。仍需实际检查“原版→MOD→原版”的连续画面，状态不保证残留界面已处理。

`event_selected` 只表示目标事件在真实调度中获选，`event_dispatch` 是该观察的 evidence；只有匹配所测事件 ID 后启动对应 Lua，`event_entered` 和事件请求的 `game_execution_confirmed` 才为 true。它们仍不代表已执行目标节点，节点确认单看 `event_node_entered`。若同一脚本被另一个事件选中，不能认作所测事件成功；到达原版 target 而事件未进入时保留 event_not_triggered。

新的跟踪事件请求采用 `format=2`，使旧 Host 拒绝执行，防止忽略前置模式而静默变成入口直达。新版 Host 仍接受旧 `format=1` 的 F5、旧事件测试和视频请求；`.lommod` 格式不变。正在运行的游戏需要退出后加载配套新版 Host。

1.6.0 顶层 `lom-editor --help`／`--version` 直接输出文字，不准备组件、不打开窗口。查询不支持该顶层入口的旧版时使用 `lom-editor --cli --version`，避免误开窗口。版本变化以对应 [GitHub Release](https://github.com/mohui666/lom_modkit/releases/tag/v1.6.0) 为准，仓库不保留版本 notes。
