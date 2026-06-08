#!/bin/bash
# Creates/macOS .app bundle for Squarez.
set -euo pipefail

APP="target/debug/Squarez.app"
EXE="target/debug/squarez"

mkdir -p "$APP/Contents/MacOS"
cp "$EXE" "$APP/Contents/MacOS/squarez"

if [ ! -f "$APP/Contents/Info.plist" ]; then
cat > "$APP/Contents/Info.plist" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>squarez</string>
    <key>CFBundleIdentifier</key>
    <string>com.squarez.app</string>
    <key>CFBundleName</key>
    <string>Squarez</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
</dict>
</plist>
PLIST
fi
