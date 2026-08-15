# AI 에이전트 CLI / Python API 매뉴얼(story_api)

> 언어: [简体中文](../zh_CN/ai_cli.md) · [繁體中文](../zh_TW/ai_cli.md) · [日本語](../ja/ai_cli.md) · 한국어(본문)

`editor/story_api.py`는 AI 에이전트와 스크립트가 호출하는 스토리 데이터 인터페이스입니다: 통제된 쓰기 작업(Python API)
+ argparse 명령줄(check / compile / pack / new-story). 이 문서는 **서브프로세스 방식으로
CLI를 호출하거나 직접 import하는 AI 에이전트**를 대상으로 하며, 예시는 모두 저장소 실제 환경에서 실행해 통과했습니다(Windows + Python
3.10, editor/.venv).

형식 규약(노드 타입, 패키지 구조, 런타임 동작)은 `mod_format.md`를 참조하며, 그 §7이
story_api의 규약 조항입니다; 이 문서는 그 작동 매뉴얼이며, 둘이 충돌하면 규약을 기준으로 합니다.

핵심 규칙(규약 §7): **AI는 story JSON이나 Lua를 직접 손으로 작성하지 않습니다**. 모든 스토리 구축은
story_api를 거칩니다 — 노드는 models 규약 기본값으로 생성하고, 필드는 NODE_SCHEMAS로 검증하고, 알 수 없는 필드는
일률적으로 거부하며, 컴파일 시점의 남은 문제는 lomc 검증이 폴백합니다. 에디터와 AI가 같은 방어선을 공유합니다.

인터페이스는 `models.NODE_SCHEMAS`의 현재 60종만 받습니다. Gameplay 노드는 검증된 원작 API 또는 기존 원자 API만 조합합니다. `mod_quest`는 비영속 Host 세션이며 원작 Mission으로 취급하지 않습니다.

## 1. 환경 요구 사항과 호출 방식

- Python 3.10+(저장소 자체 venv: `editor/.venv`). story_api는 표준 라이브러리 +
  `editor/models.py` + `compiler/lomc`에만 의존하며, **PySide6에 의존하지 않아** 헤드리스 환경에서 사용할 수 있습니다.
- 저장소 내 두 가지 리소스에 의존하며 하나도 빠뜨릴 수 없습니다(저장소에 포함, 정상적으로 클론하면 있음):
  - `compiler/lomc/`(컴파일러; 사용할 수 없으면 check/compile/pack/add_dice가
    「lomc 编译器不可用」(lomc 컴파일러를 사용할 수 없음) 보고)
  - `data/editor_data.json`(인물/표정/장면/주사위 체크포인트 등 공식 목록)

### 1.1 소스 상태(개발/AI 서브프로세스)

```bash
cd editor
.venv/Scripts/python story_api.py <子命令> [参数]
```

스크립트 내부가 `editor/`와 `<저장소 루트>/compiler`를 `sys.path`에 삽입하며, **현재 작업
디렉터리와 무관**합니다 — 저장소 루트에서 실행해도 마찬가지로 성립합니다:

```bash
editor/.venv/Scripts/python editor/story_api.py check --json samples/demo_mod/story/main.json
# {"ok": true, "errors": [], "warnings": []}
```

경로 유도: `EDITOR_DIR = story_api.py가 있는 디렉터리`, `PROJECT_ROOT = 그 상위`,
editor_data는 `<저장소 루트>/data/editor_data.json`을 읽습니다.

### 1.2 동결 상태(PyInstaller 패키징 exe)

`editor/build_exe.py`가 onedir 이중 진입 패키지를 산출하며, 그중 `story_api_cli.exe`가 이 CLI이고
대상 컴퓨터에 Python이 필요 없습니다:

```bash
editor/dist/lom_modkit/story_api_cli.exe check story.json --json
```

소스 상태와의 차이는 경로 유도뿐입니다: `__file__`이 압축 해제 디렉터리를 가리키고, 프로젝트 루트는 `_MEIPASS`
(`dist/lom_modkit/_internal`)로 바뀌며, `data/editor_data.json`과 lomc는 패키징 spec이
패키지에 넣습니다. **하위 명령, 매개변수, 출력 형식, 종료 코드는 소스 상태와 완전히 일치합니다**.

### 1.3 공통 약정

- **종료 코드**: `0` 성공; `1` 검증/컴파일/패키징/IO 실패; `2` argparse 사용법 오류
  (매개변수 누락, 알 수 없는 옵션, usage를 stderr에 출력).
