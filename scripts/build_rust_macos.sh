#!/bin/sh
# Build a standalone Apple Silicon/native macOS app without Python or Qt.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"
CARGO_BIN=${CARGO_BIN:-"$HOME/.cargo/bin/cargo"}
"$CARGO_BIN" build --locked --release -p lomc -p lom-editor
APP="$ROOT/out/LoM Modkit Rust.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/target/release/lom-editor" "$APP/Contents/MacOS/lom-editor"
cp "$ROOT/target/release/lomc" "$APP/Contents/MacOS/lomc"
cp "$ROOT/editor/assets/lom_editor.icns" "$APP/Contents/Resources/lom_editor.icns"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>lom-editor</string>
<key>CFBundleIdentifier</key><string>org.lommodkit.rust.editor</string>
<key>CFBundleName</key><string>LoM Modkit Rust</string>
<key>CFBundleDisplayName</key><string>LoM Modkit Rust</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>1.2.0</string>
<key>CFBundleVersion</key><string>1.2.0</string>
<key>CFBundleIconFile</key><string>lom_editor.icns</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>12.0</string>
</dict></plist>
PLIST
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"
"$APP/Contents/MacOS/lomc" --version
"$APP/Contents/MacOS/lom-editor" --smoke-preview "$ROOT/samples/showcase3"
printf 'Built: %s\n' "$APP"
