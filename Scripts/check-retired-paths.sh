#!/usr/bin/env bash
#
# Fails when the tracked tree still names a path the cutover retired (RET-91).
#
# The cutover deleted the Swift tree and retired two scripts. A doc, a prompt or
# a comment that still names one sends a reader to a path that no longer exists,
# and PR #283 found that class of rot in review instead of in a check. This
# greps the tracked tree and names every file and line that still mentions a
# retired path, minus the mentions the allowlist below records: the ADRs, the
# port's dated spec, and the provenance notes that point at the git history.
#
# Run it from anywhere: `bash Scripts/check-retired-paths.sh`.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# Every path the cutover retired. `Sources/Tandem` also matches
# `Sources/TandemCore`, and both stay listed because this list is the record.
retired=(
  Sources/Tandem
  Sources/TandemCore
  Sources/tandem-cli
  Tests/TandemCoreTests
  Package.swift
  Scripts/shot.sh
  Scripts/capture-goldens.sh
  Scripts/make-app-bundle.sh
  Scripts/usage-report.sh
)

pattern=$(printf '%s\n' "${retired[@]}" | sed 's/\./\\./g' | paste -sd '|' -)

# The allowlist. Each entry is `file-glob|line-regex|reason`: a hit is allowed
# when its file matches the glob and its line matches the regex. `.*` allows the
# whole file; a regex of `section:<heading>` allows the lines from that heading
# to the next heading of the same or higher level. `*` in a glob crosses `/`, so
# `rust/**/*.rs` is every Rust source under `rust/`. The reason is for the
# reader; the script ignores it.
allow=(
  'docs/adr/*.md|.*|An ADR is the historical record: it names a path as it stood when the decision was made, and its amendments name what retired.'
  'docs/port.md|section:## Where the old code goes|The mapping table is the record of where each old path went. The spec prose around it is live and states the cutover, so it carries no retired path.'
  'rust/**/*.rs|^[[:space:]]*//!|A module doc comment records what the module was ported from, a reference to the git history.'
  'app/tests/*.test.ts|^[[:space:]]*//.*Sources/Tandem[^ ]*\.swift|A ported test cites the Swift file the behaviour came from, a provenance comment: the same class as the Rust module docs.'
  'Editor/build.mjs|^console\.log\("Editor bundle|The bundle log line names the directory the bundle replaced.'
)

# `in_section <file> <line> <heading>`: is `<line>` inside the section the
# heading opens, up to the next heading of the same or higher level?
in_section() {
  local file=$1 line=$2 heading=$3
  local hashes=${heading%%[^#]*}
  local level=${#hashes}
  local start end total
  start=$(grep -n -F -x -- "$heading" "$file" | head -1 | cut -d: -f1 || true)
  [ -n "$start" ] || return 1
  total=$(wc -l <"$file")
  end=$(awk -v start="$start" -v lvl="$level" '
    NR > start && /^#+ / {
      n = 0
      while (substr($0, n + 1, 1) == "#") n++
      if (n <= lvl) { print NR; exit }
    }
  ' "$file")
  [ -n "$end" ] || end=$((total + 1))
  [ "$line" -ge "$start" ] && [ "$line" -lt "$end" ]
}

# A hit is `file:line:text`; `file` cannot contain a colon in a git path we track.
allowed_hits=0
failures=0

while IFS= read -r hit; do
  [ -n "$hit" ] || continue
  file=${hit%%:*}
  rest=${hit#*:}
  line=${rest%%:*}
  text=${rest#*:}

  permitted=0
  for entry in "${allow[@]}"; do
    IFS='|' read -r glob regex reason <<<"$entry"
    # shellcheck disable=SC2053 # the glob is a pattern, not a literal
    [[ "$file" == $glob ]] || continue
    if [[ "$regex" == section:* ]]; then
      if in_section "$file" "$line" "${regex#section:}"; then
        permitted=1
        break
      fi
    elif [[ "$text" =~ $regex ]]; then
      permitted=1
      break
    fi
  done

  if [ "$permitted" -eq 1 ]; then
    allowed_hits=$((allowed_hits + 1))
  else
    if [ "$failures" -eq 0 ]; then
      echo "retired path still named in the tracked tree:" >&2
    fi
    echo "  $hit" >&2
    failures=$((failures + 1))
  fi
done < <(git grep -nI -E "$pattern" -- . ':(exclude)Scripts/check-retired-paths.sh' || true)

if [ "$failures" -gt 0 ]; then
  echo >&2
  echo "$failures mention(s) of a retired path. Name the live path instead, or add" >&2
  echo "the mention to this script's allowlist when it is history that must keep it." >&2
  exit 1
fi

echo "no retired path named in the tracked tree ($allowed_hits historical mention(s) allowed)"