- **텍스트 모드**(기본): 결과 경로를 stdout에 출력; `警告：...`와 `错误：...`는 일률적으로
  **stderr**에 출력. check가 모두 통과하면 **아무 출력도 없습니다**, 조용하면 성공입니다.
- **--json 모드**: stdout에 **한 줄** 구조화 JSON 출력(UTF-8 직접 바이트 스트림 기록, Windows
  콘솔 인코딩 우회), stderr는 깨끗하게 유지. `--json`은 하위 명령 앞이나 뒤에 둬도 유효합니다:

```bash
.venv/Scripts/python story_api.py check --json story.json   # 子命令后
.venv/Scripts/python story_api.py --json check story.json   # 子命令前
```

- 프로그램 진입부가 이미 stdout/stderr를 UTF-8로 reconfigure했습니다; AI가 서브프로세스로 호출할 때 UTF-8로
  디코딩하면 됩니다.

## 2. 하위 명령 상세

예시는 `editor/` 디렉터리에서 `.venv/Scripts/python story_api.py`로 실행합니다; 임시 디렉터리
`C:/Users/mohui666/AppData/Local/Temp/lom_cli_test`는 `<TMP>`로 줄입니다.

### 2.1 check — story.json 검증

```
usage: story_api check [-h] [--json] story_json
```

| 매개변수 | 설명 |
| --- | --- |
| `story_json` | story.json 경로(위치 매개변수, 필수) |
| `--json` | 한 줄 JSON 출력 |

동작: 스토리를 읽고 검증합니다. errors가 비어 있지 않음 → 종료 코드 1; 아니면 0. 오류와 경고는 모두 완전한 중국어
문장이며; 오류에는 `story.json: ` 출처 접두사가 붙습니다(이 접두사는 "story.json"으로 고정, 실제 파일명과
무관), 경고에는 접두사가 없습니다.

```bash
# 全部通过：文本模式无任何输出，exit=0
.venv/Scripts/python story_api.py check ../samples/demo_mod/story/main.json
.venv/Scripts/python story_api.py check --json ../samples/demo_mod/story/main.json
# {"ok": true, "errors": [], "warnings": []}

# 有警告（非致命，exit 仍为 0）——transition phase=in 之后没有 out
.venv/Scripts/python story_api.py check --json <TMP>/warn.json
# {"ok": true, "errors": [], "warnings": ["节点 \"n2\"(transition, phase=in) 之后没有 phase=out 解除：TransitionIn 会隐藏剧情 UI 并盖满黑幕……请在其后补一个 phase=out 节点，或改用 scene 节点做转场。"]}

# 有错误（悬空 goto），exit=1
.venv/Scripts/python story_api.py check --json <TMP>/bad.json
# {"ok": false, "errors": ["story.json: 节点 \"n1\": goto 指向不存在的节点 \"not_exist\""], "warnings": []}

# 文件不存在，exit=1
.venv/Scripts/python story_api.py check <TMP>/nope.json
# stderr：错误：story.json 读取失败: [Errno 2] No such file or directory: '...nope.json'
```

> 주의: 막 `new-story`로 만든 스토리를 바로 check하면 **통과하지 않습니다** — 오프닝이 show 등장 +
> 빈 say이고, say가 마지막 노드이면서 명시적 goto가 없기 때문입니다. 이것은 정상 현상이며, end 노드를 하나 보충하면 됩니다
> (§3.4 작업 흐름 참조):
> `{"ok": false, "errors": ["story.json: 节点 \"n2\"(say): 是最后一个节点且没有显式 goto，脚本无法正常结束（请改用 end/goto_scene/raw 节点或显式 goto）"], "warnings": []}`

### 2.2 compile — story.json → Lua 컴파일

```
usage: story_api compile [-h] [-o OUTPUT] [--json] story_json
```

| 매개변수 | 설명 |
| --- | --- |
| `story_json` | story.json 경로(위치 매개변수, 필수) |
| `-o, --output` | 출력 .lua 경로; 기본은 입력과 같은 디렉터리, 같은 이름 `.lua` |
| `--json` | 한 줄 JSON 출력 |

