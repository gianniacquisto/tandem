#!/bin/sh
# Pin the Claude Code plugin to a release (ADR 0051).
#
# Usage: Scripts/pin-plugin.sh <version>
#
# <version> is X.Y.Z without the leading v. The script checks that the tag
# v<version> exists on origin, then rewrites the plugin entry's `ref` in
# .claude-plugin/marketplace.json to it. Run it after the tag is pushed, and
# land the change on main: Claude Code reads the marketplace from main and
# fetches the plugin from the tag, so a ref that names a tag not yet pushed
# would break every install until it is.
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: pin-plugin.sh <version>" >&2
  exit 2
fi
version=$1

if printf '%s' "$version" | grep -Eqv '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "pin-plugin.sh: version must be X.Y.Z, got '$version'" >&2
  exit 2
fi
tag="v$version"

root=$(git rev-parse --show-toplevel)
manifest="$root/.claude-plugin/marketplace.json"

if ! git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1; then
  echo "pin-plugin.sh: tag $tag is not on origin; push it first" >&2
  exit 1
fi

count=$(grep -c '"ref": "[^"]*"' "$manifest" || true)
if [ "$count" -ne 1 ]; then
  echo "pin-plugin.sh: expected one \"ref\" in $manifest, found $count" >&2
  exit 1
fi

tmp="$manifest.tmp"
sed "s/\"ref\": \"[^\"]*\"/\"ref\": \"$tag\"/" "$manifest" >"$tmp"
mv "$tmp" "$manifest"
echo "pinned the plugin to $tag"
