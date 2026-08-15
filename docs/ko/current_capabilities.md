# 현재 기능과 경계

이 문서는 현재 저장소 코드만 설명합니다. 노드의 정식 집합은 `editor/models.py`의 `NODE_SCHEMAS`이며 현재 62종입니다.

## 구현됨

- 스토리, 무대 연출, 여러 챕터, 현지화, 사용자 지정 캐릭터/오디오/배경/CG/Overlay와 사용자 지정 엔딩.
- `campaign.new_game`의 `mod_<id>` 분리 저장 슬롯과 Manifest의 장소/시간/Flag/호감도 트리거.
- `custom_shop`은 원작 상점 재고를 일시 교체하고 종료 시 복원합니다.
- F5 테스트, 핫 리로드/Debugger, Editing/Release 점검, 복구 사본, 템플릿, 통계, 음성 커버리지, 로컬 Release Builder, 설치 진단과 Runtime 롤백.
- 강제 비공식 표시, 패키지 지문, 오프라인 이미지/동영상 워터마크 탐지. 작성자 서명은 아닙니다.

## 전투 경계

`enemy`, `battle_skill`, `goto_scene`의 `Combat` / `Battle`은 검증된 원작 API를 호출합니다. 고수준 `combat` / `battle`은 원작 템플릿을 선택하고 전자는 Combat win/lose, 후자는 finish=true인 FriendWin/EnemyWin만 돌려보냅니다. 독자 Battle Engine이 아니며 실기 미검증입니다.

장 설정에서 Battle Preset을 관리하여 원작 `combat` / `battle` 템플릿과 검증된 적 설정을 재사용할 수 있습니다. `battle_result`는 실제 win/lose만 분기하고 `battle_setup`, `reward`, `activity`는 기존 API만 묶습니다. `mod_quest` / `quest_check`는 Host 세션 상태입니다. `persistent_var` / `persistent_check`는 Int32 상태를 `mod_<id>` 전용 슬롯에 결합된 Host sidecar에 원자적으로 저장하며 원작 GameSave는 바꾸지 않습니다. 임의 Lua 객체는 영속화하지 않습니다. draw/escape, 전투 지도, 모델, AI, 애니메이션, 메커니즘 사용자 지정도 지원하지 않습니다.

디컴파일과 실제 게임 검증으로 확인되지 않은 API는 추측해서 구현하지 않습니다.
