# 현재 기능과 경계

이 문서는 현재 저장소 코드만 설명합니다. 노드의 정식 집합은 `editor/models.py`의 `NODE_SCHEMAS`이며 현재 63종입니다.

## 구현됨

- 스토리, 무대 연출, 여러 챕터, 현지화, 사용자 지정 캐릭터/오디오/배경/CG/Overlay와 사용자 지정 엔딩.
- 타이틀의 원작형 ‘MOD 캠페인 시작’ 불러오기 화면. MOD 수동 슬롯, 세 자동 슬롯, Universe 최근 슬롯과 영구 변수를 원작에서 격리하며 Manifest 장소/시간/Flag/호감도 트리거를 지원합니다.
- `custom_shop`은 원작 상점 재고를 일시 교체하고 종료 시 복원합니다.
- F5 테스트, 핫 리로드/Debugger, Editing/Release 점검, 복구 사본, 템플릿, 통계, 음성 커버리지, 로컬 Release Builder, 설치 진단과 Runtime 롤백.
- 강제 비공식 표시, 패키지 지문, 오프라인 이미지/동영상 워터마크 탐지. 작성자 서명은 아닙니다.

## 전투 경계

`combat`은 원작 1대1 캐릭터/장면 템플릿을 쓰고 이번 상대의 HP, 기력, 능력, 재능, 필살기와 행동 확률을 덮어쓸 수 있습니다. `battle`은 원작 아군·적군·중립 roster를 따로 재사용하고 인원, NPC HP와 플레이어 기술을 설정합니다. 전자는 Combat win/lose, 후자는 finish=true인 FriendWin/EnemyWin만 반환합니다. 원작 asset은 바꾸지 않으며 독자 Battle Engine도 아닙니다. 실기 검증은 필요합니다.

저수준 `enemy`, `battle_skill`, `goto_scene`는 호환성과 고급 구성을 위해 유지합니다.

`combat` / `battle` 노드는 검증된 모든 매개변수를 직접 설정하며 도구 전용 프리셋을 사용하지 않습니다. `key`는 원작 캐릭터／장면 기반만 선택하고 체력, 능력치, 행동 확률, 세 진영의 편성·인원·NPC 체력·전장 스킬은 현재 노드에 저장합니다. `battle_result`는 실제 win/lose만 분기하고 `battle_setup`, `reward`, `activity`는 기존 API만 묶습니다. `result_screen`은 원본 `mainui.DisplayMessageText`로 결산 제목과 설명을 표시한 뒤 `reward`와 같은 기존 API로 보상을 지급하며 새 UI를 만들지 않습니다. `mod_quest` / `quest_check`는 Host 세션 상태입니다. `persistent_var` / `persistent_check`는 Int32 상태를 `mod_<id>` 전용 슬롯에 결합된 Host sidecar에 원자적으로 저장하며 원작 GameSave는 바꾸지 않습니다. 임의 Lua 객체는 영속화하지 않습니다. draw/escape, 전투 지도, 모델, AI, 애니메이션, 메커니즘 사용자 지정도 지원하지 않습니다.

디컴파일과 실제 게임 검증으로 확인되지 않은 API는 추측해서 구현하지 않습니다.
