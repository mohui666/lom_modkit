# CLI 命令与参数

主入口 `lom-editor.exe --cli COMMAND ... --json`，不需要 Python。独立 `lomc` 提供下表的核心命令；操作类命令由编辑器提供。Python 调用方法见 [接口参考](python_api.md)。

所有 JSON 输出为 UTF-8，退出码：0 成功；1 参数／校验／执行失败；水印未检出为 2。命令失败输出 `{ok:false,errors:[...]}` 或 `{ok:false,issues:[...]}`。issues 带 severity/code/story_id/node_id/message。--json 可放命令前后。自动化不要解析默认中文文本输出。

## 核心命令

`PATH` 为输入路径；`-o` 和 `--output` 等价。下列命令均支持 --json。

| 命令 | 参数 | 输出／副作用 |
| --- | --- | --- |
| `--help` / `--version` | 无 | 帮助／版本 |
| `sdk` | `-o 新目录` 必填 | 导出配套 Python 包、两份参考文档、示例和 AI skill；拒绝覆盖现有文件 |
| `schema SECTION` | SECTION=nodes/catalog/forms/defaults/project；可选 -o JSON | 当前完整节点契约、目录或工程请求说明 |
| `project REQUEST.json` | -o 工程目录必填 | 读取请求并生成可编辑工程；返回 checks/issues |
| `project-read PATH` | 可选 -o JSON | `{ok,project:{source,manifest,stories,scenario},assets:[包内路径]}` |
| `export PATH` | -o .lommod必填 | 原生读取工程／包，重新编译并导出 |
| `pack DIRECTORY` | 可选 -o .lommod | 打包标准 story/ 目录工程；export 适合通用工程输入 |
| `inspect PACKAGE` | 无 | 完整性、源码／Lua 一致性和条目预览；不执行 Lua |
| `new-story NAME_OR_PATH` | 可选 --title 标题、-o JSON | 新建章节草稿 |
| `check STORY.json` | 可选 --locale chs/cht/ja/ko | 校验单章 |
| `build/compile STORY.json` | 可选 -o Lua、--locale | 生成 Lua；不带 -o 写同名 .lua |
| `texts STORY.json` | 可选 -o JSON | `{ok,texts:{本地化路径:原文}}` |
| `author REQUEST.json` | 可选 -o JSON | 原受控节点 API，返回 result/after；参数见创作指南 |
| `edit STORY.json` | --operations 操作数组JSON 必填，可选 -o JSON | 批量编辑；无 -o 时写回原文件，操作失败不写回 |
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

工程请求字段：source（可选旧工程）、mode（新建模式）、target（原版补丁目标）、manifest（顶层合并）、stories（完整替换章节集合）、scenario（完整替换，null移除）、assets（路径或内嵌JSON）、remove_assets（明确移除路径）。完整示例见 [创作指南](ai_cli.md#工程请求字段)。相对 source/资源路径相对请求文件；-o 相对命令工作目录。

## 编辑器操作命令

先执行 `lom-editor.exe --cli automation-schema --json` 查询当前实际契约。以下统一为 `COMMAND REQUEST.json --json`；字段名以 JSON 中的名称为准，不转换为同名短选项。相对路径按请求文件所在目录解析。

| 命令 | 必填字段 | 可选字段 | 行为 |
| --- | --- | --- | --- |
| `open-editor` | project | output、script、node | 读取工程／包／Lua，必要时另存目录，打开新窗口并定位，返回pid |
| `preview` | project、script、node | 无 | 返回离线最终预览状态；不打开界面 |
| `preview-adjust` | project、script、node、target、x、y、output | scale=100、effect_scale=[x,y] | 复用预览拖动规则，保存调整后的完整工程 |
| `prepare-playtest` | project、script、output | node=章节入口 | 生成隔离演出包，不写游戏、不推进任务 |
| `playtest` | project、script、game | node=入口、runtime=内置Host | 安装／更新试玩所需Host，安装隔离包并请求／启动游戏 |
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

`playtest/capture-effects/install-play-bundle` 有游戏副作用；普通 build/export/preview 不会自动执行这些命令。实际音画、随机事件、战斗与存档结果由作者实机验收。
