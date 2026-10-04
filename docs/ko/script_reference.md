# 스크립트 / API 문서

> **버전 안내:** 이 문서는 현재 Rust 버전과 아직 완전히 동기화되지 않았습니다. 이전 UI 메뉴와 구현 설명은 과거 버전의 참고 정보입니다. 최신 내용은 [중국어 간체 문서](../chs/script_reference.md)를 기준으로 확인하세요.

‘도움말 → 문서 → 스크립트 / API’는 실제 schema에서 생성되며 63개 노드마다 JSON 키, UI 의미, 필수 여부, 유형·열거형, 기본값, 최소 예제와 Runtime API를 표시합니다.

- [Mod v3 형식과 컴파일 규약](mod_format.md)
- [story_api / CLI](ai_cli.md)
- [다국어 규약](i18n.md)
- [현재 기능과 경계](current_capabilities.md)

`combat`의 인물은 결투 이름과 전투 애니메이션만 결정하고 배경은 공식 `views`에서 별도로 선택하며 다른 값은 자유롭게 설정합니다. `battle`은 아군·적군 진영, 총인원, 확인된 공식 이름 있는 인물만 설정합니다. 이전 프리셋과 `battle_setup`은 삭제되었습니다. 사용자 콘텐츠는 `user:<id>`를 사용합니다.
