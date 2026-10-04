# Rust API / CLI

편집기와 컴파일러가 Rust로 이전되었습니다. 기존 Python API는 폐기되었습니다.

```sh
cargo build --locked --release -p lomc
target/release/lomc --help
target/release/lomc check samples/showcase3/story/main.json --json
target/release/lomc compile samples/showcase3/story/main.json -o out/main.lua --json
target/release/lomc pack samples/showcase3 -o out/showcase3.lommod --json
target/release/lomc author request.json --json
target/release/lomc edit story.json --operations operations.json --json
```

`author`: `{"op":"add_node","params":{"story":{...},"node_type":"say","fields":{"mode":"narrative","text":"..."}}}`.

`edit` applies an array of controlled operations atomically. On failure the input is unchanged. Use `check` before compilation. JSON output is UTF-8; exit code 0 means success and 1 means failure.

[API reference and operation parameters](../chs/ai_cli.md) · [Migration coverage](../chs/rust_migration.md)
