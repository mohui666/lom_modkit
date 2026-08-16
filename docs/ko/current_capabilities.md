# 현재 기능과 경계

이 문서는 현재 저장소 코드만 설명합니다. 노드의 정식 집합은 `editor/models.py`의 `NODE_SCHEMAS`이며 현재 62종입니다.

## 구현됨

- 스토리, 무대 연출, 여러 챕터, 현지화, 사용자 지정 캐릭터/오디오/배경/CG/Overlay와 사용자 지정 엔딩.
- 타이틀의 원작형 ‘MOD 캠페인 시작’ 불러오기 화면. MOD 수동 슬롯, 세 자동 슬롯, Universe 최근 슬롯과 영구 변수를 원작에서 격리하며 Manifest 장소/시간/Flag/호감도 트리거를 지원합니다.
- `custom_shop`은 원작 상점 재고를 일시 교체하고 종료 시 복원합니다.
- F5 테스트, 핫 리로드/Debugger, Editing/Release 점검, 복구 사본, 템플릿, 통계, 음성 커버리지, 로컬 Release Builder, 설치 진단과 Runtime 롤백.
- 강제 비공식 표시, 패키지 지문, 오프라인 이미지/동영상 워터마크 탐지. 작성자 서명은 아닙니다.

## 전투 경계

`combat`은 원작 1대1 캐릭터/장면 템플릿을 쓰고 이번 상대의 HP, 기력, 능력, 재능, 필살기와 행동 확률을 덮어쓸 수 있습니다. `battle`은 원작 아군·적군·중립 roster를 따로 재사용하고 인원, NPC HP와 플레이어 기술을 설정합니다. 전자는 Combat win/lose, 후자는 finish=true인 FriendWin/EnemyWin만 반환합니다. 원작 asset은 바꾸지 않으며 독자 Battle Engine도 아닙니다. 실기 검증은 필요합니다.

저수준 `enemy`, `battle_skill`, `goto_scene`는 호환성과 고급 구성을 위해 유지합니다.

`combat`의 인물은 네 종류 전투 애니메이션만 결정하며 체력, 능력치, 스킬, 행동 확률은 자유롭게 설정합니다. `battle`은 아군·적군 진영, 총인원, 확인된 공식 이름 있는 인물만 설정하며 이름 있는 인물도 총인원에 포함됩니다. 이전 프리셋과 `battle_setup`은 삭제되었습니다. 영구 상태는 `mod_campaign_<campaign_id>` 전용 슬롯에 연결되며 원작 GameSave는 바꾸지 않습니다.

저수준 `enemy` / `battle_skill`은 독립적으로 사용할 수 있고 Combat / Battle 결과는 `reward`, `result_screen`, `custom_shop`, `mod_quest` 등 기존 노드와 조합할 수 있습니다.
장기 정수 상태에는 `persistent_var` / `persistent_check`를 사용합니다.

디컴파일과 실제 게임 검증으로 확인되지 않은 API는 추측해서 구현하지 않습니다.
