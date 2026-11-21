#!/usr/bin/env bash
#
# Tests Scripts/check-prose-dash.sh in a hermetic temp repo (TAN-357).
# Run: `bash Scripts/test_check_prose_dash.sh`.

set -uo pipefail

script="$(cd "$(dirname "$0")" && pwd)/check-prose-dash.sh"
dash="$(printf '\342\200\224')"
pass=0
fail=0
ok() { pass=$((pass + 1)); echo "ok: $1"; }
no() { fail=$((fail + 1)); echo "no: $1"; }

repo="$(mktemp -d)"
trap 'rm -rf "$repo"' EXIT
cd "$repo"
git init -q
git config user.email t@t
git config user.name t
mkdir -p rust/core/src/tests/fixtures
printf 'one\nold %s dash\nthree\n' "$dash" > a.md
git add -A && git commit -qm base
base="$(git rev-parse HEAD)"

run() { out="$(bash "$script" "$@" 2>&1)"; code=$?; }

# 1: an added dash fails and names file:line and text
printf 'one\nold %s dash\nthree\nnew %s line\n' "$dash" "$dash" > a.md
git commit -qam add
run "$base..HEAD"
if [ "$code" -eq 1 ] && [[ "$out" == *"a.md:4: new $dash line"* ]]; then ok "added dash fails with file:line"; else no "added dash fails ($code: $out)"; fi

# 2: a dash only as context passes
printf 'one\nold %s dash\nthree\nclean\n' "$dash" > a.md
git commit -qam clean
head1="$(git rev-parse HEAD)"
git checkout -q "$base"; printf 'one\nold %s dash\nthree\nclean\n' "$dash" > a.md
git commit -qam ctx
run "$base..HEAD"
if [ "$code" -eq 0 ]; then ok "context dash passes"; else no "context dash ($code: $out)"; fi

# 3: a removed dash line passes
printf 'one\nthree\n' > a.md
git commit -qam remove
run "HEAD^..HEAD"
if [ "$code" -eq 0 ]; then ok "removed dash passes"; else no "removed dash ($code: $out)"; fi

# 4: an excluded fixture path passes
printf 'x %s y\n' "$dash" > rust/core/src/tests/fixtures/p.json
git add -A && git commit -qm fixture
run "HEAD^..HEAD"
if [ "$code" -eq 0 ]; then ok "fixture path excluded"; else no "fixture path ($code: $out)"; fi

# 5: a bad range exits 2
run "nope..HEAD"
if [ "$code" -eq 2 ]; then ok "bad range exits 2"; else no "bad range ($code: $out)"; fi

echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