```bash
# 成功：文本模式 stdout 打印产物路径
.venv/Scripts/python story_api.py compile <TMP>/ok.json
# C:\...\lom_cli_test\ok.lua    exit=0

.venv/Scripts/python story_api.py compile <TMP>/ok.json -o <TMP>/out2.lua --json
# {"ok": true, "output": "C:\\...\\out2.lua", "warnings": []}

# 带警告编译（仍成功；Lua 头部同时嵌 `-- lomc 警告：` 注释）
.venv/Scripts/python story_api.py compile <TMP>/warn.json -o <TMP>/warn.lua --json
# {"ok": true, "output": "...\\warn.lua", "warnings": ["节点 \"n2\"(transition, phase=in) 之后没有 phase=out 解除：..."]}

# 失败（不写文件；注意失败时 JSON 只有 ok/errors 两个键，没有 warnings 键）
.venv/Scripts/python story_api.py compile <TMP>/bad.json --json
# {"ok": false, "errors": ["story.json: 节点 \"n1\": goto 指向不存在的节点 \"not_exist\""]}    exit=1
```

산출물 앞부분 예시(`-- Generated by lomc, do not edit`로 시작):

```lua
-- Generated by lomc, do not edit
-- Source: story/my_tale.json (id=my_tale, title=测试剧情)

-- mod 内剧情 flag 表（不存档，重进游戏清零）
modflags = modflags or {}
mod_set_mood(false)
```

### 2.3 pack — mod 디렉터리 → .lommod 패키징

```
usage: story_api pack [-h] [-o OUTPUT] [--json] mod_dir
```

| 매개변수 | 설명 |
| --- | --- |
| `mod_dir` | mod 디렉터리(`manifest.json`과 `story/` 하위 디렉터리 포함, 규약 §1/§2) |
| `-o, --output` | 출력 .lommod 경로; 기본은 `<mod 디렉터리>`와 같은 단계, 같은 이름 `<디렉터리명>.lommod` |
| `--json` | 한 줄 JSON 출력 |

패키징 전에 manifest를 검증하고 story/ 아래의 모든 스크립트를 하나씩 검증+컴파일합니다(파일명은 반드시 내부
id와 일치해야 함), 실패하면 전체가 실패합니다. 산출물 zip에는 `manifest.json`, `story/<id>.json`,
`lua/<id>.lua`, `texts.json`(읽음 텍스트 테이블)이 들어 있습니다.

```bash
.venv/Scripts/python story_api.py pack <TMP>/my_mod -o <TMP>/my_mod_v2.lommod --json
# {"ok": true, "output": "C:/Users/mohui666/AppData/Local/Temp/lom_cli_test/my_mod_v2.lommod"}

# 失败样例
.venv/Scripts/python story_api.py pack <TMP>/no_manifest --json
# {"ok": false, "errors": ["mod 目录缺少 manifest.json: C:\\...\\no_manifest"]}    exit=1
```

> `samples/`의 예제 mod에 pack을 시험할 때는 **반드시 `-o`로 다른 곳을 지정하세요**: 기본 출력은
> `<mod 디렉터리>.lommod`(예: `samples/demo_mod.lommod`)로, 저장소에 이미 있는 산출물을 덮어씁니다.

### 2.4 new-story — 새 스토리 스크립트 story.json 작성

```
usage: story_api new-story [-h] [--title TITLE] -o OUTPUT [--json] story_id
```

| 매개변수 | 설명 |
| --- | --- |
| `story_id` | 스토리 스크립트 id, 규칙 `[a-zA-Z0-9_-]+`(위치 매개변수, 필수) |
| `--title` | 제목, 기본값 「新剧情」 |
| `-o, --output` | 출력 story.json 경로(**필수**) |
| `--json` | 한 줄 JSON 출력 |

```bash
.venv/Scripts/python story_api.py new-story my_tale --title "测试剧情" -o <TMP>/story2.json --json
# {"ok": true, "output": "C:\\...\\story2.json"}

.venv/Scripts/python story_api.py new-story "坏id!" -o <TMP>/bad.json --json
# {"ok": false, "errors": ["剧情脚本 id 非法: '坏id!'（规则 [a-zA-Z0-9_-]+）"]}    exit=1

.venv/Scripts/python story_api.py new-story my_tale
# story_api new-story: error: the following arguments are required: -o/--output    exit=2
```

생성된 파일(UTF-8, 들여쓰기 2, 중국어 유지): 오프닝은 고정으로 **show 등장 + 빈 say** 두
노드(등장 후 동작, §4 규칙 4 참조), `mood=false`(매번 show/say 전후에 자동으로
`mod_hide_mood()`를 방출해 공식 기분 버블 숨김), 인물 필드는 기본으로 editor_data의 첫 번째 인물을 사용합니다:

