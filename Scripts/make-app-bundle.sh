#!/usr/bin/env bash
# Assembles Tandem.app from a built binary: Contents/MacOS/Tandem +
# Contents/Info.plist (versioned) + Contents/Resources/AppIcon.icns, ad-hoc
# signed. The Homebrew formula runs this and installs the bundle into the
# Cellar; the cask then links it into /Applications (ADR 0002).
#
# Usage: Scripts/make-app-bundle.sh <built-Tandem-binary> <version> <output-dir>
# Produces <output-dir>/Tandem.app
set -euo pipefail

usage() { echo "usage: $0 <built-Tandem-binary> <version> <output-dir>" >&2; exit 2; }
[ $# -eq 3 ] || usage
binary=$1
version=$2
outdir=$3
[ -x "$binary" ] || { echo "error: $binary is not an executable" >&2; exit 1; }

root=$(cd "$(dirname "$0")/.." && pwd)
app="$outdir/Tandem.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp "$binary" "$app/Contents/MacOS/Tandem"
sed "s/@@VERSION@@/$version/g" "$root/Resources/Tandem-Info.plist" > "$app/Contents/Info.plist"
cp "$root/Resources/AppIcon.icns" "$app/Contents/Resources/AppIcon.icns"

# Ad-hoc signature: all Apple Silicon requires for a locally built binary.
codesign --force --sign - "$app"

# Sanity-check the bundle before it ships anywhere.
codesign --verify --strict "$app"
plutil -lint "$app/Contents/Info.plist" >/dev/null
echo "assembled $app"
