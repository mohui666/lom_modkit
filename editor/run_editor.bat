@echo off
setlocal
cd /d "%~dp0\.."
cargo run --locked --release -p lom-editor -- %*