```json
{
  "id": "my_tale",
  "title": "测试剧情",
  "mood": false,
  "start": "n1",
  "nodes": [
    {
      "id": "n1",
      "type": "show",
      "character": "artist1",
      "position": "M",
      "portrait": "normal",
      "facing": "right",
      "fadeDuration": 0,
      "moveDuration": 0
    },
    {
      "id": "n2",
      "type": "say",
      "text": "",
      "character": "artist1",
      "portrait": "normal",
      "mode": "character"
    }
  ]
}
```

### 2.5 --json 필드 구조 총정리

| 하위 명령 | 성공(exit 0) | 실패(exit 1) |
| --- | --- | --- |
| check | `{"ok": true, "errors": [], "warnings": [...]}` | `{"ok": false, "errors": [...], "warnings": []}` |
| compile | `{"ok": true, "output": "<lua 경로>", "warnings": [...]}` | `{"ok": false, "errors": [...]}`(**warnings 키 없음**) |
| pack | `{"ok": true, "output": "<lommod 경로>"}` | `{"ok": false, "errors": [...]}` |
| new-story | `{"ok": true, "output": "<story.json 경로>"}` | `{"ok": false, "errors": [...]}` |

- `ok`: bool, 유일하게 항상 있는 키; `errors`/`warnings`: 문자열 배열, 요소는 완전한 중국어
  문장; `output`: 문자열 경로(Windows에서는 JSON 내 역슬래시가 `\\`로 이스케이프, pack에서 명시적으로
  `-o`를 주면 전달한 형태 그대로 반환).
- check는 실패 시에도 `warnings` 키를 포함하는 유일한 하위 명령; 나머지는 실패 시 일률적으로 `ok`/`errors`만 있습니다.
- 여러 오류가 있으면 `errors`를 줄 단위로 분할(lomc의 여러 줄 메시지를 여러 항목으로 분할).

## 3. Python API 요약

```python
import sys
sys.path.insert(0, r"<仓库根>/editor")   # 任意 cwd 均可，story_api 内部自理 compiler 路径
import story_api
```

모든 함수의 오류 메시지는 전부 중국어; **`ValueError`만 던집니다**(pack_mod 내부가 lomc.LomcError를
ValueError로 변환), 검증/컴파일류 함수는 예외를 던지지 않고 오류 목록을 반환합니다.

### 3.1 데이터와 환경

| 함수 | 설명 |
| --- | --- |
| `load_editor_data() -> (dict, bool)` | `data/editor_data.json` 읽기, (데이터, 폴백 여부) 반환. 매번 디스크에서 다시 읽음; 폴백=true는 파일 누락/손상으로 내장 폴백 목록을 사용했음을 의미 |

### 3.2 스토리와 노드 읽기/쓰기(쓰기 작업은 모두 고정 규칙으로 검증)

