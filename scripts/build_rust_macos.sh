#!/bin/sh
# Build a standalone Apple Silicon/native macOS app without Python or Qt.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"
CARGO_BIN=${CARGO_BIN:-"$HOME/.cargo/bin/cargo"}
"$CARGO_BIN" build --locked --release -p lomc -p lom-editor
APP_VERSION=$("$ROOT/target/release/lomc" --version | awk '{print $2}')
APP="$ROOT/out/LoM Modkit Rust.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
EXTRACTOR_PYTHON=${EXTRACTOR_PYTHON:-"$ROOT/editor/.venv/bin/python"}
"$EXTRACTOR_PYTHON" "$ROOT/scripts/build_asset_extractor.py" --output-dir "$ROOT/out/asset-extractor-dist"
mkdir -p "$APP/Contents/Resources/asset-extractor"
cp -R "$ROOT/out/asset-extractor-dist/lom-asset-extractor/." "$APP/Contents/Resources/asset-extractor/"
cp "$ROOT/target/release/lom-editor" "$APP/Contents/MacOS/lom-editor"
cp "$ROOT/target/release/lomc" "$APP/Contents/MacOS/lomc"
# Bundle an already built managed Host; this does not install or launch the game.
HOST_INPUT=${HOST_RUNTIME_DIR:-"$ROOT/runtime/MortalModHost/bin/Release/net48"}
if [ -f "$HOST_INPUT/MortalModHost.dll" ] && [ -f "$HOST_INPUT/NVorbis.dll" ]; then
  mkdir -p "$APP/Contents/MacOS/runtime"
  cp "$HOST_INPUT/MortalModHost.dll" "$HOST_INPUT/NVorbis.dll" "$APP/Contents/MacOS/runtime/"
fi
cp "$ROOT/editor/assets/lom_editor.icns" "$APP/Contents/Resources/lom_editor.icns"
mkdir -p "$APP/Contents/Resources/doorstop"
cp "$ROOT/editor/assets/doorstop/README.md" "$ROOT/editor/assets/doorstop/LICENSE.UnityDoorstop" "$APP/Contents/Resources/doorstop/"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>lom-editor</string>
<key>CFBundleIdentifier</key><string>org.lommodkit.rust.editor</string>
<key>CFBundleName</key><string>LoM Modkit Rust</string>
<key>CFBundleDisplayName</key><string>LoM Modkit Rust</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$APP_VERSION</string>
<key>CFBundleVersion</key><string>$APP_VERSION</string>
<key>CFBundleIconFile</key><string>lom_editor.icns</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>12.0</string>
</dict></plist>
PLIST
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"
"$APP/Contents/Resources/asset-extractor/lom-asset-extractor" --self-check
"$APP/Contents/MacOS/lomc" --version
"$APP/Contents/MacOS/lom-editor" --smoke-preview "$ROOT/samples/showcase3"
printf 'Built: %s\n' "$APP"
