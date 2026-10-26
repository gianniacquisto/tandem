#!/bin/sh
# The GitHub release body (7B005267): what shipped, not a fixed tag line.
# Usage: Scripts/release-notes.sh <new-tag> — run from a checkout that has
# the tags. Prints the body to stdout: the fixed lead line plus a
# "## What shipped" list of the PRs merged on the integration branch since
# the previous tag. PR titles come from `gh` when it is authed; without it
# the bullet falls back to the merge subject's branch name.
set -eu

new=${1:?usage: release-notes.sh <new-tag>}
version=${new#v}

# The previous release tag: the nearest tag behind the new one.
prev=$(git describe --tags --abbrev=0 "$new^" 2>/dev/null || true)
if [ -z "$prev" ]; then
  echo "release-notes.sh: no tag precedes $new" >&2
  exit 1
fi

lead="Tandem $version for macOS 27 (Apple silicon). Install: brew install --cask gianniacquisto/tap/tandem"

# The integration branch's own merges between the tags: the PR merges.
# --first-parent keeps the union merges a PR branch carries when the
# integration branch moves under it ("Merge dev into board/…") out of the
# list — they are not something the release ships.
subjects=$(git log "$prev..$new" --first-parent --merges --format='%s')

if [ -z "$subjects" ]; then
  printf '%s\n' "$lead"
  exit 0
fi

printf '%s\n\n## What shipped\n\n' "$lead"
printf '%s\n' "$subjects" | while IFS= read -r subject; do
  [ -n "$subject" ] || continue
  case "$subject" in
    "Merge pull request #"*" from "*)
      num=${subject#*#}; num=${num%% *}
      branch=${subject##*from }
      branch=${branch#*/}
      title=""
      gh_cmd=${RELEASE_NOTES_GH:-gh}
      if command -v "$gh_cmd" >/dev/null 2>&1; then
        title=$("$gh_cmd" pr view "$num" --json title --jq .title 2>/dev/null || true)
      fi
      if [ -n "$title" ]; then
        printf -- '- #%s %s\n' "$num" "$title"
      else
        printf -- '- #%s (%s)\n' "$num" "$branch"
      fi
      ;;
    *)
      printf -- '- %s\n' "$subject"
      ;;
  esac
done