| 함수 | 핵심 제약 |
| --- | --- |
| `new_story(story_id="main", title="新剧情", mood=False) -> dict` | story_id는 `[a-zA-Z0-9_-]+` 매칭 필요; title은 str 필요; mood는 bool 필요. show 등장(n1) + 빈 say(n2) 오프닝의 스토리 dict 반환(등장 후 동작, §4 규칙 4 참조) |
| `get_node(story, node_id) -> dict` | 없음 → ValueError. 반환값은 story 내의 **원본 객체**(update에 따라 유효) |
| `list_nodes(story) -> list[dict]` | 각 항목 `{"id", "type", "summary"}`, summary는 중국어 요약(예: `对白·唐惟元: 师弟，你来了。`) |
| `add_node(story, node_type, fields=None, after=None) -> dict` | node_type은 60종으로 제한(`models.NODE_TYPES`); fields 키는 NODE_SCHEMAS 합법 필드+공통 필드(id/type/goto)로 제한, 타입은 kind에 따라 느슨하게 검증; 알 수 없는 타입/필드/타입 불일치 → ValueError. id 자동 생성(say1, show2, choice1…), after=노드 id면 그 뒤에 삽입, None이면 끝에 추가. **등장 방어선**: 동작류 노드의 대상 인물이 앞에서 미등장/퇴장 상태이면 그 앞에 자동으로 show 노드를 하나 삽입(§4 규칙 4 참조) |
| `update_node(story, node_id, fields) -> dict` | add_node와 같은 필드 검증; 노드 없음 → ValueError. 병합 후 branch 정규화와 표정 검증 수행. **등장 방어선**: 업데이트 후 동작 인물이 미등장/퇴장 상태이면 해당 노드 앞에 자동으로 show를 삽입하고, 그곳을 가리키는 goto/옵션/분기 점프를 새 노드로 변경(§4 규칙 4 참조) |
| `delete_node(story, node_id) -> dict` | 삭제된 노드 반환; **끊어진 goto는 차단하지 않고** check_story가 보고 |
| `rename_node(story, node_id, new_id) -> dict` | 노드 id 이름 변경 및 start와 모든 점프 참조 동기화(goto / choice 옵션 / branch cases / dice 행선지), 변경된 노드 반환. 새 id는 `[A-Za-z0-9_-]+`로 제한(앞뒤 공백 제거); old==new는 무작업; 번호 점유 또는 원래 노드 없음 → ValueError |
| `move_node(story, node_id, delta) -> dict` | delta는 ±1만 가능; 범위 초과(이미 처음/끝) → ValueError |
| `set_start(story, node_id) -> dict` | story["start"] 설정; 노드 없음 → ValueError |
| `add_say(story, text, character=None, mode="character", portrait="normal", voice=None, after=None) -> dict` | mode ∈ character/think/narrative/center; character/think 모드는 character 필수(인물 id), narrative/center는 character 필드를 쓰지 않음; text는 줄바꿈 가능; (character, portrait)는 공식 표정표로 검증; voice는 선택적 user: 오디오 참조 |
| `add_scene(story, view, after=None) -> dict` | view는 비어 있지 않은 장면 id 문자열 |
| `add_choice(story, options, after=None) -> dict` | options는 [(text, goto), ...] 2~4개, text는 비어 있지 않은 str, goto는 노드 id str; dialog는 "Options"로 강제 기록(§4 규칙 1 참조) |
| `add_dice(story, check, goto_成功, goto_失败, goto_大成功="", band_texts=None, after=None) -> dict` | check는 반드시 공식 메타데이터 적중 필요(`lomc.dice_data.get_dice_meta`); 2대 체크포인트는 goto_成功/goto_失败 필수, goto_大成功 비워 둘 수 있음; ≥3대는 세 개 모두 필수; band_texts는 선택적 대별 옵션 텍스트 덮어쓰기, 개수=결과대 수, 각 항목 비어 있으면 안 됨 |
| `add_death(story, text, death_id, next="Title", title=None, after=None) -> dict` | text 비어 있으면 안 됨(여러 줄 가능); death_id는 ≥900000의 숫자 문자열(약정 9+공식 id); next는 "Title"만 허용; title은 선택, 기본값/빈 문자열이면 필드를 쓰지 않음(codegen은 「勝敗乃兵家常事」 사용) |

fields 타입 검증 약정(`_check_kind`): int/float 필드는 수치를 받지만 **bool은 거부**
(`True`를 float 필드에 주면 거부됨); bool 필드는 bool만 받음; options/cases/vars/
dice_options 필드는 list를 받음; 나머지는 일률적으로 str. 값을 수정하지 않고 검증만 합니다.

### 3.3 검증 / 컴파일 / 패키징 / 파일

| 함수 | 반환 약정 |
| --- | --- |
| `check_story(story) -> (errors, warnings)` | 두 개의 문자열 목록; errors가 비어 있지 않으면 실패. errors에는 `story.json: ` 접두사, warnings에는 접두사 없음 |
| `compile_story(story) -> (lua \| None, errors, warnings)` | 실패 `(None, errors, [])`; 성공 `(lua 소스, [], warnings)`. lua 앞부분에 이미 `-- lomc 警告：` 주석 포함 |
| `load_story_json(path) -> dict` | story.json 읽기; 읽기 실패/구조 불법 → ValueError |
| `save_story_json(story, path) -> None` | UTF-8, 들여쓰기 2, 중국어 유지로 쓰고 끝에 줄바꿈 포함 |
| `pack_mod(mod_dir, output=None) -> str` | manifest 검증 + 전체 컴파일 + zip 패키징, .lommod 경로 반환; 실패 → ValueError |

### 3.4 전형적인 작업 흐름(Python)

새로 작성 → 시작 노드 채우기 → 노드 추가 → check → compile → pack:

