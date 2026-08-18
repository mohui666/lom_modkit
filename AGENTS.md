# Agent rules

This file is binding for anyone (human or agent) changing this repository.

## Game features require decompiled APIs

If the work touches *game functionality* — Combat, Battle, saves, shops,
stats, flags, talents, items, scenes, UI panels, characters, Addressables,
or any other official *Legend of Mortal* type — **read the decompiled
interface first**. Do not invent methods, fields, scene keys, or result
codes from memory or from old comments.

Required sources, in order:

1. [`research/gameplay_api.md`](research/gameplay_api.md) — confirmed
   signatures and what they may be used for.
2. [`research/gameplay_api_contract.json`](research/gameplay_api_contract.json)
   — assembly hashes and the exact fragments that must still match.
3. Live decompile of the installed game (do not reuse stale snippets):

   ```powershell
   ilspycmd -t Mortal.Battle.ReadyPanel `
     "C:\Program Files (x86)\Steam\steamapps\common\LegendOfMortal\Mortal_Data\Managed\Mortal.Battle.dll"
   ```

   Prefer the game’s `Mortal_Data/Managed/*.dll`. Frozen copies under
   [`docs/research/decompiled/`](docs/research/decompiled/) are an
   archive only; they do not include `Mortal.Battle` / `Mortal.Combat`.
4. After a game update, run `python tools/verify_gameplay_api.py --json`
   before keeping old conclusions.

Do **not** commit or upload full decompiled game source
(`docs/research/decompiled/**`, `ilspycmd -p` dumps, extracted assets).
That is the game's code. It is already gitignored. What *may* live in
the repo is our own notes: method names we hook, assembly hashes, and
the workflow in `docs/chs/decompiled_api.md`.

How to use a decompiled type:

- Quote the type and method you actually called (`ReadyPanel.Setup`,
  `CharacterHealth.MaxHealth`, `NpcSpawner.InitNpcList`, …).
- Patch or wrap that entry. Do not mutate official ScriptableObject
  assets; clone per instance when a value must change.
- If the type is missing, the field is private with no safe hook, or the
  hash no longer matches: stop and record it. Do not guess a nearby API.

Docs map: [`docs/README.md`](docs/README.md). The decompile workflow is
[`docs/chs/decompiled_api.md`](docs/chs/decompiled_api.md). The v3
package contract is [`docs/chs/mod_format.md`](docs/chs/mod_format.md).

## Rebuild showcase and the frozen editor

Do not leave samples or `lom_editor.exe` on an old schema. The frozen
editor embeds `lomc`; opening a new field (such as `enemy_health`) with
an old exe reports “未知字段”. Showcase3 is the acceptance pack: if
Combat/Battle/node fields change and the sample still uses the previous
character, stats, or roster, the fix was not finished.

Rebuild **both** in the same turn when any of these change:

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
