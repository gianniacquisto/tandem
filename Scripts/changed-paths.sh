#!/bin/bash
# Did this pull request touch a path the gates read? (CUT-32)
#
# Usage: Scripts/changed-paths.sh <path-prefix>...
#
# Prints `true` or `false`. Outside a pull request it prints `true`: pushes are
# already filtered by `on.push.paths` and a tag or a dispatch always runs
# everything. On a pull request the checkout is the merge commit, so
# `HEAD^1..HEAD` is exactly the change the PR brings. Any doubt (no parent, an
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
  for prefix in "$@"; do
    case "$file" in
      "$prefix"*) echo true; exit 0 ;;
    esac
  done
done <<EOF2
$files
EOF2
echo false
