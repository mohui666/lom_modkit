# 현재 기능과 경계

이 문서는 현재 저장소 코드만 설명합니다. 노드의 정식 집합은 `editor/models.py`의 `NODE_SCHEMAS`이며 현재 47종입니다.

## 구현됨

- 스토리, 무대 연출, 여러 챕터, 현지화, 사용자 지정 캐릭터/오디오/배경/CG/Overlay와 사용자 지정 엔딩.
- `campaign.new_game`의 `mod_<id>` 분리 저장 슬롯과 Manifest의 장소/시간/Flag/호감도 트리거.
- F5 테스트, 핫 리로드/Debugger, Editing/Release 점검, 복구 사본, 템플릿, 통계, 음성 커버리지, 로컬 Release Builder, 설치 진단과 Runtime 롤백.
- 강제 비공식 표시, 패키지 지문, 오프라인 이미지/동영상 워터마크 탐지. 작성자 서명은 아닙니다.

## 전투 경계

`enemy`, `battle_skill`, `goto_scene`의 `Combat` / `Battle`은 검증된 원작 《활협전》 API를 호출합니다. 고수준 `combat`은 원작 Combat key와 적 설정을 조합하고 `CombatManager.GameOver(bool)`의 실제 win/lose를 작성자 노드로 돌려보냅니다. 독자 Battle Engine이 아니며 새 흐름은 실기 미검증입니다.

고수준 `battle`, Battle Preset, draw/escape 콜백, `reward` 집계 노드, 임의 상품 Custom Shop, 독립 `mod_quest`, 임의의 영속 Mod 변수는 미구현입니다. `modflags` / `modvars`는 Story 세션에만 있고 `game_flag`는 원작에 존재하는 FlagData만 기록합니다. 전투 지도, 모델, AI, 애니메이션, 메커니즘의 사용자 지정도 지원하지 않습니다.

디컴파일과 실제 게임 검증으로 확인되지 않은 API는 추측해서 구현하지 않습니다.
