# lom_modkit 문서

> 언어：[中文（CHS）](../README.md) · [中文（CHT）](../cht/README.md) · [日本語](../ja/README.md) · 한국어（현재 페이지）

현재 소스는 **v1.2.0**입니다. 편집기와 컴파일러는 Rust로 작성했으며 C# 게임 호스트와 `.lommod` v3 규격은 유지합니다. 2026-10-04 기준 최신 공개 릴리스인 [v1.1.1](https://github.com/mohui666/lom_modkit/releases/tag/v1.1.1)은 구형 Python / Qt 도구입니다.

## 현재 버전 안내

**사용법은 중국어 간체(CHS) 문서를 기준으로 합니다.** 이 폴더의 UI 안내는 아직 Rust 버전에 완전히 맞춰 갱신되지 않았습니다. 기존 메뉴, 단축키, Python 명령을 현재 버전의 사용법으로 적용하지 마세요.

한국어 입문: [Rust CLI 소개](ai_cli.md). 전체 API와 매개변수는 아래 중국어 간체 문서를 참조하세요.

| 안내(중국어 간체) | 내용 |
| --- | --- |
| [사용 안내](../chs/software_usage.md) | Rust 편집기 기본 조작 |
| [지원 기능](../chs/current_capabilities.md) | 기능 범위 및 검증 상태 |
| [Mac 편집기](../chs/macos.md) | 빌드, 실행 및 리소스 설정 |
| [CLI / API](../chs/ai_cli.md) | Rust `lomc`와 제어된 편집 API |
| [Mod 패키지 규격](../chs/mod_format.md) | v3 규격과 63종 노드 |
| [사용자 콘텐츠](../chs/user_content.md) | 사용자 캐릭터, 이미지 및 오디오 |
| [유지보수 안내](../chs/maintainers.md) | 저장, 검사, 배포 및 게임 연동 |
| [Rust 전환](../chs/rust_migration.md) | 구성 요소 및 전환 범위 |

## 아직 갱신되지 않은 한국어 번역

다음 문서는 구버전과 용어를 참고할 수 있도록 보존합니다. 현재 동작과 규격은 위의 중국어 간체 원문에서 확인하세요.

[사용 안내](software_usage.md) · [지원 기능](current_capabilities.md) · [스크립트 / API](script_reference.md) · [Mod 패키지 규격](mod_format.md) · [사용자 콘텐츠](user_content.md) · [다국어 규칙](i18n.md)

디컴파일 절차는 [중국어 간체 안내](../chs/decompiled_api.md)를 참조하세요. 게임의 전체 디컴파일 소스는 저장소에 포함하지 않습니다. 문서를 바꿀 때는 먼저 `chs/`를 갱신한 뒤 각 번역에 반영합니다.
