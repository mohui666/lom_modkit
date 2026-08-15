# lom_modkit

**『활협전』(Legend of Mortal) 비주얼 시나리오 Mod 제작 도구.**

Lua를 작성할 필요가 없습니다. 그래픽 에디터로 인물 대사, 장면 연출, 분기 시나리오, 음악·효과음을 편성하고,
한 번의 클릭으로 `.lommod`를보내 게임에서 바로 실행할 수 있습니다.

[![Release v0.7.0](https://img.shields.io/badge/release-v0.7.0-blue)](https://github.com/mohui666/lom_modkit/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-lightgrey)](#호환성)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)

**[⬇ Windows 판 다운로드](https://github.com/mohui666/lom_modkit/releases/download/v0.7.0/lom_modkit-v0.7.0_windows_x64.zip)** ·
[빠른 시작](#빠른-시작) ·
[문서](docs/ko/README.md)

> 언어: [简体中文](README.md) · [繁體中文](README.zh_TW.md) · [日本語](README.ja.md) · 한국어(본문)

<!-- TODO(홍보 소재): 여기에 8~15초 루프 GIF 배치:
     lom_editor.exe 열기 → 새 시나리오 → 캐릭터/대사/장면/음악 선택 → F5 → 게임 내 실제 연출.
     권장 경로 docs/assets/screenshots/demo.gif, 이후 ![demo](docs/assets/screenshots/demo.gif)로 본 주석을 교체. -->

## 무엇인가

lom_modkit은 『활협전』의 **기존 인물, 장면, 음악, 특수 효과와 수치 시스템**으로 오리지널 시나리오를 만들게 해 줍니다:
그래픽 에디터에서 클릭으로 설정하고, 독립적인 `.lommod` Mod 패키지로내면 게임 내 플러그인이 로드해 연출합니다.
「새 캠페인 시작」(새 게임 흐름을 인계받고 세이브 슬롯을 격리)과 자유 모드 맵 지점 트리거를 지원합니다.

팬이 직접 제작한 도구로 게임 개발사와 무관하며, 게임 본체의 어떤 파일도 포함하지 않습니다. MIT 라이선스.

## 무엇을 할 수 있나

- **비주얼 시나리오 편집**: 인물, 대사, 표정, 위치, 장면, 음악, 효과음, 특수 효과를 모두 UI로 설정합니다.
- **분기 시나리오**: 선택지, 조건 분기, 속성 판정, 주사위 체크, 다중 챕터 체인 스크립트.
- **스토리 콘텐츠 현지화**: 하나의 Story에서 중국어 간체·번체, 일본어, 한국어 번역을 관리하며 기본 언어와 누락 번역 대체를 지원합니다. 기존 프로젝트는 마이그레이션할 필요가 없습니다.
- **게임 콘텐츠 직접 호출**: 게임에 있는 인물, 장면, 음악과 연출 시스템을 그대로 사용하므로 직접 새로 만들 필요가 없습니다.
- **사용자 지정 오디오와 대사 음성**: `.ogg` / `.wav`를 가져와 음악, 효과음, 그리고 문장 단위의 캐릭터 대사 음성으로 사용할 수 있습니다.
- **한 번의 클릭으로 게임 내 시연**: 임의의 시나리오 단계를 선택하고 F5를 누르면 해당 단계부터 바로 게임에 진입해 테스트합니다.
- **진정한 Mod 패키지**: 보낸 `.lommod`는 자체 완결형으로 다른 플레이어에게 바로 공유할 수 있습니다.

## 빠른 시작

### 1. 다운로드

[lom_modkit-v0.7.0_windows_x64.zip](https://github.com/mohui666/lom_modkit/releases/download/v0.7.0/lom_modkit-v0.7.0_windows_x64.zip)을 다운로드하고 압축을 풉니다. Python을 설치할 필요가 없습니다.

### 2. 실행

`lom_editor.exe`를 실행합니다.

### 3. 『활협전』 연결

메뉴 「파일 → 설치 관리」에서 `Mortal.exe`가 있는 게임 폴더를 선택하고 「BepInEx 설치」를 클릭합니다——
에디터가 호환되는 BepInEx 6와 게임 내 런타임을 자동으로 다운로드·설치하고 Steam 일반 실행 수리를 기록합니다.

### 4. 첫 번째 시나리오 만들기

새 시나리오 → 캐릭터 추가 → 대사 추가 → **F5**로 시연.

### 5. 보내기

내면 `xxx.lommod`를 얻으며, 다른 사람에게내면 됩니다(상대방도 본 도구로 설치한 런타임이 필요합니다).

## 스크린샷

<!-- TODO(홍보 소재): 최소 네 장의 이미지, docs/assets/screenshots/ 배치 권장:
     ① 메인 에디터 전체 모습 ② 시나리오 흐름도 ③ 사용자 오디오/대사 음성 ④ 게임 내 실제 효과.
     주석을 해제하고 경로를 교체:
![시나리오 에디터](docs/assets/screenshots/editor.png)
![게임 내 효과](docs/assets/screenshots/ingame.png)
![분기와 흐름도](docs/assets/screenshots/flow_graph.png)
-->

## 시나리오 제작을 위해 설계된 워크플로

### 임의의 위치에서 시연(F5)

단계를 선택하고 **F5**를 누릅니다: 에디터가 독립 임시 패키지를 생성하고, 게임이 안전 장면에 도달하면 해당 단계부터 자동으로 시작합니다.
진입 전에 해당 단계 이전의 무대 상태(현재 장면, 무대 위 인물의 위치/표정/방향)를 자동으로 보완하므로——
시나리오 중간부터 진입해도 더 이상 "캐릭터가 존재하지 않음"으로 블랙 스크린이 발생하지 않습니다. 임시 패키지는 정식 Mod를 덮어쓰지 않으며, 읽힌 후 자동으로 삭제됩니다.

### 보내기 전 점검(F6)

**F6**를 눌러 검사합니다: 컴파일 오류, 끊어진 경로와 도달 불가 단계, 무한 루프, 플레이스홀더 텍스트, 누락된 에셋,
잘못된 사용자 오디오 참조, "인물이 등장하지 않은 상태에서 대사/행동"하는 블랙 스크린 위험. 문제를 더블 클릭하면 해당 단계로 이동합니다.
「안전 자동 수정」은 시나리오 의미를 바꾸지 않는 기계적 문제만 처리하며(인물 자동 등장 보완 포함), 실행 취소를 지원합니다.

### 시나리오 흐름도

오른쪽 「흐름도」는 실제 점프 연결을 표시하며(일대다 분기는 서로 다른 색으로 구분), 끊어진 경로,
종료할 수 없는 무한 루프와 도달 불가 단계는 빨간 테두리와 텍스트로 동시에 표시됩니다.

## 사용자 콘텐츠

로컬 캐릭터, 오디오 또는 이미지를 「사용자 콘텐츠 보관함」(메뉴 「파일 → 사용자 콘텐츠 보관함」)으로 가져오면 안정적인 번호
(예: `user:mohui.battle`)를 얻고, 시나리오 단계에서 「사용자 / 공식」 그룹으로 나누어 선택합니다. 시나리오는 번호만 저장합니다.
보낼 때는 현재 Mod가 실제로 참조하는 콘텐츠만 패키징되므로, 플레이어 컴퓨터는 작성자 본인 컴퓨터의 콘텐츠 보관함에 의존하지 않습니다.

| 콘텐츠 유형 | 상태 |
| --- | --- |
| 사용자 지정 음악 / 효과음 / 환경음 | ✅ 지원됨 |
| 캐릭터 대사 음성 | ✅ 지원됨 |
| 사용자 지정 스탠딩 / 칭호 / 소개 카드 / 체형 | ✅ 지원됨 |
| 사용자 지정 배경 / CG / Overlay 이미지 | ✅ 지원됨 |
| 커뮤니티 콘텐츠 보관함 | ◯ Roadmap |

자세한 사용법은 [사용자 콘텐츠 보관함 문서](docs/ko/user_content.md)를 참조하세요.

## 다른 사람이 만든 Mod 설치

`.lommod`를 에디터 「파일 → 설치 관리」로 설치하고 사용에 체크하면 됩니다
(수동 경로: `BepInEx/plugins/MortalModHost/mods/`).
게임에 진입한 후 자유 장면/타이틀 화면 왼쪽 아래의 「活侠MOD」 버튼을 클릭(또는 F8)하고,
「mod 스토리 연출」 또는 「새 캠페인 시작」을 선택합니다. 게임 내 메뉴는 게임의 현재 언어를 따릅니다.

## 호환성

| 항목 | 상태 |
| --- | --- |
| Windows 10/11 | ✅ |
| Steam 『활협전』 | ✅(일반 실행 수리 포함) |
| BepInEx | 에디터가 자동으로 설치 |
| Python | Windows 배포판 불필요 |
| 게임 원본 파일 수정 | 필요 없음 |

## 현재 버전

**v0.7.0**: 커스텀 스탠딩 · 대사 음성 연결 · 소개 카드와 칭호 · 체형 슬라이더 ·
퇴장 시 무대 정리 · 노드를 종류로 번호 매김.

전체 변경 사항은 [Release Notes](https://github.com/mohui666/lom_modkit/releases)를 참조하세요.

## Roadmap

- 커뮤니티 콘텐츠 저장소(사용자 콘텐츠 공유/재사용)
- 작가용 전투 / 전역 오케스트레이션 계층(현재는 검증된 저수준 노드만 제공)

## 문서

| 문서 | 내용 |
| --- | --- |
| [문서 색인](docs/ko/README.md) | 언어 네비게이션과 독자 가이드 |
| [사용자 콘텐츠 보관함](docs/ko/user_content.md) | 사용자 지정 오디오 / 대사 음성 사용법 |
| [현재 기능과 경계](docs/ko/current_capabilities.md) | 구현됨, 저수준만 제공, 미구현 기능의 경계 |
| [Mod 패키지 형식 규약](docs/ko/mod_format.md) | 패키지 구조, 49종 노드, 컴파일 규약, 런타임 동작 |
| [AI / CLI 매뉴얼](docs/ko/ai_cli.md) | story_api 명령줄과 Python API |
| [다국어](docs/ko/i18n.md) | 인터페이스와 문서의 i18n 아키텍처 |

## For Developers

### 아키텍처

```text
┌─────────────┐
│ lom_editor  │  PySide6 图形编辑器
└──────┬──────┘
       │ story JSON
       ▼
┌─────────────┐
│    lomc     │  JSON → 游戏原生 Lua 编译器（纯标准库）
└──────┬──────┘
       │ Lua + assets
       ▼
┌─────────────┐
│   .lommod   │  自包含 Mod 包（zip）
└──────┬──────┘
       ▼
┌──────────────────┐
│ MortalModHost    │  BepInEx 游戏内插件（C# net48）
└──────┬───────────┘
       ▼
  Legend of Mortal
```

### 소스 코드 디렉터리

- `compiler/`(`lomc`) — JSON 시나리오 → 게임 네이티브 Lua 컴파일러
- `editor/` — PySide6 그래픽 에디터; `editor/story_api.py`는 AI/스크립트 통제 인터페이스(Python API + CLI)
- `runtime/MortalModHost/` — BepInEx 게임 내 플러그인
- `tools/` — 언팩 산출물에서 에디터 데이터/에셋을 추출하는 스크립트
- `data/` — 에디터 데이터(`editor_data.json`, schema 3)
- `samples/` — 예제 mod(demo_mod, showcase, showcase2 전체 노드 시연 2.0, snack_case 《点心大盗疑案》, probe)

### 소스에서 실행

```bash
# 编辑器
cd editor
python -m venv .venv
.venv/Scripts/pip install PySide6
run_editor.bat

# 编译器（无依赖）
PYTHONPATH=compiler python -m lomc check story.json
PYTHONPATH=compiler python -m lomc pack mod目录 -o 我的mod.lommod
```

### 빌드와 테스트

```bash
# 编译器测试（160 例）
cd compiler && python -m unittest tests.test_lomc

# 编辑器测试（冒烟/压力，offscreen 无头运行）
cd editor && .venv/Scripts/python tests/smoke_test.py
cd editor && .venv/Scripts/python tests/stress_test.py

# story_api / 登场防线测试（61 + 18 例）
cd editor && .venv/Scripts/python tests/story_api_test.py
cd editor && .venv/Scripts/python tests/stage_guard_test.py

# 插件构建与冒烟测试
cd runtime/MortalModHost && dotnet build -c Release
cd runtime/MortalModHost && dotnet run --project test/SmokeTest -c Release
```

Windows 배포판 패키징: `cd editor && .venv/Scripts/python build_exe.py`
(산출물은 `editor/dist/lom_modkit/`에 있으며, `lom_editor.exe`와 `story_api_cli.exe`를 포함합니다).

게임 내 디버깅: 아무 장면에서나 **F7**을 눌러 「원작 시나리오 비활성화」 세션 단위 스위치를 전환합니다(영구 저장 안 됨).
읽음이 다시 노랗게 변하는 경우를 재테스트할 때는 에디터 「시연 → 시나리오 읽음 상태 초기화」를 사용합니다.

## FAQ

**Q: Steam에서 「시작」을 눌러도 「活侠MOD」 버튼이 없고 F8이 반응하지 않나요?**
에디터 「파일 → 설치 관리」에서 「Steam 로드 실패 수리」를 클릭한 다음 Steam에서 **일반 실행**(관리자 권한 아님)으로 시작합니다.

**Q: Python을 설치해야 하나요?**
필요 없습니다. Windows 배포판은 독립 exe입니다. 소스에서 실행/개발할 때만 Python 3.10+와 .NET(플러그인 빌드)이 필요합니다.

**Q: Mod가 제 게임 파일이나 세이브를 수정하나요?**
공식 스크립트와 텍스트 테이블은 수정하지 않습니다. 「새 캠페인 시작」은 격리된 세이브 슬롯(`mod_<modid>`)을 사용하므로 정상 세이브를 덮어쓰지 않습니다.

**Q: 만든 Mod를 다른 사람에게 보낼 수 있나요?**
가능합니다. 보낸 `.lommod`는 자체 완결형(참조하는 오디오/이미지 포함)이며, 상대방이 본 도구로 런타임을 설치하면 플레이할 수 있습니다.

## 라이선스와 고지

MIT 라이선스([LICENSE](LICENSE)). 팬이 직접 제작한 도구로 게임 개발사와 무관하며, 게임 본체의 어떤 파일도 포함하지 않습니다.

- 게임 메커니즘 조사는 공식 스크립트의 실증 분석(1814개 시나리오 스크립트)에 기반합니다. 디컴파일된 소스 코드는 저작권 문제로 저장소에 공개하지 않습니다.
- `data/editor_data.json`은 `tools/extract_editor_data.py`가 언팩 산출물에서 생성합니다. 저장소에는 언팩 산출물과 게임 파일이 포함되지 않습니다.
- 예제 mod는 도구의 기능을 시연하기 위한 것일 뿐, 게임 원작 시나리오 콘텐츠를 포함하지 않습니다.
