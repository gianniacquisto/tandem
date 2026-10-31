#!/usr/bin/env bash
# make-fixture-home.sh: regenerate a committed port fixture store from its seed.
#
# Two homes are committed content, not scratch: `Fixtures/port/Home` (the
# contract home both implementations read) and `Fixtures/port/Board` (the
# 78-ticket board the Tickets face's pictures are shot on). Each `tandem.db` is
# committed and no run writes into it. This script is their one writer, and the
# seed beside each home is the source: every session id, ticket id and timestamp
# is in the file and the generator reads no clock, so the output is byte for
# byte the same on every machine.
#
# `rust/core/tests/fixture_db.rs` runs the same generator into a temp path and
# fails when either committed file disagrees, which is what proves the artifacts
# are what this script says. Run this after changing a seed, and commit the file
# it writes.
#
# Usage:
#   Scripts/make-fixture-home.sh
#   Scripts/make-fixture-home.sh Fixtures/port/board.seed.json Fixtures/port/Board/tandem.db

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
seed=${1:-"$root/Fixtures/port/tandem.seed.json"}
database=${2:-"$root/Fixtures/port/Home/tandem.db"}
[[ $seed = /* ]] || seed="$root/$seed"
[[ $database = /* ]] || database="$root/$database"

cd "$root/rust"
cargo run --quiet --package tandem-core --bin make-fixture-home -- "$seed" "$database"

echo "wrote ${database#"$root/"}"
