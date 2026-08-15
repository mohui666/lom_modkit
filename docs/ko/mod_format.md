# 활협전 Mod 패키지 형식(v3 규약)

> 언어: [简体中文](../zh_CN/mod_format.md) · [繁體中文](../zh_TW/mod_format.md) · [日本語](../ja/mod_format.md) · 한국어(본문)

**모든 구성 요소(에디터 / 컴파일러 / 런타임 플러그인)는 이 문서를 기준으로 합니다.** 변경 시 이 문서도 함께 갱신해야 합니다.
문서 내 규칙의 공식 스크립트/디컴파일 실증 자료는 `../research/`에 있으며, 본문에서는 다시 다루지 않습니다.

## 1. 패키지 구조

`.lommod` 파일 = zip 압축 패키지, 내부 구조:

```
manifest.json          # 必填，包元信息
story/<id>.json        # 必填≥1，剧情源文件（编辑器可编辑的源格式）
lua/<id>.lua           # 必填≥1，编译产物（运行时只读这里）；每个 story/<id>.json 对应一个
texts.json             # 必填，已读文本表：{MOD_<modid>_<scriptid>_<nodeid>: 文本}（say 节点文本）
package-content.sha256 # 필수, 압축과 무관한 논리 콘텐츠 SHA-256
assets/                # 可选，自定义资源
                       #   图片：结局插图 / 人物介绍图 PNG/JPG
                       #   用户音频：assets/user/audio/<content_id>/
```

