# lom_modkit

**《활협전》(Legend of Mortal)의 스토리 Mod를 만드는 시각적 편집 도구입니다.**

대사, 배경, 분기, 오디오를 편집하고 `.lommod`로 내보내면 C# 게임 호스트가 불러옵니다.

> 언어：[中文（CHS）](README.md) · [中文（CHT）](README.cht.md) · [日本語](README.ja.md) · 한국어（현재 페이지）

## 버전과 다운로드

| 버전 | 설명 |
| --- | --- |
| **현재 소스: v1.4.3** | Rust 편집기·컴파일러와 C# 게임 호스트를 사용하며 `.lommod` v3 형식을 유지합니다. |
| **Windows 릴리스: v1.4.3** | 원작 인물 일러스트 추가, 자유로운 이미지 배율, 플레이용 ZIP, 창 크기·DPI 대응을 지원합니다. |

[v1.4.3 다운로드(Windows x64)](https://github.com/mohui666/lom_modkit/releases/download/v1.4.3/lom_modkit-v1.4.3_windows_x64.zip) · [업데이트 내용 및 검증 범위(중국어 간체)](RELEASE_NOTES_v1.4.3.md)

ZIP 전체를 압축 해제하고 `lom_modkit/lom-editor.exe`를 실행하세요. 게임을 종료한 뒤 ‘게임 및 MOD’에서 게임에 설치된 Host를 1.4.3으로 업데이트하세요. 새 기능에 필요한 최소 Host 버전은 작품을 내보낼 때 자동으로 기록됩니다.

## 1.4.3의 새 기능

- 원작 인물의 ID·이름·기존 표정을 유지하면서 `tired`, `sleep` 등의 표정을 추가하거나 지정한 일러스트를 교체할 수 있습니다. 리소스 화면에서 표정을 선택하고 해당 이미지를 미리 볼 수 있습니다.
- 인물, CG, 오버레이 이미지, 소개 카드 이미지의 배율을 게임에서 표현할 수 있는 임의의 양수 백분율로 지정할 수 있습니다. 소수도 지원하며 기존의 고정 상한을 없앴습니다. 인물은 원작 표시 영역 전체에 종횡비를 유지해 맞춥니다.
- 작품, 자체 제작 리소스, Host, 로더, 설치 도구를 하나의 플레이용 ZIP으로 내보냅니다. 처음에는 없는 로더를 다운로드합니다. 플레이어는 압축을 푼 뒤 ‘安装MOD.cmd’를 실행해 오프라인으로 설치할 수 있으며 편집기는 필요하지 않습니다. 정품 게임은 별도로 설치되어 있어야 합니다.
- 시스템 DPI를 유지하면서 창 크기에 맞춰 UI를 자동 조정합니다. 설정에서 UI 배율을 직접 바꿀 수도 있습니다.

사용법은 [인물 일러스트 추가 및 리소스 공유(중국어 간체)](docs/chs/user_content.md)를 참고하세요. 플레이용 ZIP에는 게임 본체나 원작 이미지의 미리 보기 캐시가 포함되지 않습니다.

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
| [Mod 패키지 규격](docs/chs/mod_format.md) | `.lommod` v3, 67종 노드 및 C# 호스트 호환성 |
| [전환 및 빌드](docs/chs/rust_migration.md) | Rust 구성 요소, 빌드 방법 및 전환 범위 |

[한국어 문서 색인 및 구버전 번역](docs/ko/README.md) · [전체 문서](docs/README.md)

## 라이선스

[MIT 라이선스](LICENSE). 팬이 만든 비공식 도구이며 게임 개발사와 관계없습니다. 게임 본체, 추출한 리소스, 전체 디컴파일 소스는 포함하지 않습니다. 예제는 도구의 기능을 보여 주기 위한 것입니다.
