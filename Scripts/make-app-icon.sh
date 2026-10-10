#!/usr/bin/env bash
# Regenerates Resources/AppIcon.icns (and the .iconset sources) from the SF
# Symbol renderer. Run on a Mac with the Xcode Command Line Tools.
#
# Usage: Scripts/make-app-icon.sh
set -euo pipefail
cd "$(dirname "$0")/.."

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
swift Scripts/make-app-icon.swift "$TMP"

ICONSET=Resources/AppIcon.iconset
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
src="$TMP/1024.png"
sips -z 16 16   "$src" --out "$ICONSET/icon_16x16.png"      >/dev/null
sips -z 32 32   "$src" --out "$ICONSET/icon_16x16@2x.png"   >/dev/null
sips -z 32 32   "$src" --out "$ICONSET/icon_32x32.png"      >/dev/null
sips -z 64 64   "$src" --out "$ICONSET/icon_32x32@2x.png"   >/dev/null
sips -z 128 128 "$src" --out "$ICONSET/icon_128x128.png"    >/dev/null
sips -z 256 256 "$src" --out "$ICONSET/icon_128x128@2x.png" >/dev/null
sips -z 256 256 "$src" --out "$ICONSET/icon_256x256.png"    >/dev/null
sips -z 512 512 "$src" --out "$ICONSET/icon_256x256@2x.png" >/dev/null
sips -z 512 512 "$src" --out "$ICONSET/icon_512x512.png"    >/dev/null
cp "$src" "$ICONSET/icon_512x512@2x.png"

iconutil -c icns "$ICONSET" -o Resources/AppIcon.icns
echo "wrote Resources/AppIcon.icns"
