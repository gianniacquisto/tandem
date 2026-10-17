#!/usr/bin/env bash
# Assembles Tandem.app from two built binaries: Contents/MacOS/Tandem +
# Contents/MacOS/tandem-cli (the agent-facing CLI, the in-bundle path agent
# configs reference) + Contents/Info.plist (versioned) + Contents/Resources/AppIcon.icns +
# Contents/Resources/TandemBike.jpg (brand photo, ticket 03) +
# Contents/Resources/tandem_Tandem.bundle (the editor page, ticket 08) +
# Contents/Resources/NOTICE.txt (upstream license notice), ad-hoc signed. The
# release workflow runs this to assemble the release artifact; the cask downloads
# it (ADR 0003).
#
# Usage: Scripts/make-app-bundle.sh <built-Tandem-binary> <built-tandem-cli-binary> <version> <output-dir>
# Produces <output-dir>/Tandem.app
set -euo pipefail

usage() { echo "usage: $0 <built-Tandem-binary> <built-tandem-cli-binary> <version> <output-dir>" >&2; exit 2; }
[ $# -eq 4 ] || usage
binary=$1
cli_binary=$2
version=$3
outdir=$4
[ -x "$binary" ] || { echo "error: $binary is not an executable" >&2; exit 1; }
[ -x "$cli_binary" ] || { echo "error: $cli_binary is not an executable" >&2; exit 1; }

root=$(cd "$(dirname "$0")/.." && pwd)
app="$outdir/Tandem.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp "$binary" "$app/Contents/MacOS/Tandem"
cp "$cli_binary" "$app/Contents/MacOS/tandem-cli"
sed "s/@@VERSION@@/$version/g" "$root/Resources/Tandem-Info.plist" > "$app/Contents/Info.plist"
cp "$root/Resources/AppIcon.icns" "$app/Contents/Resources/AppIcon.icns"
cp "$root/Resources/TandemBike.jpg" "$app/Contents/Resources/TandemBike.jpg"
cp "$root/NOTICE.txt" "$app/Contents/Resources/NOTICE.txt"

# The editor bundle (ticket 08): `swift build` emits the SwiftPM resource
# bundle next to the binary; in the app, Bundle.module looks it up under
# Contents/Resources/ (that is the release-build candidate in the generated
# resource_bundle_accessor).
bundle="$(dirname "$binary")/tandem_Tandem.bundle"
[ -d "$bundle" ] || { echo "error: $bundle is missing (the build did not emit the editor resource bundle)" >&2; exit 1; }
cp -R "$bundle" "$app/Contents/Resources/tandem_Tandem.bundle"

# Ad-hoc signature: all Apple Silicon requires for a locally built binary.
codesign --force --sign - "$app"

# Sanity-check the bundle before it ships anywhere.
codesign --verify --strict "$app"
plutil -lint "$app/Contents/Info.plist" >/dev/null
echo "assembled $app"
