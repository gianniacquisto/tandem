#!/bin/sh
# The publication step's staging half (CUT-16): stamp the installer with the
# tag's version and write the manifest that covers it.
#
# Usage: Scripts/stage-release-assets.sh <version> <dist-dir>
#
# <version> is X.Y.Z without the leading v; <dist-dir> holds the platform
# archives the build jobs produced, `tandem-<version>-darwin-arm64.tar.gz`,
# `tandem-<version>-linux-x86_64.tar.gz` and
# `tandem-<version>-windows-x86_64.zip`. The script copies Scripts/install.sh to
# <dist-dir>/install.sh with its DEFAULT_VERSION set to <version>, then writes
# <dist-dir>/SHA256SUMS with one line per published verifiable asset: every
# `tandem-<version>-*` archive present and the staged script. The release job
# uploads them and `sha256sum -c SHA256SUMS` (or `shasum -a 256 -c`) over dist
# stays the handoff check.
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: stage-release-assets.sh <version> <dist-dir>" >&2
  exit 2
fi
version=$1
dist=$2

if printf '%s' "$version" | grep -Eqv '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "stage-release-assets.sh: the version must be X.Y.Z, got '$version'" >&2
  exit 2
fi

top=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
src="$top/Scripts/install.sh"
if [ ! -f "$src" ]; then
  echo "stage-release-assets.sh: no installer at $src" >&2
  exit 1
fi
if [ ! -d "$dist" ]; then
  echo "stage-release-assets.sh: no dist directory at $dist" >&2
  exit 1
fi
if ! grep -q '^DEFAULT_VERSION="latest"$' "$src"; then
  echo "stage-release-assets.sh: $src has no DEFAULT_VERSION=\"latest\" line to stamp" >&2
  exit 1
fi

found=0
for archive in "$dist"/tandem-"$version"-*; do
  [ -f "$archive" ] || continue
  found=$((found + 1))
done
if [ "$found" -eq 0 ]; then
  echo "stage-release-assets.sh: no tandem-$version-* archive in $dist" >&2
  exit 1
fi

sed "s/^DEFAULT_VERSION=\"latest\"$/DEFAULT_VERSION=\"$version\"/" "$src" > "$dist/install.sh"
chmod +x "$dist/install.sh"

sum256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$@"
  else
    shasum -a 256 "$@"
  fi
}

(
  cd "$dist"
  for archive in tandem-"$version"-*; do
    [ -f "$archive" ] || continue
    sum256 "$archive"
  done
  sum256 install.sh
) > "$dist/SHA256SUMS"
cat "$dist/SHA256SUMS"
