#!/usr/bin/env bash
# make-fixture-home.sh: regenerate the fixture home's store from its seed.
#
# `Fixtures/port/Home` is content, not scratch: `tandem.db` is committed and no
# run writes into it. This script is its one writer, and
# `Fixtures/port/tandem.seed.json` is the seed: every session id, ticket id and
# timestamp is in the file and the generator reads no clock, so the output is
# byte for byte the same on every machine.
#
# `rust/core/tests/fixture_db.rs` runs the same generator into a temp path and
# fails when the committed file disagrees, which is what proves the artifact is
# what this script says. Run this after changing the seed, and commit the file
# it writes.
#
# Usage:
#   Scripts/make-fixture-home.sh

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)

cd "$root/rust"
cargo run --quiet --package tandem-core --bin make-fixture-home -- \
  "$root/Fixtures/port/tandem.seed.json" \
  "$root/Fixtures/port/Home/tandem.db"

echo "wrote Fixtures/port/Home/tandem.db"