- `<id>` 규칙: `[a-zA-Z0-9_\-]+`, 패키지 내에서 고유, 즉 "스토리 스크립트 id"입니다.
-보내기(패키징) 시 반드시 다시 컴파일합니다: story/*.json → lua/*.lua, 둘의 이름이 같습니다.
- 런타임 플러그인은 **manifest.json, lua/ 디렉터리와 assets/만 읽습니다**; story/*.json은 에디터가 다시 읽어 편집하는 용도입니다. 컴파일러는 스토리가 명시적으로 참조하는 PNG/JPG(장당 ≤8MB)와 명시적으로 참조하는 `user:` 오디오만 패키지에 넣습니다.보낸 `.lommod`는 자체 완결적이며, 플레이어 컴퓨터에 에디터 저장소가 필요하지 않습니다.
- texts.json은 패키징 시 자동 생성됩니다: 각 story의 모든 **say** 노드 텍스트를 수집하며, key는 lua의 `GetStoryText` key와 일대일로 대응합니다; 런타임에 LeanLocalization에 등록됩니다(§4/§6 참조). **death 텍스트는 texts.json에 들어가지 않습니다**: codegen이 `mod_set_death_text(<제목>, <텍스트>)` 두 인자 lua_str 리터럴을 방출합니다(§3.1/§6 참조).
- 항목/JSON/Lua 순서와 ZIP 시간/권한을 고정하여 동일 Python/zlib 도구 체인에서는 같은 입력을 바이트 단위로 재현합니다. `package-content.sha256`은 압축 결과와 무관한 논리 콘텐츠 해시입니다. 서로 다른 도구 체인 사이의 완전한 reproducible build는 보장하지 않으며, 이 해시는 서명이나 공식 인증이 아닙니다.
- 에디터의 “파일 → Mod 패키지 검사”는 Manifest, Story, Lua, Texts, 에셋, 사용자 콘텐츠, 크기와 항목별 SHA-256을 읽기 전용으로 보여 주고 호환성, 형식, 논리 해시 및 참조 차이를 검사합니다. Lua 실행, 디스크 추출 또는 콘텐츠 가져오기는 하지 않습니다.

## 2. manifest.json

```json
{
  "format": 1,
  "package_format": 1,
  "story_schema": 1,
  "content_schema": 1,
  "min_host_version": "0.6.0",
  "tested_host_version": "0.6.0",
  "tested_game_version": "1.2.3",
  "id": "demo_mod",
  "name": "示例 Mod",
  "version": "1.0.0",
  "author": "somebody",
  "description": "一句话简介",
  "entry": "main",
  "campaign": {
    "new_game": true,
    "disable_official_events": true,
    "triggers": [
      {"type": "position", "position": "Center", "script": "train", "when_flag_set": "SOME_FLAG"}
    ]
  }
}
```

`package_format` / `story_schema` / `content_schema`는 각각 패키지, Story, 사용자 콘텐츠의 명시적 형식 버전이며 현재 모두 `1`입니다. `format: 1`은 구형 reader 호환용입니다. 알 수 없는 버전이나 서로 충돌하는 선언은 에디터, 컴파일러, Runtime에서 모두 거부됩니다.

구형 v1 Story/사용자 콘텐츠는 원본 바이트를 `*.pre-migration-v1.bak`에 먼저 보관한 뒤 같은 디렉터리에서 원자적으로 마이그레이션합니다. 검증, 백업 또는 교체에 실패하면 원본을 덮어쓰지 않으며 알 수 없는 필드는 유지합니다. `migration.restore_migration_backup`으로 명시적으로 복구할 수 있습니다. 구형 `.lommod` 가져오기는 메모리 사본만 변환하고 원본 패키지는 수정하지 않습니다.

- `format`: 고정값 `1`.
- `id`: mod 고유 id(`[a-z0-9_\-]+`), 런타임 등록 이름의 접두사로 충돌을 방지합니다.
- `entry`: 진입 스토리 스크립트 id, 반드시 존재해야 합니다.
- `min_host_version`은 SemVer 하드 요구 사항이며, 현재 Host가 `tested_host_version`보다 높으면 경고만 표시합니다. `game_version`은 Unity의 실제 `Application.version`과 정확히 일치해야 하는 하드 요구 사항이고, `tested_game_version` 불일치는 경고입니다. 네 필드는 모두 선택 사항이며 필드가 없는 구형 manifest는 기존처럼 동작합니다.
- `campaign`(선택): 캠페인 모드.
  - `new_game`: true이면 이 mod가 게임 내 mod 메뉴의 "새 캠페인 시작" 구역에 표시되며, 클릭하면 **격리 세이브 슬롯**(`SetSlot("mod_<modid>")`, 플레이어의 정상 세이브를 덮어쓰지 않음)으로 새 게임을 시작하고, 첫 번째 스토리 스크립트가 이 mod의 `entry`로 교체됩니다.
  - `disable_official_events`(선택, bool, 기본 false): true이면 이 캠페인에서 **원작 스토리 이벤트를 비활성화**합니다 — Free로 돌아올 때 장소 없는 메인/서브 스토리가 자동으로 시작되지 않으며, 맵 위치에는 이 mod의 트리거만 남습니다(적중하지 않으면 해당 위치의 기본 활동을 사용할 수 없으므로 mod가 자체 폴백 트리거를 준비해야 합니다).
  - `triggers`: 자유 모드 트리거 배열. `type="position"`: 맵 위치 `position`(PositionType 열거형 id: Mall/Center/Alchemy/Forge/BackMountain/Room1/Door/Study/Kitchen/Room2/Secret)을 클릭하면, 해당 위치의 기본 활동 스크립트가 `script`(같은 패키지의 스크립트 id)로 교체됩니다. 선택 조건이 모두 적중해야 유효합니다(다중 조건은 AND; **배열 순서=우선순위**, 런타임은 모든 조건이 적중하는 첫 번째 트리거를 사용):
    - `when_flag_set` / `when_flag_clear`: 스토리 flag(즉 `flag` 노드 AddStory의 key, 세이브에 영구 저장)가 설정됨/설정되지 않음.
    - `when_month`: 정수 1~12, 해당 월에만 유효.
    - `when_stage`: 정수 1~3(순: 상/중/하순), 해당 순에만 유효.
    - `when_affinity`: `{"character": <인물 id>, "min": <정수>}`, 호감도 ≥ min.
  - 기본적으로 공식 메인/서브가 우선; `disable_official_events` 또는 F7 임시 스위치가 유효할 때는 공식 퀘스트 판정을 건너뛰고 mod 트리거를 우선 매칭합니다.
  - **트리거는 캠페인별로 격리**: 활성 mod 캠페인이 있으면 현재 캠페인 mod의 트리거만 매칭합니다; 캠페인이 없으면 모든 mod가 매칭에 참여하고 먼저 로드된 쪽이 우선합니다(로드 순서=파일명 순서).
  - 트리거 예시(연무장: 호감도 이벤트 > 하순 저녁 수련 > 기본 산책):

```json
"campaign": {
  "new_game": true,
  "disable_official_events": true,
  "triggers": [
    {"type": "position", "position": "Center", "script": "train_affinity", "when_affinity": {"character": "brother4", "min": 3}},
    {"type": "position", "position": "Center", "script": "train_dusk", "when_stage": 3},
    {"type": "position", "position": "Center", "script": "train_any"}
  ]
}
```

## 3. story/*.json — 스토리 스크립트 형식

```json
{
  "story_schema": 1,
  "id": "main",
  "title": "显示给玩家的标题",
  "mood": false,
  "start": "n1",
  "nodes": [ ... ]
}
```

- `mood`(선택, bool, 기본 false): 기분 버블 스위치. false=매번 show 노드 끝과 매번 say 노드 전후에 `mod_hide_mood()`를 방출(공식 원형 감정 패널 숨김); true=공식 기분 버블 유지.
- `nodes`는 노드 배열; 기본적으로 배열 순서대로 실행합니다(암묵적으로 다음 노드로 goto).
- 각 노드는 고유한 `id`를 가집니다; 어떤 노드든 `"goto": "<nodeId>"`를 명시해 순차 흐름을 덮어쓸 수 있습니다.
- `choice` / `branch` / `dice`의 분기는 반드시 `goto`로 대상 노드 id를 가리켜야 합니다.
- 여러 선행 노드가 같은 노드로 합류(합류점)하는 것은 합법입니다.

### 3.1 노드 타입(전체 46종)

이 표가 현재 합법 노드 전부입니다. `combat`, `battle`, `reward`, `quest_*` 같은 고수준 Gameplay 이름은 아직 노드가 아닙니다. 전투 기능은 `enemy`, `battle_skill`, `goto_scene`의 `Combat` / `Battle`뿐이며 검증된 원작 API를 호출합니다.

**연출류**

| type | 필드 | 설명 |
| --- | --- | --- |
| `music` | `name`; 선택 `op`("play"기본/"stop"/"fadeout"), fadeout 시 `seconds`(기본2) | 공식 이름 그대로 `PlayMusic` / `StopMusic` / `FadeOutMusic(seconds)` 후 **`wait(seconds)`**. `name`이 `user:`로 시작하면 사용자 콘텐츠 참조(§8)이며, 런타임이 **현재 패키지**의 `assets/user/`에서 해석해 재생합니다. 로컬 절대 경로 금지 |
| `sound` | `name`; 선택 `kind`("sound"기본/"env"), `op`("play"기본/"fadeout"은 env만, `seconds`기본1) | 공식 이름 그대로 `PlaySound` / `PlayEnvSound` / `FadeOutEnvSound(seconds)` 후 마찬가지로 **`wait(seconds)`**. `user:` 참조 규칙은 music과 같고, `audio_kind`는 반드시 노드와 일치해야 합니다 |
| `scene` | `view` | 장면 전환: `runblock(flowcharts.view,"out")` 후 `ViewName=view; runblock(...,"view")`. `view="out"`은 페이드아웃만; `"black"/"white"`는 단색. 단색이 아닌 view는 먼저 `runwait(flowcharts.LoadView(view))`로 배경 에셋을 미리 로드합니다(미리 로드하지 않으면 배경이 검은 화면) |
| `background` | `action`(`set`/`show`/`replace`/`fadein`/`fadeout`/`clear`), 표시 동작은 `image`(`user:` 이미지) 필수, `fade`(기본0.5) 선택 | 현재 패키지의 사용자 이미지를 사용자 배경으로 표시합니다. 챕터·공식 `scene`·장면 전환·재로딩 시 자동 정리하며 원작 View 리소스는 수정하지 않습니다 |
| `custom_cg` | `action`(`show`/`hide`), show는 `image` 필수, `fade`·`scale`·`x/y` 선택 | 인물 레이어 앞에 사용자 이미지 CG를 표시합니다. 확대와 중심 위치를 조절하고 hide·챕터·장면 전환 시 정리합니다. 공식 `cg`는 변경하지 않습니다 |
| `overlay` | `action`(`show`/`hide`)과 `slot` 필수. show는 `image` 필수. `position`·`scale`·`opacity`·`layer`·`fade` 선택 | 여러 슬롯의 전경·소품·삽화·마스크. 같은 슬롯은 교체하며 챕터·장면 전환 시 정리합니다 |
| `show` | `character`, `position`; 선택 `portrait`(기본normal), `facing`(기본right), `fadeDuration`(0), `moveDuration`(0) | 인물을 로드하고 표시. story.mood가 false이면 끝(Focus 후)에 `mod_hide_mood()`를 추가 |
| `move` | `character`, `from`, `to`; 선택 `duration`(기본1) | 이동하고 `wait(duration)` |
| `face` | `character`, `facing` | 방향 전환 |
| `hide` | `character`; 선택 `fadeDuration`(기본0) | 인물 숨기기 |
| `focus` | `character` | `characters.Focus` |
| `offset` | `character`, `x`, `y`, `duration` | 인물 오프셋 연출 `runwait(characters.MoveOffsetCoroutine(id,x,y,t))` |
| `say` | `text`; 선택 `character`, `portrait`(기본normal), `mode`("character"기본/"think"/"narrative"/"center"), 선택 `voice` | 대사/내면 독백(os_mask 포함)/내레이션/중앙 내레이션. narrative와 center는 character를 무시합니다. **읽음 메커니즘**: 텍스트를 Lua에 그대로 넣지 않고 `say(luamanager.GetStoryText("MOD_<modid>_<scriptid>_<nodeid>"))`를 방출(modid가 없으면 "MOD"로 폴백), 텍스트 본체는 texts.json에 들어가 런타임이 등록합니다. **`voice`**(선택): 사용자 오디오 참조, 예: `user:mohui.line_01`; 이 문장 진입 전 `mod_play_voice`(이전 문장을 먼저 정지), `say()` 반환 후 `mod_stop_voice`. 음성은 독립 채널을 사용하며 `sound` / `StopMusic`이 이를 멈추지 않습니다. 절대 경로와 공식 효과음 이름 금지 |
| `choice` | `options`: `[{"text","goto"}]`(2~4개); 선택 `dialog`(기본 "Options", 스킨은 §3.3) | 옵션 메뉴 `choose()` |
| `shock` | `character`; 선택 `duration`(기본0.5) | 인물 진동(flowcharts.common "shock") |
| `mask` | `show`(bool) | 독백 마스크 `os_mask.Show` |
| `intro` | 선택 `intro_source`(`official` 기본/`custom`). official은 `character` 필수; custom은 `name`,`text` 필수, 선택 `title`,`image`(패키지 내 `assets/` PNG/JPG, ≤8MB), `image_scale`(40~160, 기본100), `image_x`/`image_y`(-30~30, 기본0) | official은 원작 `runwait(intropanel.Show(character))` 호출; custom은 `mod_prepare_character_intro(title,name,text,image,scale,x,y)` 호출, 동일한 CharacterIntroPanel 재사용. 이미지는 화면 안전 영역에서 독립 레이아웃으로 비율을 유지합니다; x 양수는 오른쪽, y 양수는 위쪽, 이미지가 없으면 초상 영역 숨김 |
| `effect` | `name`; 선택 `x`,`y`,`a`,`b`,`c`(수치, 기본0/0/1/1/1), `play`(bool, 기본true) | 화면 특수효과 `effects.SetupEffect(name,x,y,a,b,c,play)`, 예: Hit_001/Blood_002/Sword_001. `play=false`는 정지 호출 방출(마지막 인자 0): **루프형 특수효과는 자동으로 소멸하지 않습니다**(예: EventBubble/Glow), 반드시 뒤에 play=false인 동일 인자 노드를 붙여 정지해야 하며, 그렇지 않으면 화면에 상주합니다(구 데이터의 `d` 필드도 여전히 호환: play가 없으면 d 사용) |
| `transition` | `phase`("in"/"out"); 선택 `dir`(기본"lr", lr/rl/tb/bt) | 암전 전환 `runwait(transitionblack.TransitionIn/Out(dir))`. **반드시 쌍으로 사용해야 합니다**: TransitionIn은 스토리 UI를 숨기고 화면을 검은 막으로 덮으며, TransitionOut이어야 복구됩니다; in만 있고 out이 없으면 컴파일러 경고(화면이 계속 검은 상태) |
| `camera` | `name`, `active`(bool) | 카메라 필터 `maincamera.ActiveVolume(name, 0 | 1)`, 예: stage-memory/stage-dream/stage-fire/stage-blurdim |
| `block` | `flowchart`("view"/"common"), `name`; 선택 `vars`: `[{"name","value"}]` | 범용 flowchart 블록 호출: `getvar`로 하나씩 대입한 후 `runblock(fc, name)`. out_white/shake/flash/vshock 등을 커버 |
| `cg` | `action`("show"/"hide"), `kind`("picture"/"item"/"big"/"map"/"family"/"title"); 선택 `key`, `key2`, `n1`, `n2` | mainui 이미지/지도/가계도/제목: `ShowPicture(key)`/`HidePicture`/`ShowItemPicture`/`ShowBigPicture`/`ShowMap(key,key2)`/`ShowFamilyTree(key,key2,n1,n2)`/`DisplayTitle(key)` 등 |
| `dim` | `character`, `dimmed`(bool 필수, 기본 true) | 인물 어둡게 처리 `stage.SetDimmed(character, dimmedState)`(실인자는 character가 앞, bool이 뒤; dimmed=true이면 공식 구현이 해당 캐릭터의 기분 버블도 숨김) |
| `message` | `text`(필수, 비어 있으면 안 됨, 여러 줄 가능) | 시스템 알림 `mainui.DisplayMessageText(text)`가 **원문**을 표시(DisplayMessage는 로컬라이제이션 key로 해석되므로, Text 버전을 사용해 사용자 정의 텍스트가 key로 조회되어 빈 값이 되는 것을 방지) |
| `rotate` | `character`, `angle`(int 필수, 기본 180), `duration`(float 필수, 기본 1, >0) | 인물 회전 `characters.Rotate(key, angle, duration)` — **공식 매개변수 순서는 angle이 앞, duration이 뒤** |
| `dayenv` | `day_type`(int 필수, 1=낮 / 2=밤) | 낮/밤 환경 `luamanager.SetGameDayEnvironment(day_type)`. **필드명 day_type**: 노드 공통 키 "type"과의 충돌 방지 |

**수치/상태류**

| type | 필드 | 설명 |
| --- | --- | --- |
| `stat` | `key`, `delta`; 선택 `waitDisplay`(기본true), `display`(기본1), `mode`(기본"") | 주인공 속성 증감 `statmodifymanager.Player(key, delta, mode, display)` |
| `stat_set` | `key`, `value`; 선택 `update`(bool 기본false) | 절대 설정 `SetPlayer(key, value)`; update=true는 `UpdateSetPlayerStat` 사용(title 등에 사용) |
| `affinity` | `character`, `delta` | 인물 호감도 `statmodifymanager.Character(character, delta, 1)` |
| `talent` | `talent`, `level`(±1) | 재능 `statmodifymanager.AddTalent(id, level)` |
| `item` | `kind`("book"/"misc"/"special"), `item`, `count`(기본1); 선택 `remove`(bool 기본false) | 아이템 증감 `AddBook/AddMisc/AddSpecial(id,count)`; remove 시 `RemoveBook/RemoveMisc(id)`(book/misc만) |
| `flag` | `flag` | mod 스토리 flag: `statmodifymanager.AddStory(flag)` + `modflags[flag]=true` |
| `game_flag` | `flag`, `value`; 선택 `op`("set"기본/"add") | 공식 퀘스트 flag: `SetFlag(id, 상태)` / `AddFlag(id, ±증분)`. **id는 반드시 게임에 이미 있는 FlagData여야 합니다**(14_속성과Flag 표), 그렇지 않으면 게임이 조용히 무시 |
| `enemy` | `op`("team"/"level"/"people"/"id"), `enemy`, `value`(수치, id의 op는 불필요), `display`(기본1) | 적 팀 수정 `ModifyEnemyTeam/Level/People/Id` |
| `battle_skill` | `op`("set"/"active"/"reset"), `key`(reset 불필요), `index`(set용, 기본2), `active`(active용, 기본1) | 전장 스킬 `SetPlayerBattleSkill/SetBattleSkillActive/ResetBattleSkill` |
| `mission` | `name`, `key` | 퀘스트 조작 `statmodifymanager.Mission(name, key)`: `Mission("Main","M0001")` 메인 진행 / `Mission("S2200","clear")` 서브 클리어 |
| `time` | `op`("set"/"round"/"month"/"mission"); set은 `year,month,stage` 사용; mission은 `name,year,month,stage` 사용 | 시간 `SetGameTime/NextRound/NextMonth/SetMissionTime` |
| `autosave` | 선택 `kind`("story"기본/"free"/"prologue"); 선택 `save_button`(0/1, 세이브 버튼 별도 제어) | `AutoSave()/AutoFreeSave()/PrologueSave(mode)`; `save_button`은 별도로 `ToggleSaveButton(n)` 방출 |

**흐름류**

| type | 필드 | 설명 |
| --- | --- | --- |
| `branch` | `cases`(≥1); 선택 `source`("mod"기본/"game"/"stat"/"flag_value"/"condition"). 키 필드: source=stat이면 `stat`(속성 id, editor_data stats 목록) 사용, 나머지 출처는 `flag`(비어 있으면 안 됨) 사용 | 조건 분기, 다섯 가지 출처: mod=modflags 설정 여부; game=공식 체크포인트 `checkpointmanager.Switch(flag)`; stat=주인공 속성 `luamanager.GetStatData(stat, 1)`; flag_value=공식 퀘스트 플래그 `tonumber(luamanager.GetFlagData(flag))`; condition=공식 조건 체크포인트 `checkpointmanager.Condition(flag)`(bool). case 구조는 출처별로: mod/condition은 `[{"value","goto"}]`(value는 1/2만: mod=설정됨/설정 안 됨, condition=참/거짓); game은 `[{"value","goto"}]`(임의 정수); stat/flag_value는 `[{"op","value","goto"}]`(op 기본값 ">=", >=/>/<=/</== 허용). 미적중 시 모두 else로 순서상 다음 노드로 떨어짐(마지막 노드이면서 모든 값을 커버하지 않음 → LomcError; mod/condition은 두 case가 갖춰지면 커버됨) |
| `dice` | `check`, `options`: `[{"goto_大成功","goto_成功","goto_失败","band_texts"?}]`(정확히 1개) | 주사위 판정. **check는 반드시 공식 메타데이터가 있는 체크포인트여야 합니다**(editor_data의 dice_meta: 주사위 범위 max와 결과대 bands; 메타데이터가 없는 체크포인트는 게임 내 주사위 메뉴 NRE 크래시 유발). 공식 5단계 체인 방출 + 결과대 수에 따라 대마다 옵션 방출(텍스트+조건); 분기는 대 품질 순위에 따라 매핑: 최악 대→goto_失败, 중간 대→goto_成功, 최우 대→3대 이상이면 goto_大成功 / 2대면 goto_成功. 대 품질은 조건 수치로 추론(같은 값이면 >계열이 <계열보다 우수). **band_texts**(선택): 대마다 주사위 메뉴 옵션 텍스트 덮어쓰기(개수=결과대 수, 각 항목은 비어 있으면 안 됨, 아니면 LomcError); `<작성자 텍스트> \| <공식cond>` 방출(작성자 텍스트는 리터럴이며 texts.json에 들어가지 않음; ASCII \|는 전각｜로 정화; cond는 항상 공식 메타데이터 사용). 기본값은 공식 결과대 텍스트 |
| `goto_scene` | `scene`("Free"/"Title"/"Combat"/"Battle"/"GameOver"/"End"/"Story"/"DemoEnd"); 선택 `key`(Combat=전투id/Battle=전역id/GameOver=사망 화면id/End=엔딩 식별자), `next`, `title`, `desc`(모두 str, End/GameOver만 사용), `image`(str, **End만 사용**: 패키지 내 이미지 상대 경로, 예: `assets/ending.png`) | 일반 장면은 여전히 `luamanager.ChangeScene(scene,key,next)`. **End 특례는 원작 한청서 흐름을 따름**: 사용자 지정 제목/본문/삽화를 캐시 → `runwait(endgamepanel.Open("__MORTAL_MOD_END__"))` → 플레이어 확인 → 암전 → Title; 런타임이 진짜 `EndGamePanel`을 patch해 공식 레이아웃을 그대로 재사용; `image`는 왼쪽 페이지 `_picImage`에 쓰고, 비어 있으면 원작 엔딩 20047의 Picture를 빌려 자리를 채움; 이미지 누락/손상 시 경고만 하고 자리 채움으로 폴백. End/GameOver의 next는 무효(원작 버튼은 로드/타이틀 고정), 구 값은 무시하고 경고(구 호환값 Story는 Title로 처리, 경고 없음). 사용자 지정 콘텐츠 없이 공식 key만 준 경우에만 공식 엔딩 항목을 직접 엽니다(원작 방식으로 잠금 해제/기록하고 경고 출력). mod 전용 End key에 title/desc/image가 없거나, mod 전용 GameOver key에 title/desc가 없으면 검증에서 바로 실패해 빈 카드를 방지 |
| `panel` | `panel`("martial"/"weapon"/"poison"/"cg"/"cgvideo"/"shop"/"newshop"/"credit"/"endgame"); 선택 `key`(cg/cgvideo/endgame의 id), `discount`(shop용, 기본0), `mode`(martial용, 기본0) | 시스템 패널 열기, newshop 외에는 모두 `runwait`: `martialpanel.Open(mode)`/`weaponupgradepanel.Open()`/`poisonupgradepanel.Open()`/`cgpanel.Open(key)`/`cgvideopanel.Open(key,0)`/`shoppanel.Open(discount)`/`shoppanel.NewShop()`/`creditpanel.Open()`/`endgamepanel.Open(key)` |
| `wait` | `seconds` | `wait(seconds)` |
| `end` | 선택 `next_script` | 있음: `SetNextScript("MOD_<modid>_<id>")`+`Init()`로 같은 패키지 스크립트에 체인; 없음: `ChangeScene("Free","","")`로 자유 모드 복귀 |
| `death` | `text`(필수, 비어 있으면 안 됨, 여러 줄 가능), `death_id`(필수); 선택 `title`(str, 기본값 「勝敗乃兵家常事」), 구 필드 `next` | **사망 텍스트**: 검은 화면 전환(view="black") → `mod_set_death_text(title, text)`(두 인자 lua_str 리터럴, **texts.json / 읽음 시스템에 들어가지 않음**) → `luamanager.ChangeScene("GameOver", death_id, "Title")`로 **공식 GameOver 사망 화면** 진입(검은 바탕 붉은 글자 + 로드/타이틀 버튼, §6 참조); 원작은 사용자 지정 next를 읽지 않으며, 구 값은 무시하고 경고. `death_id`는 반드시 ≥900000의 mod 전용 숫자 id여야 함(아니면 LomcError, 「사망/엔딩 id 약정」 참조). 종료 노드(자체 흐름 이행 포함, 명시적 goto 불가, 마지막 노드로 마무리 가능) |
| `raw` | `code` | 네이티브 Lua 탈출구: 코드를 그대로 삽입(여러 줄 가능). **메커니즘 폴백**: 어떤 노드로도 표현할 수 없는 공식 메커니즘에 사용 |

### 3.2 자주 쓰는 값(권위 목록은 data/editor_data.json, schema 2부터 중국어 이름 포함)

- 포지션 position: `SL L1 L2 M R1 R2 RM2 SR …`(총 36개, S=화면 밖 L=왼쪽 M=중앙 R=오른쪽 B=뒤 C=중앙)
- 표정 portrait: `normal nervous1..3 angry1 angry2 laugh1 gloomy2 …`(인물 설정에 따름, 없으면 게임이 첫 번째 입체 일러스트로 폴백)
- say mode: `character` 대사 / `think` 내면 독백 / `narrative` 내레이션 / `center` 중앙 내레이션
- stat key: `mental(심상) money(은자) disposition behaviour karma fame talking team …`(31개)

### 3.3 옵션 메뉴 스킨(choice.dialog)

**`Options`만 사용 가능**(기본값, 순수 텍스트 옵션; Dice는 주사위 노드 내부 전용). 나머지 스킨(Talk/Meet/Door/Section_* 등)은 자유 장면의 break 형식 메뉴(옵션 텍스트가 `타입+key+행동 포인트+기여` 네 단락 `+` 구분)이며, 순수 텍스트 옵션은 `BreakOptionButton.UpdateContent`의 IndexOutOfRange 크래시(메뉴 동결)를 유발합니다 — 컴파일러가 오류를 내고 거부합니다. 방출: `setmenudialog(menudialogs.Options)` → `choose()` → `menudialogs.Options.SetActive(false)`.

## 4. story.json → Lua 컴파일 규약(lomc 구현)

- 각 노드는 하나의 Lua 함수로 컴파일됩니다; 파일 앞부분에서 전방 선언 `local node_n1, node_n2, ...`, 이어서 `node_nX = function() ... end`; 흐름 이행은 꼬리 호출 `return node_<goto>()`; 최상위는 `return node_<start>()`.
- 텍스트 이스케이프: `\`→`\\`, `"`→`\"`, 줄바꿈→`\n`, `\r`→`\r`.
- 각 스크립트 앞부분에 `modflags = modflags or {}`(전역 테이블, Story 장면 세션 내에서 유지, 체인 스크립트 간 공유; 세이브하지 않음)를 방출하고, 바로 이어서 한 줄 `mod_set_mood(true|false)`(story 최상위 mood 선언, 기본 false; §6 참조)를 방출합니다.
- `flag` 노드는 이중 방출: `AddStory` + `modflags[flag]=true`.
- **분기 폴백**: choice 외의 모든 다중 경로 구조는 조용히 빠져나가는 것을 허용하지 않습니다 — case 미적중 시 else로 순서상 다음 노드로 떨어짐; 폴백 불가(branch가 마지막 노드이면서 모든 반환값을 커버하지 않음)는 검증 오류로 간주.
- 노드 id 문자 집합 `[a-zA-Z0-9_]+`(스크립트 id는 `-` 허용).
- story 최상위 `title`은 선택.
- **읽음 key 규칙**: 모든 say(character/think/narrative/center) 노드의 텍스트는 일률적으로 `say(luamanager.GetStoryText(key))`를 방출, key = `MOD_<modid>_<scriptid>_<nodeid>`; modid는 manifest에서 가져오며(패키징 시), 독립 build/에디터 미리보기에서 없으면 "MOD"로 폴백. **death 텍스트는 읽음 key를 거치지 않습니다**: `mod_set_death_text(<제목 리터럴>, <텍스트 리터럴>)` 방출(제목 기본값/빈 문자열은 「勝敗乃兵家常事」), 텍스트는 texts.json에 들어가지 않습니다.
- **엔딩/사망 카드 규칙**: goto_scene scene=End이면서 title/desc/image가 있으면 먼저 `mod_set_ending_text(...)`를 방출한 후 원작 한청서 흐름대로 표시; image는 왼쪽 페이지 삽화이며 전체 화면 배경이 아닙니다. scene=GameOver에 title/desc가 있으면 `mod_set_death_text(<title>, <desc>)`로 변경. death 노드도 마찬가지로 `mod_set_death_text(<title>, <text>)` 방출(두 인자; 단일 인자 구 패키지 호환은 여전히 런타임이 지원). 두 전역 호출은 런타임 플러그인이 등록합니다(§6).
- **mood 규칙**: story.mood=false이면 show 노드 끝(Focus 후)과 say 노드의 say(...) 전후에 각각 한 번씩 `mod_hide_mood()`를 방출; true이면 방출하지 않습니다.
- **death 방출**: §3.1 death 행 참조(runblock out → ViewName="black" → runblock view → `mod_set_death_text(title, text)` → ChangeScene("GameOver", death_id, next)).
- 마지막 노드가 `end`/`death`/`goto_scene`/`raw`가 아니고 goto도 없음 → 검증 오류.
- `choice`/`branch`/`dice`/`end`/`death`/`goto_scene`에 명시적 `goto` 작성 → 검증 오류.
- `say`의 narrative/center 모드에 character를 주는 것은 허용하지만 무시합니다.
- `raw` 노드 내용은 그대로 삽입(컴파일러가 구문 검사를 하지 않음); 그 뒤의 흐름 이행은 정상(순차/goto).
- **치명적이지 않은 경고**: `-- lomc 警告：` 주석 형태로 Lua 앞부분에 삽입(예: transition에 in만 있고 out이 없음); `lomc check`도 stderr에 동기 출력합니다.

핵심 API 패턴:

```lua
-- show
runwait(characters.LoadCharacterAsset("player"))
stage.show{character=characters.Get("player"), portrait=characters.GetPortrait("player", "normal"), fromPosition="RM2", toPosition="RM2", facing="right", fadeDuration=0, moveDuration=0, useDefaultSettings=false}
characters.Focus("player")
-- say (character 模式)
setsaydialog(saydialogs.character)
sayoptions.waitforinput = true
sayoptions.fadewhendone  = true
stage.showPortrait(characters.Get("player"), characters.GetPortrait("player", "nervous1"))
setcharacter(characters.Get("player"), characters.GetPortrait("player", "nervous1"))
characters.Focus("player")
mod_hide_mood()
say(luamanager.GetStoryText("MOD_demo_mod_main_n7"))
mod_hide_mood()
-- say (think)：setsaydialog(saydialogs.think)，say 前 os_mask.SetLastPosition(); os_mask.Show(true)，后 os_mask.Show(false)
-- say (narrative/center)：setsaydialog(saydialogs.narrative|saydialogs.center); sayoptions 两行同上; setcharacter(narrative); say(GetStoryText(...))（任何 say 前都设 sayoptions）
-- choice（含皮肤）
setmenudialog(menudialogs.Options)
local option1 = {}
option1[1] = "选项一"
option1[2] = "选项二"
local choice1 = choose(option1)
menudialogs.Options.SetActive(false)
if choice1 == 1 then return node_a() elseif choice1 == 2 then return node_b() end
-- scene
runblock(flowcharts.view, "out")
getvar(flowcharts.view, "ViewName").value = "center"
runblock(flowcharts.view, "view")
-- shock
getvar(flowcharts.common, "ShockPosition").value = characters.Get("brother4").State.holder.gameObject
getvar(flowcharts.common, "ShockDuration").value = 0.5
runblock(flowcharts.common, "shock")
-- effect / transition / camera（循环特效必须成对 play=0 停止，如 EventBubble/Glow）
effects.SetupEffect("Hit_001", 10, -5, 1, 1, 1, 1)
effects.SetupEffect("Glow_001", -4.5, 0, 1, 1, 1, 0)
runwait(transitionblack.TransitionIn("lr"))
maincamera.ActiveVolume("stage-memory", 1)
-- dim / message / rotate / dayenv
stage.SetDimmed(characters.Get("trainee1"), true)
mainui.DisplayMessageText("【系统提示】欢迎来到全功能展示！")
characters.Rotate("player", 180, 1)  -- 参数序：angle 在前、duration 在后
luamanager.SetGameDayEnvironment(1)  -- 1=白天 / 2=晚上
-- stat / affinity / talent / item / flag / game_flag / enemy / mission
statmodifymanager.Player("mental", -5, "", 1)
wait(statmodifymanager.GetDisplayTime("mental"))
statmodifymanager.Character("brother4", 1, 1)
statmodifymanager.AddTalent("1010", 1)
statmodifymanager.AddMisc("2001", 50)
statmodifymanager.AddStory("SOME_FLAG_ID")
modflags["SOME_FLAG_ID"] = true
statmodifymanager.SetFlag("M0001_00", 1)
statmodifymanager.ModifyEnemyTeam("400", -10, 1)
statmodifymanager.Mission("Main", "M0001")
-- branch（source="mod"）
if modflags["SOME_FLAG_ID"] then return node_a() else return node_b() end
-- branch（source="game"，else 兜底）
local branch1 = checkpointmanager.Switch("S0003_01_001")
if branch1 == 1 then return node_a() elseif branch1 == 2 then return node_b() else return node_next() end
-- branch（source="stat"：主角属性数值比较，op 缺省 >=）
local branch1 = luamanager.GetStatData("mental", 1)
if branch1 >= 50 then return node_a() else return node_next() end
-- branch（source="flag_value"：官方任务旗标数值比较）
local branch1 = tonumber(luamanager.GetFlagData("50019"))
if branch1 >= 1 then return node_a() else return node_next() end
-- branch（source="condition"：官方条件检查点，value 1=真 2=假）
if checkpointmanager.Condition("S0030_01_001") then return node_a() else return node_b() end
-- dice（check 必须带官方元数据；band_texts 可选逐带覆写）
setmenudialog(menudialogs.Dice)
local dice_rand1 = math.random(99)
dicemenudialog.SetRandom(99, dice_rand1)
local dice_result1 = checkpointmanager.Dice("S0205_01_001", dice_rand1)
local dice_opts1 = {}
dice_opts1[1] = "O_S0205_01_001|<40"        -- 缺省：官方结果带文本
dice_opts1[2] = "O_S0205_01_002|>=40"
-- band_texts 存在时：dice_opts1[i] = "<作者文本>|<官方cond>"（作者文本为字面量，GetStoryText 查不到时原样显示）
dicemenudialog.Setup(dice_result1.ResultCount, dice_result1.Result, dice_result1.Header, dice_result1.Additions)
runwait(dicemenudialog.ExecuteRoll(dice_opts1, 1, "S0205_01_001"))
local dice_sel1 = dicemenudialog.ResultSelection
-- 分支按带质量：最差带→失败，最优带→大成功（2带→成功）
if dice_sel1 == 1 then return node_fail() else return node_ok() end
-- goto_scene（GameOver 的 key 用 mod 专属 id：9+官方 id，如 910021）
luamanager.ChangeScene("Combat", "5102_01", "Story")
-- 自定义 End：官方汗青书 EndGamePanel；留空 image 时游戏内借用原版 20047 插图占位
mod_set_ending_text("武林传奇", "你的名字，从今往后便是传说。")
luamanager.SetEnableActions(0)
runwait(endgamepanel.Open("__MORTAL_MOD_END__"))
luamanager.SetEnableActions(1)
runwait(transitionblack.TransitionIn("lr"))
luamanager.ChangeScene("Title", "", "")
-- 自定义左页插图（scene=End 且带 image 时发射三参）
mod_set_ending_text("武林传奇", "你的名字，从今往后便是传说。", "assets/ending.png")
-- panel（除 newshop 外 runwait）
runwait(martialpanel.Open(0))
runwait(shoppanel.Open(10))
shoppanel.NewShop()
runwait(endgamepanel.Open("20003"))
-- autosave / time / battle_skill / block
luamanager.AutoSave()
luamanager.SetGameTime(1, 4, 1)
luamanager.SetPlayerBattleSkill("special3", 2)
runblock(flowcharts.common, "flash")
-- end（链式 / 回自由模式）
luamanager.SetNextScript("MOD_<modid>_<scriptid>")
luamanager.Init()
luamanager.ChangeScene("Free", "", "")
-- death（黑屏过渡 → mod_set_death_text(title, text) → 官方 GameOver 死亡画面）
runblock(flowcharts.view, "out")
getvar(flowcharts.view, "ViewName").value = "black"
runblock(flowcharts.view, "view")
mod_set_death_text("勝敗乃兵家常事", "你坠入山崖，万事休矣。")
luamanager.ChangeScene("GameOver", "910021", "Title")
```

## 5. data/editor_data.json — 에디터 데이터 규약(schema 3)

`tools/extract_editor_data.py`가 생성합니다. schema 2부터 `characters`/`stats`/`positions`/`views`/`music`/`free_positions`는 모두 `{id, name}` 객체 배열(characters는 portraits 추가); schema 3은 `dice_meta`(주사위 체크포인트 메타데이터: `{check: {max, bands: [{text, cond}]}}`, bands는 공식 표시 순서)와 `death_ids`/`ending_ids`의 확충 객체 배열(name은 `data/ref/death_ending_ids.json`에서 가져옴, 아래 「사망/엔딩 id 약정」 참조)을 추가했습니다. **dice_meta는 스토리 장면 체크포인트만 포함**: 여행 시스템 체크포인트(Travel_*)는 스토리 장면의 CheckPointManager에서 조회되지 않으면 크래시하므로 추출 시 제거했습니다; `dice_checks`는 전체 이름 목록으로 모든 호출 지점(여행 포함)을 유지합니다:

```json
{
  "schema": 3,
  "characters": [{"id": "brother4", "name": "唐惟元", "portraits": ["normal", "nervous1"]}],
  "views": [{"id": "center", "name": "校場_白天"}],
  "music": [{"id": "普通_001", "name": "普通_001"}],
  "positions": [{"id": "RM2", "name": "右中2"}],
  "stats": [{"id": "mental", "name": "心相"}],
  "free_positions": [{"id": "Center", "name": "练功场"}],
  "modes": ["character", "think", "narrative", "center"],
  "menu_dialogs": ["Options", "Talk", "Meet", "Center", "..."],
  "effects": [{"id": "Hit_001", "name": "Hit_001"}],
  "dice_checks": ["S0205_01_001", "Travel_601_101_001"],
  "dice_meta": {"S0205_01_001": {"max": 99, "bands": [{"text": "O_S0205_01_001", "cond": "<40"}, {"text": "O_S0205_01_002", "cond": ">=40"}]}},
  "combat_ids": ["5102_01"],
  "battle_ids": ["A0001_01"],
  "death_ids": [{"id": "10021", "name": "乱战中被践踏而死"}],
  "ending_ids": [{"id": "20003", "name": "唐门叛徒"}],
  "game_flags": [{"id": "M0001_00", "name": "…"}],
  "talents": [{"id": "1010", "name": "…"}],
  "items_book": [{"id": "6002", "name": "…"}],
  "items_misc": [{"id": "2001", "name": "…"}],
  "items_special": [{"id": "2001", "name": "…"}],
  "messages": [{"id": "M_Add_Misc_2002", "name": "…"}],
  "affinity_characters": ["brother4"]
}
```

### 사망/엔딩 id 약정(mod 전용 구간)

공식 GameOver/EndGamePanel은 id로 LibrarySystem을 조회하고 LibraryItemData.Add()를 실행할 수 있습니다(공식 엔딩 잠금 해제/기록). 사용자 지정 End는 존재하지 않는 내부 key를 고정 사용해 공식 엔딩을 조회/쓰지 않습니다; "사용자 지정 콘텐츠 없이 공식 End key를 직접 여는" 경우에만 원작 방식으로 기록합니다.

- **mod 사망/엔딩 id = `9<공식id>`**(900000 구간): 공식 사망 10021 → mod 910021; 공식 엔딩 20003 → mod 920003. 공식 1xxxx(사망)/2xxxx(엔딩)/4xxxx(후일담) 전 구간과 충돌하지 않습니다.
- 임의로 만든 GameOver id는 공식 항목을 찾을 수 없어 → 부작용 없음, 텍스트는 플러그인이 주입; 임의로 만든 End id는 mod 내 식별자로만 쓰고, 실제 표시는 고정 내부 key를 거칩니다.
- `death` 노드의 `death_id`는 ≥900000 정수로 검증합니다. 내용이 빈 자작 GameOver/End 카드는 컴파일러가 거부합니다; 사용자 지정 콘텐츠 없이 공식 key를 직접 사용하면 치명적이지 않은 세이브 오염 경고를 출력합니다.
- 권위 참조: `data/ref/death_ending_ids.json`: `death` 106개(10000~10104, 11000), `ending` 54개(20000~20053), `epilogue` 4개(40000~40003). 추출기는 그 제목으로 editor_data의 death_ids/ending_ids를 확충합니다; 에디터 death_id 입력란에는 공식 참조 앞 5개가 나열됩니다.

## 6. 런타임 플러그인 동작(MortalModHost)

1. 시작 시 `BepInEx/plugins/MortalModHost/mods/*.lommod`를 스캔해 `MOD_<modid>_<scriptid>` → lua 텍스트를 등록합니다.
2. Harmony prefix `LuaManager.ExecuteLuaScript()`: 등록 이름이 적중하면 mod lua로 실행하고 원래 메서드를 건너뜁니다.
3. 진입: Free 자유 장면과 Title 타이틀 화면 왼쪽 아래의 "活侠MOD" 버튼 + F8(설정 가능)로 메뉴를 엽니다. Free 메뉴는 "mod 스토리 연출"과 "새 캠페인 시작" 두 구역; Title 메뉴는 "새 캠페인 시작" 구역만(스토리 연출은 로드된 세이브 플레이어 상태가 필요하므로 Free에서만 제공).
4. **캠페인**: "새 캠페인 시작" 클릭 → `SetSlot("mod_<modid>")`(격리 세이브 슬롯) → 공식 `NewGameData()` → postfix가 첫 번째 스토리 스크립트를 해당 mod의 entry로 교체 → LoadStory.
5. **원작 스토리 억제와 위치 트리거**: `disable_official_events` 또는 F7이 유효할 때, `UpdateCheckMissions` 내에서 메인 트리거 상태를 잠시 숨기고 `HasAnyMissionTrigger`가 false를 반환해, Free로 돌아올 때 공식 메인/서브가 자동으로 시작되는 것을 막습니다; 장소 클릭 postfix `FreePositionData.GetExecuteScript`는 manifest.triggers를 우선 매칭하고, mod 적중이 없으면 공식 장소 기본 스크립트를 억제합니다.
6. **폴백**: Story 장면이 요청한 MOD_ 스크립트가 미등록(mod 삭제됨)이면 실행하지 않고 `ChangeScene("Free","","")`로 소프트락을 방지합니다.
7. mod는 공식 스크립트와 텍스트 테이블을 수정하지 않습니다; mod의 flag는 StoryKeyList에 들어가 세이브와 호환됩니다.
8. **texts.json 등록**: .lommod 로드 시 texts.json의 key→텍스트를 LeanLocalization에 등록(`Story/`+key); `GetStoryText`는 key로 읽음 시스템을 조회: 읽음→노란색+빨리 감기 가능, 읽지 않음→정상 색+읽음 기록, 찾을 수 없음→key 자체 반환.
9. **mod_hide_mood**: 전역 Lua 함수 `mod_hide_mood()`(무인자) 등록, 전장 캐릭터의 원형 감정 패널(CharacterMoodPanel) 숨김; 컴파일러는 story.mood 스위치에 따라 show/say에서 방출(§4 참조).
10. **mod_set_mood**: 전역 Lua 함수 `mod_set_mood(bool)` 등록, 스크립트 앞부분 선언에 따라 공식 기분 패널 스위치(ShowMood)를 강제 제어하며, 각 mod 스크립트 진입 시 한 번 방출하고 체인 스크립트는 스크립트마다 전환되어 적용됩니다.
11. **UpdateTranslations 방 wipe**: 공식 텍스트 새로 고침이 플러그인이 등록한 mod 텍스트를 지우므로, 반드시 hook하고 새로 고침 후 texts.json 등록을 다시 재생(로드 시 전체 등록 항목 캐시)해 mod key가 절대 실효되지 않도록 보장합니다.
12. **인물 소개 카드**: 공식 인물은 원래 `CharacterIntroPanel.Show(key)` 동작 유지; 사용자 지정 인물은 특수 key에서 Harmony가 이어받아 공식 패널 레이아웃을 재사용해 사용자 지정 칭호/이름/본문을 씁니다. 선택 `image`는 현재 `.lommod`의 `assets/`에서 디코딩해 독립 안전 레이아웃에 배치: 기본 중심 화면 `(31%,50%)`, 최대 너비/높이 화면 `(30%,62%)`, 비율 유지; `image_scale`은 자동 적합 크기에서 확대/축소, `image_x/image_y`는 화면 백분율로 미세 조정. 닫을 때 임시 텍스처를 파괴하고 원작 컨트롤을 완전히 복구합니다; 이미지가 없으면 초상 영역을 숨기며, 공식 로컬라이제이션 테이블이나 관계 데이터를 수정하지 않습니다.
13. **엔딩/사망 카드 그리기**: 두 개의 전역 Lua 함수 등록(§3.1/§4 참조):
    - `mod_set_death_text(title, desc)`: 사망 제목/설명 캐시; Harmony postfix `GameOverController`가 두 단락 텍스트를 공식 `_titleText`/`_descTextPrefab` 컨트롤러에 쓰고, 공식 레이아웃으로 사망 화면 중앙에 표시. 단일 인자 호출은 구 규약에 따라 desc로 간주하고 제목은 비워 둡니다(구 패키지 호환).
    - `mod_set_ending_text(title, desc[, image])`: 엔딩 제목/설명과 선택적 패키지 내 이미지 캐시; Harmony postfix가 `EndGamePanel.Open`을 감싸 공식 첫 캔버스 fade 전에 `_titleText/_descText`와 왼쪽 페이지 `_picImage`에 씁니다; 이미지가 없으면 공식 엔딩 20047의 Picture를 빌려 자리를 채웁니다. 공식 페이드인, 확인 대기와 페이드아웃은 모두 유지; 표시 동안 `_saveLibrary`를 임시로 꺼 mod key가 전기(傳記) 세이브 슬롯에 들어가는 것을 막고, 끝나면 복구합니다.
    - 새 컴파일러의 사용자 지정 End는 더 이상 단순화된 `EndGameController` 장면으로 들어가지 않습니다; 구 패키지는 여전히 원래 End 장면 덮어쓰기 호환을 유지합니다. 텍스트 없는 GameOver 자작 id와 내용 없는 End 자작 id는 모두 컴파일 시 차단됩니다.
14. **에디터 단회 플레이 테스트 프로토콜**: 에디터는 진입 챕터의 `start`를 현재 선택 노드로 임시 변경하고, 고정 패키지 `__lom_modkit_preview.lommod`(manifest id `lom_modkit_preview`)로 설치한 뒤, 플러그인 디렉터리에 `preview-request.json`을 원자적으로 씁니다. 런타임은 0.35초마다 한 번씩 확인합니다: Free 장면은 바로 연출, Title 장면은 `mod_lom_modkit_preview` 격리 슬롯으로 시작, 기타 장면은 안전 장면까지 대기; 소비 후 요청과 임시 패키지를 삭제합니다. 요청은 format=1과 `[A-Za-z0-9_-]+`의 mod/script/node id만 받아들이며, 정식 Mod 패키지는 자동 삭제 범위에 포함되지 않습니다.
15. **mod 새 캠페인에 운명 2점 지급**: 공식 새 게임은 운명 포인트를 가지고 시작하지만 mod 격리 세이브는 초기 0이므로, 주사위 「역천(逆天)」 흐름(`DiceMenuDialog.CheckRevolution`이 命運>0 요구)이 mod 캠페인에서 사용 불가; NewGameData postfix는 첫 스크립트 교체 후 mod 캠페인에 `GameStatType.命運`을 2점 추가합니다. 공식 새 게임은 영향을 받지 않습니다.
16. **mod 스토리의 주사위 범위 수정 개방**: 공식 「범위 수정」 버튼은 2회차이면서 업적 30016 보유를 요구; mod 스토리 중(`CurrentStoryScript`가 `MOD_`로 시작) `get_NewGamePlus` prefix가 true를 반환하고, `CheckRevolution`이 원래 true를 반환할 때 `_rangeButton`을 바로 활성화합니다(mod에서 공식 업적 30016을 잠금 해제하지 않아 공식 세이브 오염 방지). 공식 스토리는 전혀 영향을 받지 않습니다.
17. **사용자 오디오**: `LuaManager.PlayMusic/PlaySound/PlayEnvSound` 인자가 `user:`로 시작하면 플러그인이 이어받아 **현재 연출 중인 Mod 패키지**의 `UserContents`에서 해석(`assets/user/audio/<id>/content.json` + 메인 파일)하고, 디코딩 후 Windows `waveOut`으로 재생합니다(이 게임의 메인 믹싱은 Wwise이며 Unity `AudioSource`는 종종 소리가 나지 않음). 공식 이름은 모두 원작 Wwise로 넘깁니다. 런타임은 `%APPDATA%/lom_modkit/repository` 읽기를 금지합니다. 두 Mod가 ID가 같아도 각자 자신의 패키지만 해석합니다. 지원 형식은 `.ogg` / `.wav`뿐이며 개당 ≤20MB. 사용자 지정 fadeout은 출력 볼륨 페이드아웃(이후에도 컴파일러가 방출한 `wait`가 있음); 사용자 지정 음악으로 전환할 때 먼저 공식 Wwise 음악을 정지합니다(공식 `StopMusic`은 환경음도 함께 지움).
18. **대사 음성**: `mod_play_voice(ref)` / `mod_stop_voice()` 등록. `mod_play_voice`는 현재 음성을 먼저 정지한 후 재생(루프 없음, 독립 `_voice` 채널 사용). `sound` 노드, 사용자 지정 효과음, `StopMusic` 모두 이 채널을 건드리지 않습니다. 스토리 중단, 공식 스크립트 전환, Mod 재로드 시 `StopEverything()`이 음성을 정지합니다. `voice`가 없는 구 Lua는 이 두 함수를 호출하지 않으므로 동작이 변하지 않습니다.
19. **게임 내 Mod 메뉴 다국어**: 메뉴 문구(`src/I18n.cs`에 zh_CN/zh_TW/ja/ko 4개 언어 목록 내장)는 게임의 현재 언어를 따릅니다 — 리플렉션으로 LeanLocalization `CurrentLanguage`를 읽고 언어 이름을 퍼지 매칭; 공식 게임 자체에는 일본어 옵션이 없어 일본어 목록은 실제로 발동하지 않습니다; 감지 실패 시 일률적으로 zh_CN으로 폴백. 자세한 내용은 `i18n.md` 참조.

20. **구조화 Runtime 오류**: Mod 재생을 fail-closed로 중단시키는 장애는 한 줄의 `[mod-runtime-error]` JSON 로그로 기록됩니다. 고정 필드는 `mod_id`, `mod_name`, `version`, `story`, `node`, `category`, `error`, `recent_trace`와 UTC 시간입니다. 일반 Mod는 변수 값을 포함하지 않는 노드/이동 breadcrumb를 최대 32개만 보관하며 오류에는 길이가 제한된 최근 16개만 첨부합니다. F5의 전체 256개 개발 trace 규칙은 그대로입니다. 예외 포맷, trace 조회, JSON 직렬화 또는 로그 출력 자체가 실패해도 최소 보고서로 대체하여 원래 오류나 안전한 Free 복귀를 방해하지 않습니다. 마지막 보고서는 진단 번들을 위해 메모리에 유지됩니다.

## 7. AI 도구 인터페이스(story_api)

editor/story_api.py는 AI/에디터 공용의 통제된 쓰기 진입구입니다. 규칙: **AI는 story JSON이나 Lua를 직접 손으로 작성하지 않습니다**,
모든 스토리 구축은 story_api를 거칩니다(models 규약 기본값 + lomc 검증/경고). 주사위 메뉴 크래시,
transition 검은 막, choice 스킨 크래시, 배경 검은 화면, 인물이 등장하지 않은 채 동작하는 등의 알려진 함정을 막습니다.

- Python API:
  - `load_editor_data()`: 에디터 데이터 읽기(dice_meta 등 목록 포함), (editor_data, is_fallback) 반환
  - `new_story(story_id="main", title="新剧情", mood=False)`: 새 스토리 스크립트 작성(show 등장 + 빈 say 두 노드 오프닝, 등장 후 동작)
  - `add_node(story, node_type, fields=None, after=None)`: models 기본값으로 노드 추가(46종), 알 수 없는 타입/필드/타입 불일치→ValueError, 노드 id 자동 생성, after로 삽입 위치 지정(노드 id 또는 None=끝). 등장 방어선: 동작류 노드의 대상 인물이 앞에서 등장하지 않았거나 이미 퇴장했으면 그 앞에 자동으로 show 삽입
  - `update_node(story, node_id, fields)`: 노드 필드 업데이트(add와 같은 필드 검증), 노드 없음→ValueError. 등장 방어선: 업데이트 후 동작 인물이 미등장/퇴장 상태이면 해당 노드 앞에 자동으로 show를 삽입하고, 그곳을 가리키는 goto/옵션/분기 점프를 새 노드로 변경
  - `get_node(story, node_id)`: 노드 읽기, 없음→ValueError
  - `list_nodes(story)`: [{"id","type","summary"}] 목록 반환
  - `delete_node(story, node_id)`: 노드 삭제, 없음→ValueError
  - `rename_node(story, node_id, new_id)`: 노드 id 이름 변경 및 start와 모든 점프 참조 동기화(goto/옵션/분기/주사위 행선지), 변경된 노드 반환; 새 id는 `[A-Za-z0-9_-]+`로 제한, 기존 노드와 충돌→ValueError
  - `move_node(story, node_id, delta)`: 상대 이동량으로 노드 순서 조정
  - `set_start(story, node_id)`: 시작 노드 설정
  - `add_choice(story, options, after=None)`: 옵션 분기 추가(2~4개, dialog는 Options 고정)
  - `add_dice(story, check, goto_成功, goto_失败, goto_大成功="", band_texts=None, after=None)`: 주사위 판정 추가(check는 반드시 공식 메타데이터 필요, 결과대 수에 따라 goto 검증; band_texts 개수는 결과대 수와 같아야 하고 각 항목은 비어 있으면 안 됨)
  - `add_say(story, text, character=None, mode="character", portrait="normal", voice=None, after=None)`: 대사 추가(character 모드는 character 필수; narrative/center는 character를 쓰지 않음; voice는 선택적 user: 오디오 참조)
  - `add_death(story, text, death_id, next="Title", title=None, after=None)`: 사망 텍스트 노드 추가(text 필수, 비어 있으면 안 되고 여러 줄 가능; death_id 필수, ≥900000의 mod 전용 숫자 id; next는 Title만 허용; title은 선택적 짧은 제목, 기본값/빈 문자열은 「勝敗乃兵家常事」 사용)
  - `add_scene(story, view, after=None)`: 장면 전환 추가
  - `check_story(story)`: 검증만, (errors: list[str], warnings: list[str]) 반환
  - `compile_story(story)`: 검증+컴파일, (lua|None, errors, warnings) 반환, 실패 시 lua는 None
  - `load_story_json(path)` / `save_story_json(story, path)`: story.json 읽기/쓰기(UTF-8)
  - `pack_mod(mod_dir, output=None)`: manifest 검증 + 전체 컴파일 + .lommod 패키징, 산출물 경로 반환
- CLI: python editor/story_api.py check|compile|pack|new-story(AI 서브프로세스 친화, 종료 코드 0/1, 중국어 오류)
- 핵심 불변량(컴파일러 강제, API 투과): choice.dialog는 Options뿐; dice.check는 반드시 공식 메타데이터 필요
  (주사위 범위+결과대); transition in/out 쌍; scene은 배경 자동 미리 로드;
  **show/say의 (character, portrait)는 반드시 data/editor_data.json의 캐릭터 표정표 안에 있어야 합니다**
  (표 사용 불가/캐릭터가 표에 없음 → 통과; 캐릭터가 표에 있지만 표정이 그 목록에 없음 → LomcError/ValueError——
  게임의 LoadCharacterPortrait가 무효한 표정 key에 KeyNotFoundException을 던짐 → Lua 코루틴 사망 → 대화 동결).
  say/show가 참조하는 인물은 반드시 먼저 show로 무대에 올라야 합니다(무대에 오르지 않아도 마찬가지로 KeyNotFoundException),
  쓰기 진입구의 등장 방어선이 자동으로 show를 보충합니다(add_node/update_node 참조). 에디터 건강 검사는 다중 경로 합류에 그래프 수준 폴백을 합니다.

## 8. 사용자 콘텐츠(User Content, v1은 오디오만)

개발 환경 저장소는 `%APPDATA%/lom_modkit/repository/`에 있으며, 런타임 의존성이 **아닙니다**. 스토리는 안정적인 참조만 저장합니다:

```text
user:<namespace>.<content_id>     例如 user:mohui.boss_theme
```

공식 ID(`普通_001`, `brother4`)는 그대로 유지하고 `official:`로 바꾸지 않습니다.

패키지 내 구조(실제 참조만 패키징):

```text
assets/user/audio/mohui.boss_theme/content.json
assets/user/audio/mohui.boss_theme/boss_theme.ogg
```

`content.json` schema 1:

```json
{
  "schema": 1,
  "content_schema": 1,
  "id": "mohui.boss_theme",
  "type": "audio",
  "name": "决战曲",
  "audio_kind": "music",
  "files": { "main": "boss_theme.ogg" }
}
```

대사 음성도 `type=audio`입니다. 선택적 관리 필드 `character`(`user:mohui.luoxue` 또는 공식 인물 id, 예: `player`)를 추가할 수 있습니다. 이 필드가 없는 기존 오디오도 그대로 유효합니다. `character`는 `say.voice` 재생 규약을 바꾸지 않으며, 미참조 오디오를 패키지에 넣지 않습니다.

커스텀 캐릭터 `content.json`에는 선택적으로 `title`(대사 짧은 칭호), `scale`(체형 50–130, 기본 100, 발끝 기준), `art_facing`(원본 그림 방향 `left` 기본 / `right`)을 쓸 수 있습니다. 생략과 구패키지는 100 / 왼쪽입니다.

- `type`: `audio` / `character`.
- `audio_kind`: `music` / `sound` / `env`.
- `character`(오디오만, 선택): 사용자 캐릭터 참조 또는 공식 인물 id. 생략하면 나레이션/시스템/미연결.
- 콘텐츠 ID: `[a-z][a-z0-9_]{0,31}.[a-z0-9][a-z0-9_]{0,47}`, `..`, `/`, `\`, `:` 금지.
- 누락, 타입 불일치, metadata 손상, 파일 없음, 확장자 미지원, 20MB 초과: pack이 바로 실패하며 silently skip하지 않습니다.
- Python 측 유일 해석 진입구: `compiler/lomc/content.py`. C# 측 규약 구현: `ContentRef.cs` + `ModLoader`.

사용 설명은 `user_content.md` 참조.
