#!/bin/bash
# Checks that the README's CLI grammar block is `Usage.swift`'s grammar.
#
# The block (README.md, the ```sh fence that begins `tandem-cli ticket list`)
# is a hand-copy of the usage text in Sources/tandem-cli/Usage.swift, and a
# hand-copy drifts: review gates caught it twice (the deprecated `todo` verb
# taught, `[--archived]` dropped, the `session` group missing). This check
# makes the drift a red CI run instead of a human reading the diff.
#
# What it compares: every grammar entry of the source's usage text must appear
# in the block, in the same order, and the block may not carry an entry the
# source does not have. An entry is one `tandem-cli` command with its indented
# continuation lines joined, or the deprecated-alias notice line. Whitespace is
# normalised, so the block's shallower indent is not drift.
#
# What it deliberately excludes, and where the README carries it instead:
# - the `tandem-cli --help | -h` line — the prose below the block names it
#   ("`tandem-cli --help` prints the whole of it");
# - the `Values:` section — the prose carries the statuses and the session
#   vocabulary; the block is the grammar, not the glossary.
# The alias notice participates like any grammar entry: the block carries it,
# in its source position.
#
# Run by the release workflow's `test` job on every push and pull request.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

usage_file=Sources/tandem-cli/Usage.swift
readme=README.md

die() {
  echo "::error::$1" >&2
  exit 1
}

# The README's grammar block: the ```sh fence whose first line is
# `tandem-cli ticket list`. One match, else the check has nothing to compare.
block=$(awk '
  /^```sh$/ { insh = 1; n = 0; buf = ""; next }
  /^```$/ {
    if (insh && n > 0 && first ~ /^tandem-cli ticket list/) { print buf; found = 1; exit }
    insh = 0
    next
  }
  insh {
    if (n == 0) first = $0
    buf = buf (n++ ? "\n" : "") $0
  }
  END { exit found ? 0 : 1 }
' "$readme") || die "$readme has no sh block that begins with 'tandem-cli ticket list'"

# An entry is a `tandem-cli` command line, or the alias notice line; an
# indented continuation line appends to the entry before it. Whitespace runs
# collapse to one space, so the block's indent cannot mask or fake drift.
normalise() {
  awk '
    {
      line = $0
      gsub(/[ \t]+/, " ", line)
      sub(/^ /, "", line)
      sub(/ $/, "", line)
    }
    line == "" { next }
    line ~ /^tandem-cli / || line ~ /^todo is a deprecated alias/ { entries[++n] = line; next }
    { entries[n] = entries[n] " " line }
    END { for (i = 1; i <= n; i++) print entries[i] }
  '
}

# The source's grammar: the lines of the `text` literal between the `Usage:`
# marker and `Values:`, minus the `--help` line (the prose names it — see the
# header comment). Same normalisation as the block's.
usage_entries=$(awk '
  {
    line = $0
    gsub(/^[ \t]+/, "", line)
    gsub(/[ \t]+$/, "", line)
  }
  line ~ /^static let text = """$/ { intext = 1; next }
  intext && line ~ /^"""$/ { exit }
  intext { print line }
' "$usage_file" | awk '
  /^Usage:$/ { seen = 1; next }
  seen && /^Values:$/ { exit }
  !seen { next }
  /^tandem-cli --help \| -h$/ { next }
  { print }
' | normalise)

[ -n "$usage_entries" ] || die "could not extract the grammar from $usage_file — has \`Usage.text\` moved?"

readme_entries=$(printf '%s\n' "$block" | normalise)

if [ "$usage_entries" != "$readme_entries" ]; then
  {
    echo "::error::$readme's CLI grammar block is not $usage_file's usage text."
    echo "::error::Left side is the source's grammar, right side is the README's block:"
    diff <(printf '%s\n' "$usage_entries") <(printf '%s\n' "$readme_entries") || true
  } >&2
  exit 1
fi

echo "README CLI grammar block matches $usage_file"
