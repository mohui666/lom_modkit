# lom_modkit

**《활협전》(Legend of Mortal)의 스토리 Mod를 만드는 시각적 편집 도구입니다.**

대사, 배경, 분기, 오디오를 편집하고 `.lommod`로 내보내면 C# 게임 호스트가 불러옵니다.

> 언어：[中文（CHS）](README.md) · [中文（CHT）](README.cht.md) · [日本語](README.ja.md) · 한국어（현재 페이지）

## 버전과 다운로드

| 버전 | 설명 |
| --- | --- |
| **현재 소스: v1.2.0** | 편집기와 컴파일러를 Rust로 전환했습니다. C# 게임 호스트와 `.lommod` v3 형식은 유지합니다. |
| **최신 공개 릴리스(2026-10-04 기준): v1.1.1** | 구형 Python / Qt 도구의 Windows 배포판이며, 현재 Rust 편집기가 아닙니다. |

[구버전 v1.1.1 다운로드(Windows)](https://github.com/mohui666/lom_modkit/releases/download/v1.1.1/lom_modkit-v1.1.1_windows_x64.zip) · [v1.1.1 릴리스 안내](https://github.com/mohui666/lom_modkit/releases/tag/v1.1.1)

현재 Rust 편집기를 사용하려면 소스에서 실행하거나 빌드하세요. 구버전의 `lom_editor.exe`, 메뉴, 사용 절차는 Rust 버전과 다릅니다.

## 현재 버전 실행

저장소 루트에서 실행합니다.

```sh
cargo run --locked -p lom-editor
```

macOS 앱 빌드:

```sh
scripts/build-macos.sh
```

결과물은 `out/LoM Modkit Rust.app`입니다. 환경 및 리소스 설정은 [현재 Mac 안내(중국어 간체)](docs/chs/macos.md), Windows 빌드와 C# 호스트 요구 사항은 [Rust 전환 안내(중국어 간체)](docs/chs/rust_migration.md)를 참조하세요.

## 문서

**현재 사용법은 중국어 간체 문서를 기준으로 합니다.** 한국어·중국어 번체·일본어의 기존 UI 안내는 아직 Rust 버전에 완전히 맞춰 갱신되지 않았습니다. Qt 메뉴와 Python 명령은 구버전 참고 자료입니다.

| 현재 문서(중국어 간체) | 내용 |
| --- | --- |
| [사용 안내](docs/chs/software_usage.md) | Rust 편집기의 조작 방법과 기본 흐름 |
| [지원 기능](docs/chs/current_capabilities.md) | 구현된 기능, 제한 및 검증 범위 |
| [Rust CLI](docs/chs/ai_cli.md) | `lomc` 명령과 제어된 편집 API |
| [Mod 패키지 규격](docs/chs/mod_format.md) | `.lommod` v3, 63종 노드 및 C# 호스트 호환성 |
| [전환 및 빌드](docs/chs/rust_migration.md) | Rust 구성 요소, 빌드 방법 및 전환 범위 |

[한국어 문서 색인 및 구버전 번역](docs/ko/README.md) · [전체 문서](docs/README.md)

## 라이선스

[MIT 라이선스](LICENSE). 팬이 만든 비공식 도구이며 게임 개발사와 관계없습니다. 게임 본체, 추출한 리소스, 전체 디컴파일 소스는 포함하지 않습니다. 예제는 도구의 기능을 보여 주기 위한 것입니다.