```python
import story_api

# 1. 新建（开场是 show 登场 + 空 say；先填 say 文本，默认人物已在 n1 登场）
story = story_api.new_story("my_tale", "测试剧情")
say_id = story["nodes"][1]["id"]
story_api.update_node(story, say_id, {"text": "山门前，风很大。"})

# 2. 加节点（id 自动分配；choice/dice 的 goto 用节点 id 字符串）
story_api.add_scene(story, "center")
story_api.add_say(story, "师弟，你来了。", character="brother4")
#   ↑ brother4 此前未登场：登场防线自动在它前面插一个 show·唐惟元（§4 规则 4）
story_api.add_choice(story, [("迎上去", say_id), ("转身离开", say_id)])
story_api.add_node(story, "end")          # 别忘收尾，否则 check 报「无法正常结束」

# 3. 校验
errors, warnings = story_api.check_story(story)
if errors:
    raise SystemExit("\n".join(errors))

# 4. 编译 + 存档（可选）
lua, errors, warnings = story_api.compile_story(story)
story_api.save_story_json(story, "my_tale.json")

# 5. 打包（mod 目录需含 manifest.json 与 story/<id>.json，文件名=内部 id）
out = story_api.pack_mod("path/to/my_mod")
```

list_nodes 출력 예시(인물/장면 표시명은 editor_data 목록에서 가져옴, artist1=무사,
brother4=당유원, center=연무장_낮; n5는 등장 방어선이 자동으로 보충한 show로 n4 앞에 삽입됨):

```python
[{'id': 'n1', 'type': 'show',   'summary': '人物登场·武师@M'},
 {'id': 'n2', 'type': 'say',    'summary': '对白·武师: 山门前，风很大。'},
 {'id': 'n3', 'type': 'scene',  'summary': '切换背景·校場_白天'},
 {'id': 'n5', 'type': 'show',   'summary': '人物登场·唐惟元@M'},
 {'id': 'n4', 'type': 'say',    'summary': '对白·唐惟元: 师弟，你来了。'},
 {'id': 'n6', 'type': 'choice', 'summary': '选项分支·2个选项'},
 {'id': 'n7', 'type': 'end',    'summary': '结束剧情·结束'}]
```

동등한 CLI 체인은 §2의 네 하위 명령을 순서대로 호출하는 것입니다(new-story → check → compile → pack).

## 4. 쓰기 작업의 엄격한 규칙(망가뜨림 방지)

이것들은 게임 측의 알려진 크래시 함정을 쓰기 진입구에서 막는 규칙으로, **우회를 시도하지 마세요**(우회해도
check_story/compile_story에 걸립니다). 각 규칙의 게임 측 메커니즘은 규약 `mod_format.md`
§3/§4를 참조하세요.

1. **choice 스킨은 Options로 고정**. `add_choice`는 `dialog="Options"`를 강제합니다; 다른 스킨은
   자유 장면의 break 형식 메뉴로, 순수 텍스트 옵션은 게임 내 메뉴를 동결시킵니다. 설령
   `update_node`로 다른 스킨으로 바꿔도 check_story가 오류를 냅니다.
2. **dice 체크포인트는 반드시 공식 메타데이터에 적중해야 합니다**. `add_dice`의 check는 반드시
   `data/editor_data.json`의 dice_meta 표 안에 있어야 합니다(`load_editor_data()`로 확인 가능,
   테스트 케이스: `S0205_01_001`은 2대, `Ch_6_8_2_Break_01_001`은 3대),
   메타데이터가 없는 체크포인트는 게임 내 주사위 메뉴를 크래시시킵니다. 결과대 수가 goto 필수 항목을 결정합니다:
   2대는 goto_成功/goto_失败 입력; ≥3대는 goto_大成功도 필수. band_texts 덮어쓰기
   개수는 반드시 결과대 수와 같아야 합니다.
3. **say 모드와 인물 연동**. mode=character/think는 반드시 character(인물 id)를 줘야 합니다;
   narrative/center는 character 필드를 쓰지 않습니다(줘도 제거됨).
4. **동작 인물은 반드시 먼저 등장해야 합니다(등장 방어선)**. 무대에 없는 인물에게 동작(say 대사/독백,
   move/face/hide/focus/offset/shock/dim/rotate)을 시키면 게임이 스토리 코루틴을 깨 검은 화면이 됩니다.
   `add_node`/`add_say`/`update_node`는 쓸 때 해당 인물이 앞에서 이미 등장했고
   퇴장하지 않았는지 선형으로 검사하고, 부족하면 해당 노드 앞에 자동으로 `show` 노드를 하나 삽입합니다(update_node는 또한
   그곳을 가리키는 goto/옵션/분기 점프를 새 show로 변경). 다중 경로 합류 등 복잡한 경우는 에디터
   「건강 검사」가 그래프 수준 분석으로 폴백합니다. 이렇게 자동 보충된 show를 손으로 삭제하지 마세요.
