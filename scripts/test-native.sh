#!/bin/sh
# Tool-side validation only; never starts Windows or the game.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"
CARGO_BIN=${CARGO_BIN:-"$HOME/.cargo/bin/cargo"}
"$CARGO_BIN" fmt --all -- --check
"$CARGO_BIN" test --locked --workspace
"$CARGO_BIN" run --locked -p lom-editor -- --smoke-preview samples/showcase3
"$CARGO_BIN" run --locked -p lomc --example build_showcase3 -- out/showcase3-native
"$CARGO_BIN" run --locked -p lomc -- inspect out/showcase3-native/showcase3.lommod --json
