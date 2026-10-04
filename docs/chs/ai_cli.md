# Rust 受控编辑 API 与 CLI

编辑器、脚本自动化和编译器共用 `lom-core`。旧 `import story_api` 已退役；自动化通过 Rust API 或 JSON 请求调用 `lomc`，不依赖 Python/Qt。字段定义见 `rust/lom-editor/data/authoring.json`，63 种节点默认值由 `lom-core` 内嵌。

## 构建及基础命令

```sh
cargo build --locked --release -p lomc
target/release/lomc new-story chapter2 --title 第二章 -o out/chapter2.json --json
target/release/lomc check samples/showcase3/story/main.json --json
target/release/lomc compile samples/showcase3/story/main.json -o out/main.lua --json
target/release/lomc pack samples/showcase3 -o out/showcase3.lommod --json
target/release/lomc inspect out/showcase3.lommod --json
```

Windows 二进制为 `lomc.exe`。所有路径均为普通参数；`--json` 可在命令前后使用。成功退出 0，校验、参数或 IO 失败退出 1；JSON 输出为 UTF-8。默认文本输出不是旧 argparse 输出协议，请自动化使用 `--json`。

## 受控写入

`author request.json --json` 接受 `{ "op": "操作名", "params": {...} }`，返回 `{ "ok": true, "result": ..., "after": ... }`。输入文件不被改写；`-o` 保存整个返回结果。

- `new_story`：`story_id`、`title`、可选 `mood`；返回旧 API 的 show + 空 say 草稿，需要补内容和结束节点后再校验。
- `new_node`：`node_type`、`node_id`；生成默认节点。
- `get_node`、`list_nodes`：传 `story`，前者另传 `node_id`。
- `add_node`：`story`、`node_type`、`fields`、可选 `after`（节点 ID）。未知字段或错误类型被拒绝。
- `update_node`：`story`、`node_id`、`fields`。
- `delete_node`、`set_start`：`story`、`node_id`。
- `rename_node`：另传 `new_id`；仅重写结构化跳转，不改正文。
- `move_node`：另传 `delta`，仅允许 `-1` 或 `1`。
- `add_say`：`text`、可选 `character`、`mode`、`portrait`、`after`；人物对白缺少登场时自动补 show。
- `add_scene`：`view`；`add_choice`：`options`（2～4 个 `[text, goto]` 二元数组）。
- `add_dice`：`maximum`、`header`、`bands`、`bonus`，可选 `bonus_name`、`bonus_status`、`after`。
- `add_death`：`death_id`、`text`，可选 `title`、`next`（仅 `Title`）、`after`。

最后四类也必须传 `story`。节点扩展字段可通过 `add_node` 的 `fields` 提交。完整参数及成功/拒绝对照见 `rust/lom-core/tests/fixtures/authoring_golden.json`。

批量编辑用 `edit story.json --operations operations.json [-o output.json] --json`。操作文件是数组，每条操作省略 `story`，共享上一步结果：

```json
[
  {"op":"add_node","node_type":"say","fields":{"mode":"narrative","text":"新的段落"},"after":"say1"},
  {"op":"update_node","node_id":"say1","fields":{"text":"修改后的原文"}}
]
```

任一步失败不写回；成功使用原子写入。该 API 允许编辑未完成草稿；提交给编译器前必须执行 `check`。`edit` 不接受 `new_story` / `new_node`（它们不修改现有剧情），应通过 `author` 调用。直接构造 Lua 会绕开校验，请使用受控操作和编译命令。

## 扩展命令

```sh
lomc analyze PROJECT --json
lomc test PROJECT --json
lomc statistics PROJECT --json
lomc preflight PROJECT --profile editing --json
lomc preflight PROJECT --profile release --json
lomc release PROJECT -o OUTPUT_DIR --json
lomc migrate story.json --kind story --json
lomc content-inspect content.lomcontent --library LIBRARY --json
lomc content-import content.lomcontent --library LIBRARY --json
lomc detect-watermark image.png --json
lomc detect-watermark-video video.mp4 --ffmpeg /path/to/ffmpeg --json
```

`test` 从 `_editor.tests` 读取声明；`unsupported` 不等同于通过。`release` 仅生成本地发布文件。FFmpeg 是视频检测的独立可选工具，不需要 Python。游戏安装/启动入口只在 Windows 编辑器开放，本次验证不包含 Windows 或游戏实机。
