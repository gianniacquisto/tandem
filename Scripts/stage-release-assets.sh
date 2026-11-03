#!/bin/sh
# The publication step's staging half (TAN-253): stamp the installer with the
# tag's version and write the manifest that covers every asset the release
# attaches.
#
# Usage: Scripts/stage-release-assets.sh <version> <dist-dir>
#
# <version> is X.Y.Z without the leading v; <dist-dir> holds the packaged
# Tandem-<version>.zip and Tandem-<version>.dmg (electron-builder's mac
# targets always produce both, so a missing dmg fails loudly, like the zip).
# The script copies Scripts/install.sh to <dist-dir>/install.sh with its
# DEFAULT_VERSION set to <version>, then writes <dist-dir>/SHA256SUMS with
# one line per published verifiable asset: the zip, the dmg and the staged
# script. The release job uploads all of them, and
# `shasum -a 256 -c SHA256SUMS` over dist stays the handoff check, now over
# every asset it publishes. Scripts/bump-tap.sh reads exactly the zip's line,
# so the extra lines change nothing for the cask.
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: stage-release-assets.sh <version> <dist-dir>" >&2
  exit 2
fi
version=$1
dist=$2

case "$version" in
  *[!0-9.]*|*.*.*) : ;;
  *) echo "stage-release-assets.sh: the version must be X.Y.Z, got '$version'" >&2; exit 2 ;;
esac
if printf '%s' "$version" | grep -Eqv '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "stage-release-assets.sh: the version must be X.Y.Z, got '$version'" >&2
  exit 2
fi

top=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
src="$top/Scripts/install.sh"
zip="$dist/Tandem-$version.zip"
dmg="$dist/Tandem-$version.dmg"
if [ ! -f "$src" ]; then
  echo "stage-release-assets.sh: no installer at $src" >&2
  exit 1
fi
if [ ! -f "$zip" ]; then
  echo "stage-release-assets.sh: no zip at $zip" >&2
  exit 1
fi
if [ ! -f "$dmg" ]; then
  echo "stage-release-assets.sh: no dmg at $dmg" >&2
  exit 1
fi
if ! grep -q '^DEFAULT_VERSION="latest"$' "$src"; then
  echo "stage-release-assets.sh: $src has no DEFAULT_VERSION=\"latest\" line to stamp" >&2
  exit 1
fi

sed "s/^DEFAULT_VERSION=\"latest\"$/DEFAULT_VERSION=\"$version\"/" "$src" > "$dist/install.sh"
chmod +x "$dist/install.sh"
(cd "$dist" && shasum -a 256 "Tandem-$version.zip" "Tandem-$version.dmg" install.sh > SHA256SUMS)
cat "$dist/SHA256SUMS"
