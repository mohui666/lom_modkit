# lom_modkit — 활협전(Legend of Mortal) Mod 도구

플레이어가 그래픽 에디터로 스토리를 직접 제작(게임 내 인물, 장면, 특수 효과, 음악, 수치 호출)하고,
`.lommod` 패키지로 보내 서로 공유할 수 있게 합니다. 게임 내에서는 BepInEx 플러그인이 연출을 로드하며,
"새 캠페인 시작"(새 게임 흐름을 인계받음)과 자유 모드 맵 지점 트리거를 지원합니다.

MIT 라이선스. 본 도구는 팬이 직접 제작한 도구로 게임 개발사와 무관하며, 게임 본체의 어떤 파일도 포함하지 않습니다.

> 언어: [简体中文](README.md) · [繁體中文](README.zh_TW.md) · [日本語](README.ja.md) · 한국어(본문)

## 구성 요소

- `compiler/`(`lomc`) — JSON 스토리 → 게임 네이티브 Lua 컴파일러(Python 표준 라이브러리, 패키지 형식 규약은 `docs/ko/mod_format.md` 참조)
- `editor/` — PySide6 그래픽 에디터(3단 구성: 스토리 구조 / 현재 객체 / 미리보기; 도구 모음에는 플레이 테스트·보내기만 유지; F5로 현재 단계부터 게임 진입, 흐름도, 건강 검사, 설치 관리, 읽음 초기화, 다중 챕터, 실행 취소/다시 실행)
- `editor/story_api.py` — AI/스크립트가 사용할 수 있는 통제된 도구 인터페이스(Python API + CLI): 모든 쓰기 작업은 고정 규칙으로 검증되며, AI가 story JSON/Lua를 직접 작성하지 않습니다
- `runtime/MortalModHost/` — BepInEx 게임 내 플러그인(C# net48): `.lommod` 스캔, Harmony 연출 가로채기, 캠페인 격리 세이브, 위치 트리거, 읽은 텍스트, 인물 소개 카드와 사망/엔딩 텍스트; Steam 일반 실행으로 사용 가능
- `tools/` — 언팩 산출물에서 에디터 데이터 / 미리보기 에셋 / 화면 캡처 보조 스크립트 추출
- `data/` — 에디터 데이터(`editor_data.json`: 인물/표정/장면/음악/속성/주사위 체크 포인트 목록, schema 3)
- `samples/` — 예제 mod(demo_mod, showcase, showcase2 전체 노드 시연 2.0, snack_case 《点心大盗疑案》, probe)

## 빠른 시작

### 1. 컴파일러(의존성 없음)

```bash
# 校验 / 编译 / 打包
PYTHONPATH=compiler python -m lomc check story.json
PYTHONPATH=compiler python -m lomc build story.json -o out.lua
PYTHONPATH=compiler python -m lomc pack  mod目录 -o 我的mod.lommod
```

### 2. 에디터(PySide6)

```bash
cd editor
python -m venv .venv
.venv/Scripts/pip install PySide6
run_editor.bat          # 或直接双击运行
```

### 3. 게임 내 플러그인(BepInEx)

1. 에디터 메뉴 "파일 → 설치 관리"에서 `Mortal.exe`가 있는 게임 폴더를 선택합니다.
2. "BepInEx 설치"를 클릭하면 에디터가 공식 다운로드 사이트에서 호환되는 BepInEx 6 Mono x86 build 692를 설치하고 검증합니다. 이후 런타임을 자동으로 설치하고 Steam 일반 실행 수정(`version.dll` + `ignore_disable_switch`)을 기록합니다. 그 후로 보낸 `.lommod`도 자동으로 복사되어 활성화됩니다.
3. Steam에서 「시작」을 눌러도 타이틀에 「活侠MOD」가 없거나 F8이 반응하지 않는 경우: 설치 관리에서 「Steam 로드 불가 복구」를 클릭한 다음 Steam에서 **일반 실행**(관리자 권한 아님)으로 시작합니다.
4. 같은 창에서 설치된 Mod를 체크로 활성화/비활성화할 수 있습니다. 수동 경로는 여전히 `BepInEx/plugins/MortalModHost/mods/`입니다.
5. 게임 진입: 자유 장면/타이틀 화면 왼쪽 아래의 「活侠MOD」 버튼 또는 F8로 메뉴를 열고 → 「mod 스토리 연출」 또는 「새 캠페인 시작」을 선택합니다.

읽음 상태가 다시 노랗게 변하는 경우: 먼저 게임을 종료한 다음 에디터의 「플레이 테스트 → 스토리 읽음 상태 초기화」를 실행합니다. 현재 mod와 F5 플레이 테스트 패키지(`lom_modkit_preview`)의 기록을 함께 지웁니다.

사용자 지정 음악과 효과음: 메뉴 「파일 → 사용자 콘텐츠 라이브러리」에서 `.ogg` / `.wav`(≤20MB)를 가져오면 `user:mohui.battle` 같은 안정적인 번호를 얻습니다. 음악/효과음 단계에서는 「사용자 / 공식」 그룹으로 나누어 선택하고, 대사 단계에는 「대사 음성」을 바인딩할 수 있습니다. 스토리에는 이 번호만 저장됩니다. 보낼 때는 현재 Mod가 실제로 참조하는 오디오만 패키징되므로, 다른 컴퓨터에 설치해도 작성자 본인 컴퓨터의 콘텐츠 라이브러리에 의존하지 않습니다. 자세한 설명은 `docs/ko/user_content.md`를 참조하세요.

보내기 전에 F6을 눌러 "건강 검사"를 열 수 있습니다. 컴파일 오류, 끊어진 경로와 도달 불가 단계, 플레이스홀더 텍스트, 이미지 에셋, 사용자 오디오 참조, 그리고 "인물이 등장하지 않은 상태에서 동작/대사"를 수행하는 블랙 스크린 위험을 검사합니다. 문제를 더블 클릭하면 해당 단계로 이동합니다. "안전 자동 수정"은 스토리 의미를 바꾸지 않는 기계적 문제만 처리하며(인물 자동 등장 보완 포함), 실행 취소를 지원합니다.

긴 스토리를 디버깅할 때는 단계를 선택한 후 F5를 누릅니다. 에디터가 독립 임시 패키지를 생성해 설치하고, 게임이 Title/Free 안전 장면에 도달하면 해당 단계부터 자동으로 시작합니다. 진입 전에 해당 단계 이전의 무대 상태(현재 장면, 무대 위 인물의 위치/표정/방향)를 자동으로 보완하므로, 스토리 중간부터 진입해도 더 이상 "캐릭터가 존재하지 않음"으로 블랙 스크린이 발생하지 않습니다. 임시 패키지는 정식 Mod를 덮어쓰지 않으며, 읽힌 후 자동으로 삭제됩니다. 오른쪽의 "흐름도"는 실제 점프 연결을 표시하며(일대다 분기는 서로 다른 색으로 구분), 끊어진 경로, 종료할 수 없는 무한 루프와 도달 불가 단계는 빨간 테두리와 텍스트로 동시에 표시됩니다.

### 4. 독립 실행 파일(PyInstaller 패키징, 선택 사항)

에디터와 AI 명령줄이 각각 하나의 exe로 생성되며 같은 런타임 디렉터리를 공유하므로 대상 컴퓨터에 Python이 필요 없습니다:

```bash
cd editor
.venv/Scripts/pip install pyinstaller
.venv/Scripts/python build_exe.py
```

산출물은 `editor/dist/lom_modkit/`에 있습니다(`build/`, `dist/`는 gitignore 처리됨):

| 파일 | 설명 |
| --- | --- |
| `lom_editor.exe` | 그래픽 에디터(콘솔 창 없음; 데이터 목록 내장, 에셋이 없으면 플레이스홀더 이미지 사용; 열기/저장은 기본적으로 현재 작업 디렉터리부터 시작) |
| `story_api_cli.exe` | AI / 스크립트 친화적 명령줄(check / compile / pack / new-story) |

`story_api_cli` 사용법(종료 코드 0/1, UTF-8; AI는 `--json`을 추가해 한 줄 구조화 결과를 받는 것을 권장):

```bash
story_api_cli check story.json
story_api_cli check --json story.json            # {"ok": true, "errors": [], "warnings": []}
story_api_cli compile story.json -o out.lua
story_api_cli pack mod目录 -o 我的mod.lommod
story_api_cli new-story my_story -o story.json
```

`--json`은 하위 명령 앞이나 뒤에 둘 수 있습니다. 실패 시에도 `{"ok": false, "errors": [...]}`를 출력하고 종료 코드는 여전히 1입니다.

AI 에이전트용 상세 매뉴얼(환경 요구 사항, 각 하위 명령의 매개변수/출력/종료 코드, --json 필드 구조, Python API 요약, 쓰기 작업 강제 규칙, 오류 대조표)은 `docs/ko/ai_cli.md`를 참조하세요.

## 개발

```bash
# 编译器测试（160 例）
cd compiler && python -m unittest tests.test_lomc

# 编辑器测试（冒烟/压力，offscreen 无头运行）
cd editor && .venv/Scripts/python tests/smoke_test.py
cd editor && .venv/Scripts/python tests/stress_test.py

# story_api / 登场防线测试（61 + 18 例）
cd editor && .venv/Scripts/python tests/story_api_test.py
cd editor && .venv/Scripts/python tests/stage_guard_test.py

# 插件构建
cd runtime/MortalModHost && dotnet build -c Release

# 插件冒烟测试（ModLoader/MiniJson/ModRegistry 离线验证）
cd runtime/MortalModHost && dotnet run --project test/SmokeTest -c Release
```

게임 내 디버깅: 아무 장면에서나 F7을 눌러 「원작 스토리 비활성화」 전역 임시 스위치를 전환합니다(세션 단위, 영구 저장 안 됨).
활성화하면 Free로 돌아올 때 자동으로 트리거되는 공식 메인 스토리, 서브 스토리와 장소 클릭으로 트리거되는 기본 스크립트를 건너뛰며, mod 트리거는 여전히 우선합니다. 이 스위치는 이번 게임 세션에서만 유효하며, F7을 다시 누르거나 게임을 재시작하면 복구됩니다.
이미 시작된 Story 연출은 강제로 중단되지 않으며, F8 메뉴는 영향을 받지 않습니다.

## 0.6.0

- 에디터 정보 아키텍처: 메뉴는 저빈도 기능을 관리하고 도구 모음에는 플레이 테스트/보내기만 유지; 왼쪽 열은 챕터와 단계만 관리하고 챕터 속성은 가운데 열로 이동; 단계는 두 줄 문구, 우클릭으로 삭제/이동; 미리보기 대사는 글자 수에 맞춰 늘어나고 중국어 줄바꿈 가능.
- 읽음 초기화가 `Save_universe.dat`와 `.json`을 동시에 수정하고 F5 플레이 테스트 패키지 `lom_modkit_preview`의 기록도 지웁니다.
- 음악/환경음 `fadeout` 이후에는 `wait`가 페이드아웃 시간을 모두 채워 다음 `PlayMusic`이 볼륨을 순간적으로 되돌리는 것을 방지합니다.
- Steam 일반 실행으로 BepInEx 로드 가능(`version.dll` + `ignore_disable_switch`).
- 예제 `showcase2`: 장면 1 후반의 나레이션과 대사를 분리; 위국(魏菊)이 장면 전환/제2막 진입 전에 퇴장.

## 언어

에디터 메뉴 「언어」에서 간체 중국어, 번체 중국어, 일본어, 한국어를 전환할 수 있으며, 설정이 기억됩니다.

게임 내 명사는 다음 순서로 가져옵니다:

1. [LoM-wiki](https://github.com/mohui666/LoM-wiki-CNS)(인물, 문파, 한청서(汗青书) / 생사부(生死簿), 속성 등)
2. wiki에 없는 항목은 게임 언팩 공식 언어표 사용(`lom_unpack/raw/*_zh-cn.txt` / `*_zh-tw.txt` / `*_kr.txt`)

공식 게임 인터페이스에는 번체 중국어, 간체 중국어, 한국어만 있고 **일본어는 없습니다**. 일본어 인물명과 속성명은 wiki 일본어 페이지에서 가져오고, 한국어 전체는 공식 언팩에서 가져옵니다. 게임 내 Mod 메뉴는 게임의 현재 언어를 따릅니다.

구현 세부 사항(디렉터리 구조, 폴백 규칙, 명사 재생성, 새 언어 추가 방법)은 `docs/ko/i18n.md`를 참조하세요.

## 설명 및 감사

- `docs/ko/mod_format.md`는 모든 구성 요소의 규약입니다(패키지 형식, 43종 노드, 사용자 콘텐츠, 런타임 동작). 코드를 바꾸기 전에 먼저 이 문서를 수정하세요. 사용자 지정 오디오 사용법은 `docs/ko/user_content.md`를 참조하세요.
- `data/editor_data.json`은 `tools/extract_editor_data.py`가 게임의 언팩 산출물에서 생성합니다
  (언팩 디렉터리는 환경 변수 `LOM_UNPACK_DIR`로 지정). 저장소에는 언팩 산출물과 게임 파일이 포함되지 않습니다.
- 게임 메커니즘 조사는 공식 스크립트의 실증 분석(1814개 스토리 스크립트)에 기반합니다. 디컴파일된 게임 소스 코드는
  저작권 문제로 저장소에 공개하지 않고 로컬 `docs/research/`에만 보관하여 연구용으로 사용합니다.
- 예제 mod는 도구의 기능을 시연하기 위한 것일 뿐, 게임 원작 스토리 콘텐츠를 포함하지 않습니다.