5. **(character, portrait)는 반드시 공식 캐릭터 표정표 안에 있어야 합니다**. show/say 노드의 캐릭터가 표에
   있고 표정이 그 목록에 없음 → ValueError; 캐릭터가 표에 없음(자작 캐릭터) → 통과. 표정표를 사용할 수 없음
   (lomc 누락)이면 검증이 check_story로 하강합니다.
6. **알 수 없는 필드는 일률적으로 거부**. fields는 NODE_SCHEMAS가 선언한 필드 + 공통 필드
   (id/type/goto)만 허용하며, 키가 하나라도 많으면 ValueError를 내고 허용 집합을 나열합니다; 타입은 kind로 검증
   (수치 필드는 bool 거부, bool 필드는 수치 거부, 목록 필드는 문자열 거부, 그 반대도 마찬가지).
7. **death_id는 반드시 ≥900000**(약정 9+공식 id, 예: 공식 10021 → 910021; 공식 id
   는 공식 엔딩 잠금 해제와 기록을 발동해 세이브를 오염시킴). death의 next는 "Title"만 허용합니다.
8. **branch 키 필드 정규화**. source=stat이면 stat 필드를 쓰고 flag를 지움; 나머지 출처
   (mod/game/flag_value/condition)는 flag를 쓰고 stat을 지움. add_node/update_node가
   자동으로 처리하므로 호출 측이 신경 쓸 필요는 없지만, 두 키를 손으로 함께 쓰지 마세요.
9. **삭제/이동은 그래프 완전성을 보장하지 않습니다**. delete_node가 만드는 끊어진 goto, 마지막 노드에
   goto 없음 등의 문제는 쓰기 진입구에서 차단하지 않고 일률적으로 check_story가 보고합니다 — **수정 후 반드시 check하세요**.

## 5. 흔한 오류 메시지 대조표

아래 메시지는 모두 실제 실행에서 채집했습니다(Python API는 ValueError를 던짐; CLI 텍스트 모드는
`错误：`/`警告：` 접두사를 붙여 stderr에 출력).

