# Agent rules

## Astra 协作约定

- 以用户当前目标和本轮明确约束为准。任务要求实施时，完成实际修改与必要验证，不停在计划、建议或“是否继续”。普通实现选择自行决定；只有缺失信息会实质改变结果或操作超出授权时才询问，并先完成不依赖答案的工作。
- 用户指令优先于本地 skills 的工作流建议。只读取与当前任务直接相关的文件和技能；不因关键词命中就串联整套技能、生成流程工件或增加审批。
- 保留既有业务规则、数据所有权、用户改动和明确的工具限制。只改当前目标需要的内容，不顺手重构、升级依赖、搬目录或扩展产品范围。

## 拒绝过度防御性编程

- 直接使用已有输入、文件、依赖和运行环境，不重复做环境、权限、目录或文件存在性预检查。
- 不为假想故障添加重复参数验证、大量极端输入分支、宽泛 `try/catch`、默认值兜底、静默失败或伪造成功。契约不满足时暴露具体错误。
- 不主动新增重试、退避、熔断、降级、备用实现、兼容层、自动备份、回滚、迁移或恢复机制。
- 不主动添加 SHA、MD5、签名、文件哈希、完整性校验、CI/CD、发布门禁、安全扫描、许可证审计、复杂日志、监控、遥测或诊断框架。
- 不为未来需求预建插件系统、通用框架或抽象层，不为小改动铺设大量单元测试、回归测试、故障注入或性能基准。
- 只在缺少检查会立即阻止核心功能、造成明显数据损坏或掩盖真实错误时保留最小必要检查。现有鉴权、真实业务校验和数据保护功能继续遵守其契约；本规则不授权删除这些功能。
- 例外必须来自用户明确要求，或与本次改动直接相关的既有产品契约。旧文档中泛化的“每次全量检查”“必须先审批”“自动完善”不构成额外任务。

## 验证与交付

- 选择能证明本次行为的最小验证：文档或提示词改动检查内容和 diff；代码改动运行相关构建、现有定向测试或核心流程冒烟。低影响、可逆改动不新增仅复述实现的测试。
- 必要检查通过即交付；只有新改动、失败或具体未解决疑点才扩大或重复验证。不要为了收尾重跑无关全量测试、打包、实机流程或基准。
- 错误如实报告。区分实际运行通过、静态检查、未运行与真实环境验证；历史测试数量不能当作本次证据。
- 仅在任务需要时使用子代理；不强制委派、切换模型或修改推理档位，遵守当前会话设置与工具权限。
- 按当前授权和项目约定执行 Git 操作，只提交本任务文件；不要为清空工作区而夹带其他改动，不强推或丢弃用户内容。没有远端时报告，不擅自创建远端。
- 用简明中文交代实际修改、验证结果和已知问题。只有需求、接口或已验证事实改变时同步相关文档，不追加与交付无关的报告。

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
   [`docs/research/decompiled/`](docs/research/decompiled/) are an
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

## Rebuild showcase and the frozen editor

Do not leave samples or `lom_editor.exe` on an old schema. The frozen
editor embeds `lomc`; opening a new field (such as `enemy_health`) with
an old exe reports “未知字段”. Showcase3 is the acceptance pack: if
Combat/Battle/node fields change and the sample still uses the previous
character, stats, or roster, the fix was not finished.

Rebuild the affected sample and frozen editor when changing the authoring contract used by them:

- `compiler/lomc/` schema, validate, or codegen
- editor node forms, models, or story_api
- Host Combat/Battle patches
- authoring fields on `combat` / `battle` / other gameplay nodes

### Showcase 3.0

Update `samples/showcase3/build_showcase3.py` so the sample *uses* the
new fields (not just compiles). Then regenerate JSON, pack, and install:

```powershell
editor/.venv/Scripts/python samples/showcase3/build_showcase3.py
editor/.venv/Scripts/python editor/story_api.py pack samples/showcase3 `
  -o samples/全节点样例3.0.lommod --json
```

Install the packed `.lommod` into the game mods folder when the change
is meant to be playable. Delete leftover `samples/showcase3/story/*.lua`
if a local compile wrote them next to JSON.

### Frozen editor

```powershell
editor/.venv/Scripts/python editor/build_exe.py
```

Tell the user to quit the running editor and open
`editor/dist/lom_modkit/lom_editor.exe`. Do not point them at an older
zip or desktop shortcut. Rebuild the Windows zip only when they asked
for a package.
