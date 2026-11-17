#!/bin/bash
# Did this pull request touch anything the gates could read? (CUT-32)
#
# Usage: Scripts/changed-paths.sh <skippable-prefix-or-file>...
#
# Prints `true` or `false`. Outside a pull request it prints `true`: pushes are
# already filtered by `on.push.paths` and a tag or a dispatch always runs
# everything. On a pull request the checkout is the merge commit, so
# `HEAD^1..HEAD` is exactly the change the PR brings. The arguments are an
# allowlist of what may skip the gates (a directory prefix ending in `/`, or an
# exact file): the answer is `false` only when every changed file is on it, so a
# new path runs the gates by default. Some gates read the whole tracked tree
# (the retired-path scan, the lane test), so docs and examples are not on it. Any doubt (no parent, an
# empty diff, a git failure) prints `true`: the filter may only skip work it can
# prove is irrelevant, never a gate it could not read.
set -u

if [ "${GITHUB_EVENT_NAME:-}" != "pull_request" ]; then
  echo true
  exit 0
fi

if ! files=$(git diff --name-only HEAD^1 HEAD 2>/dev/null) || [ -z "$files" ]; then
  echo true
  exit 0
fi

while IFS= read -r file; do
  skippable=false
  for allowed in "$@"; do
    case "$allowed" in
      */) case "$file" in "$allowed"*) skippable=true ;; esac ;;
      *) [ "$file" = "$allowed" ] && skippable=true ;;
    esac
  done
  if [ "$skippable" = false ]; then
    echo true
    exit 0
  fi
done <<EOF2
$files
EOF2
echo false