| 오류 메시지(예시) | 출처 | 원인과 처리 |
| --- | --- | --- |
| `未知节点类型: no_such_type（支持 60 种，见 models.NODE_TYPES）` | add_node | 타입명 철자 오류; `models.NODE_TYPES` 또는 규약 §3.1의 60종 사용 |
| `节点类型 wait 不支持字段: bogus（允许: goto, id, seconds, type）` | add_node/update_node | 필드명이 타입 표에 없음; 메시지의 허용 집합에 따라 수정 |
| `节点类型 wait 字段 "seconds" 类型不符（kind=float，应为 数值），实际为 'abc'` | add_node/update_node | 필드 타입 오류; `True`도 수치 필드에 거부됨에 주의 |
| `通用字段 "goto" 必须是字符串` | add_node/update_node | goto/id/type은 문자열만 받음 |
| `after 指定的节点不存在: no_such` | add_* 계열 | after는 반드시 기존 노드 id 또는 None |
| `节点不存在: no_such` | get/update/delete/move/rename/set_start | node_id 철자 오류 또는 이미 삭제됨; 먼저 list_nodes로 확인 |
| `节点编号已被占用: n5` / `节点编号只使用英文字母、数字、下划线或短横线` | rename_node | 새 id가 기존 노드와 충돌하거나 불법 문자 포함 |
| `delta 只能是 ±1，实际为 2` / `节点 n1 已在开头，无法再移动` | move_node | 한 칸씩 이동만 지원; 범위 초과 이동 |
| `choice 选项必须是 2~4 项，实际 1 项` / `第 1 个选项必须是 (text, goto) 二元组` | add_choice | 옵션 수 2~4; 각 항목은 (text, goto) 이원조 |
| `骰子检查点 "X" 无官方元数据，请在编辑器清单里选择...` | add_dice | check가 dice_meta 표에 없음; `load_editor_data()["dice_meta"]`에서 합법 체크포인트 선택 |
| `检查点 "Ch_6_8_2_Break_01_001" 有 3 个结果带，goto_大成功 必填（最优带）` | add_dice | ≥3대 체크포인트는 대성공 점프 필수 |
| `dice band_texts 条数必须等于检查点 "X" 的结果带数（N 条），实际为 M 条` | add_dice | 덮어쓰기 텍스트 개수와 결과대 수 불일치 |
| `mode="character" 时 character 必填（人物 id）` | add_say | character/think 모드는 인물 id 필수 |
| `say 模式非法: 'shout'（允许 character/think/narrative/center）` | add_say | mode 철자 오류 |
| `角色 "brother4" 没有表情 "angry3"（该角色表情：angry1、angry2、…、shock）。…KeyNotFoundException…` | add_say/add_node/update_node | 표정 id가 해당 캐릭터 목록에 없음; 메시지에 나열된 합법 표정으로 수정 |
| `death_id 必须是 ≥900000 的 mod 专属数字 id…实际为 '10021'` | add_death | 공식 id 사용; 9+공식 id(910021)로 변경 |
| `death next 非法: 'Free'（原版死亡画面固定返回标题，只允许 Title）` | add_death | next는 Title만 허용 |
| `剧情脚本 id 非法: '坏id!'（规则 [a-zA-Z0-9_-]+）` | new_story / CLI new-story | id에 불법 문자 포함 |
| `story.json: 节点 "n1": goto 指向不存在的节点 "not_exist"` | check/compile | 끊어진 goto; goto를 실제 노드로 향하게 하거나 노드를 삭제한 후 다시 연결 |
| `story.json: 节点 "n1"(say): 是最后一个节点且没有显式 goto，脚本无法正常结束…` | check/compile | 스토리에 마무리가 없음; 끝에 end/goto_scene/raw 노드를 보충하거나 명시적 goto |
| `story.json: 节点 "n2"(choice): dialog 只支持 "Options"。…BreakOptionButton 解析崩溃…` | check/compile | choice 스킨이 Options가 아닌 것으로 변경됨; Options로 되돌림 |
| `story.json: 节点 "n2"(branch): source="stat" 时必填字段 "stat"…` | check/compile | branch stat 출처에 stat 필드 누락 |
| `节点 "n2"(transition, phase=in) 之后没有 phase=out 解除…黑幕将一直覆盖到脚本结尾…` | check/compile(**경고**, exit 0) | transition in/out은 쌍이어야 함; out을 보충하거나 scene 전환으로 변경 |
| `mod 目录缺少 manifest.json: …` / `mod 目录缺少 story/ 子目录: …` | pack | 규약 §1/§2에 따라 디렉터리 구조 보완 |
| `story/xx.json: 文件名与内部 id 不一致…` | pack | story 파일명은 반드시 내부 id와 일치(my_tale.json ↔ "id": "my_tale") |
| `story.json 读取失败: [Errno 2] No such file or directory…` | CLI 각 하위 명령 | 경로 오류; Windows에서 exe에 전달하는 경로 구분자에 주의 |
| `story.json 不是合法 JSON: …` / `story.json 结构非法：缺少 nodes 数组` | load_story_json / CLI | 파일 손상 또는 스토리 스크립트가 아님; JSON을 손으로 쓰지 말고 API로 생성 |
| `lomc 编译器不可用（ImportError: …）。预期位置：…/compiler` | check/compile/pack/add_dice | compiler/ 디렉터리 누락 또는 이동됨; 저장소 구조 복구 |
| `usage: story_api ... error: the following arguments are required: -o/--output`(exit=2) | CLI | argparse 사용법 오류; usage에 따라 매개변수 보충 |

## 6. AI 에이전트를 위한 호출 권장 사항

- **--json 우선**: 한 줄, UTF-8, 구조 안정(먼저 `ok`를 읽고 위 표에 따라 키 사용), stderr
  깨끗; 텍스트 모드는 사람이 보기에 적합합니다.
- 각 서브프로세스 호출은 독립 프로세스이며 editor_data는 매번 디스크에서 다시 읽습니다 —
  `data/editor_data.json`을 수정한 후에도 아무것도 다시 시작할 필요가 없습니다.
- story dict는 Python 프로세스 내에서 재사용 가능한 객체입니다: 여러 번 add_/update_ 후 한 번에 check;
  그러나 프로세스 간에는 반드시 save_story_json / load_story_json으로 전달해야 합니다.
- 쓰기 작업 실패(ValueError) 시 스토리 객체가 **이미 부분적으로 수정되었을 수 있습니다**(예: update_node는 먼저
  필드를 병합한 후 표정 검증 수행), 대량 구축 시 먼저 작은 단위로 check하는 것을 권장합니다.
- 테스트 참조: `editor/tests/story_api_test.py`(GUI 의존 없음, 바로
  `.venv/Scripts/python tests/story_api_test.py`로 실행 가능)가 모든 공개 함수를 커버합니다.
