#!/usr/bin/env bash
# Assembles Tandem.app from two built binaries: Contents/MacOS/Tandem +
# Contents/MacOS/tandem-mcp (the MCP server, the in-bundle path agent configs
# reference) + Contents/Info.plist (versioned) + Contents/Resources/AppIcon.icns +
# Contents/Resources/TandemBike.jpg (brand photo, ticket 03), ad-hoc signed. The
# release workflow runs this to assemble the release artifact; the cask downloads
# it (ADR 0003).
#
# Usage: Scripts/make-app-bundle.sh <built-Tandem-binary> <built-tandem-mcp-binary> <version> <output-dir>
# Produces <output-dir>/Tandem.app
set -euo pipefail

usage() { echo "usage: $0 <built-Tandem-binary> <built-tandem-mcp-binary> <version> <output-dir>" >&2; exit 2; }
[ $# -eq 4 ] || usage
binary=$1
mcp_binary=$2
version=$3
outdir=$4
[ -x "$binary" ] || { echo "error: $binary is not an executable" >&2; exit 1; }
[ -x "$mcp_binary" ] || { echo "error: $mcp_binary is not an executable" >&2; exit 1; }

root=$(cd "$(dirname "$0")/.." && pwd)
app="$outdir/Tandem.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp "$binary" "$app/Contents/MacOS/Tandem"
cp "$mcp_binary" "$app/Contents/MacOS/tandem-mcp"
sed "s/@@VERSION@@/$version/g" "$root/Resources/Tandem-Info.plist" > "$app/Contents/Info.plist"
cp "$root/Resources/AppIcon.icns" "$app/Contents/Resources/AppIcon.icns"
cp "$root/Resources/TandemBike.jpg" "$app/Contents/Resources/TandemBike.jpg"

# Ad-hoc signature: all Apple Silicon requires for a locally built binary.
codesign --force --sign - "$app"

# Sanity-check the bundle before it ships anywhere.
codesign --verify --strict "$app"
plutil -lint "$app/Contents/Info.plist" >/dev/null
echo "assembled $app"
