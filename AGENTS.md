# Agent rules

This file is binding for anyone (human or agent) changing this repository.

## Game features require decompiled APIs

If the work touches *game functionality* — Combat, Battle, saves, shops,
stats, flags, talents, items, scenes, UI panels, characters, Addressables,
or any other official *Legend of Mortal* type — **read the decompiled
interface first**. Do not invent methods, fields, scene keys, or result
codes from memory or from old comments.

Use the relevant interface evidence below; only inspect the type needed for the task:

1. [`research/gameplay_api.md`](research/gameplay_api.md) — confirmed
   signatures and what they may be used for.
2. [`research/gameplay_api_contract.json`](research/gameplay_api_contract.json)
   — confirmed interface fragments; do not calculate assembly hashes.
3. Live decompile of the installed game (do not reuse stale snippets):

   ```powershell
   ilspycmd -t Mortal.Battle.ReadyPanel `
     "C:\Program Files (x86)\Steam\steamapps\common\LegendOfMortal\Mortal_Data\Managed\Mortal.Battle.dll"
   ```

   Prefer the game’s `Mortal_Data/Managed/*.dll`. Frozen copies under
   `docs/research/decompiled/` are a local, ignored
   archive only; they do not include `Mortal.Battle` / `Mortal.Combat`.
4. After a game update, inspect only the affected live type and method before reusing old API conclusions; do not run hash-based verification.

Do **not** commit or upload full decompiled game source
(`docs/research/decompiled/**`, `ilspycmd -p` dumps, extracted assets).
That is the game's code. It is already gitignored. What *may* live in
the repo is our own notes: method names we hook, confirmed signatures, and
the workflow in `docs/chs/decompiled_api.md`.

How to use a decompiled type:

- Quote the type and method you actually called (`ReadyPanel.Setup`,
  `CharacterHealth.MaxHealth`, `NpcSpawner.InitNpcList`, …).
- Patch or wrap that entry. Do not mutate official ScriptableObject
  assets; clone per instance when a value must change.
- If the affected type or safe hook is unavailable, report that concrete API limitation. Do not guess a nearby API.

Docs map: [`docs/README.md`](docs/README.md). The decompile workflow is
[`docs/chs/decompiled_api.md`](docs/chs/decompiled_api.md). The v3
package contract is [`docs/chs/mod_format.md`](docs/chs/mod_format.md).

## Preserve compatibility with released projects and packages

Projects and `.lommod` files saved by supported released editor versions must
remain openable in newer editors. A validation or code-generation fix must not
make an otherwise valid old file unreadable or require its author to recreate
the project or install an older editor to export it again.

- When generated Lua changes, update the existing Story/Lua verification to
  recognize the exact historical output of supported released compilers. Do not
  compare only with the newest emitter and report a normal old package as
  tampered. Keep archive safety, integrity, and source/Lua ownership checks;
  never accept arbitrary Lua merely because it has regenerated hashes.
- Opening a recognized old file must preserve its source, nodes, assets, and
  original file. Re-export uses the current corrected generator. Newly added
  authoring checks belong in editing/playtest/export diagnostics; old values
  that need correction must remain accessible for editing, with an actionable
  explanation and chapter/step location.
- Changes to validation, defaults, code generation, package reading, or project
  loading require a targeted old-version open → edit → save/export → reopen
  regression. Cover the affected historical output and ensure unrelated Lua
  substitution is still rejected. When the issue is version-specific, also
  reproduce with a file produced by that released compiler or an established
  fixture captured from it; current-compiler round trips alone are insufficient.
- Incompatible schema changes must be explicit and documented. Diagnose the
  affected file, chapter/step, and concrete reason instead of only “打开失败”.

## Rebuild showcase and the native editor

When changing authoring fields, node defaults, validation, code generation, forms,
or Host Combat/Battle contracts, update the affected sample and rebuild the native
Rust editor. Keep C# Host integration and v3 package compatibility.

Showcase source is `samples/showcase3/source.json`. It must use the new fields,
not merely compile them. Generate and validate all 64 node types with:

```sh
cargo run --locked -p lomc --example build_showcase3 -- out/showcase3-native
cargo run --locked -p lomc -- inspect out/showcase3-native/showcase3.lommod --json
```

To refresh the tracked JSON, omit the output argument. Do not commit generated
Lua beside JSON or official game assets. Install a sample only when the user's
scope includes game installation/testing.

Build the Mac editor with `scripts/build-macos.sh`; open `out/LoM Modkit Rust.app`.
Windows build entry: `scripts/build-windows.ps1 -NoArchive`, using prebuilt Host
DLLs; output is `out/windows/lom_modkit/lom-editor.exe`. Only build a release ZIP
when asked. User restrictions on Windows/game testing take precedence.

## Release 文本格式与发布后清理

每次发布统一更新仓库 `RELEASE_NOTES_v<版本>.md`，GitHub Release 正文使用该文件；标题固定为 `<版本>：<一句话概括>`。正文按下列结构编写：

```markdown
# <版本> 更新内容

<一段简短概述>

## 更新事项

1. **<更新名称>**：<用户可见的变化及适用范围>。
2. **<更新名称>**：<用户可见的变化及适用范围>。

## 升级说明

<下载附件、升级操作及必要的 Host／格式兼容说明>

## 验证与已知限制

<实际执行的验证及未覆盖范围；性能数据注明测试条件>
```

更新事项使用一份连续的 `1. 2. 3.` 有序列表，不按分类重新编号，不改用无序项目符号；没有实际内容的栏目省略。区分已验证结果与未运行项目，不能把某个样例的内存、耗时或 FPS 写成所有机器和工程的保证。Windows 正式附件继续只提供内置组件的 `lom-editor.exe`。

发布成功并核验远端附件的大小与摘要后，**默认清理本次可再生成的构建产物，无需再次询问**：Cargo `target/`、本次编译生成的 `bin/obj`、临时打包和验证目录、重复的暂存 exe／组件包。保留最终发布 exe、版本说明、必要验证记录，以及仍需用于重现问题的独有输入。

清理前列明实际路径，确认其位于工作区内且属于本次构建，确认没有运行任务或仍需的依赖引用；Windows 使用原生 PowerShell 的绝对 `-LiteralPath`，递归删除前核对路径，不使用 `-Force`。遇到符号链接或目录联接，只处理链接本身，不能递归进入目标。保留工程、素材库、游戏存档、用户改动，以及共享 Cargo 下载缓存、Rust 工具链和仍需复用的第三方依赖；不整体删除混有历史数据的 `out/`，也不为清理而新增备份副本。
