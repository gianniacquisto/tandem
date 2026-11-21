#!/usr/bin/env bash
#
# Fails when a range adds a line carrying an em dash, U+2014 (TAN-357).
#
# AGENTS.md bans U+2014 in anything an agent writes. Only added lines gate: the
# tree holds dashes the rule says stay, so a dash already there never fails a
# commit that did not add it.
#
# Usage: `bash Scripts/check-prose-dash.sh [<range>]`. The range defaults to
# `HEAD^1..HEAD` when GITHUB_EVENT_NAME is set (CI checks out the merge commit
# with fetch-depth 2), else `origin/main...HEAD`. Exit 0 clean, 1 on a hit,
# 2 when the range cannot be read: a gate that cannot read must not pass.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if [ "$#" -ge 1 ]; then
  range="$1"
elif [ -n "${GITHUB_EVENT_NAME:-}" ]; then
  range="HEAD^1..HEAD"
else
  range="origin/main...HEAD"
fi

# Built from bytes so this file holds no literal dash.
dash="$(printf '\342\200\224')"

# Excluded: captured harness payloads quote external text verbatim.
if ! diff_out="$(git diff --unified=0 --no-color --no-ext-diff "$range" -- . \
  ':(exclude)rust/core/src/tests/fixtures/' 2>&1)"; then
  echo "check-prose-dash: cannot read range '$range': $diff_out" >&2
  exit 2
fi

hits=0
file=""
line=0
while IFS= read -r row; do
  case "$row" in
    '+++ b/'*) file="${row#+++ b/}" ;;
    '+++ '*) file="" ;;
    '@@ '*)
      rest="${row#*+}"
      line="${rest%% *}"
      line="${line%%,*}"
      ;;
    '+'*)
      if [[ "$row" == *"$dash"* ]]; then
        echo "$file:$line: ${row#+}" >&2
        hits=$((hits + 1))
      fi
      line=$((line + 1))
      ;;
  esac
done <<< "$diff_out"

if [ "$hits" -gt 0 ]; then
  echo "check-prose-dash: $hits added line(s) carry an em dash; use a comma, a colon, a full stop or a hyphen." >&2
  exit 1
fi
echo "check-prose-dash: no added line in $range carries an em dash."
