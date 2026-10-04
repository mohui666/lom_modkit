#!/bin/sh
# Compatibility entry point for the native Rust app builder.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
exec sh "$ROOT/scripts/build_rust_macos.sh" "$@"
