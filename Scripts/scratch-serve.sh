#!/usr/bin/env bash
# scratch-serve.sh: run `tandem serve` from this checkout against a throwaway
# copy of a committed fixture home, for an agent (or a human) to look at the
# board without touching ~/.tandem.
#
# It builds the `tandem` binary, copies the fixture's `tandem.db` into a fresh
# scratch home, and runs the server in the foreground with a verbose log on
# stdout. `.claude/launch.json` runs it, so the in-app browser's `preview_start`
# gets the board and `preview_logs` gets the server log.
#
# Usage:
#   Scripts/scratch-serve.sh
#
# Environment (all optional):
#   SCRATCH_FIXTURE  Board (default, 78 tickets), Home (2 tickets) or empty
#   SCRATCH_HOME     reuse this home instead of a fresh one; seeded only when
#                    it holds no tandem.db, so its state survives a restart
#   SCRATCH_PORT     port to bind on 127.0.0.1 (default 7790, clear of 7700)
#   TANDEM_LOG       log filter, RUST_LOG syntax (default info,tandem_core=debug)

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture="${SCRATCH_FIXTURE:-Board}"
port="${SCRATCH_PORT:-7790}"

case "$fixture" in
  Board | Home | empty) ;;
  *)
    echo "scratch-serve: SCRATCH_FIXTURE must be Board, Home or empty, not '$fixture'" >&2
    exit 2
    ;;
esac

if [[ -n "${SCRATCH_HOME:-}" ]]; then
  home="$SCRATCH_HOME"
  mkdir -p "$home"
else
  home="$(mktemp -d "${TMPDIR:-/tmp}/tandem-scratch.XXXXXX")/Home"
  mkdir -p "$home"
fi

if [[ "$fixture" != "empty" && ! -e "$home/tandem.db" ]]; then
  cp "$root/Fixtures/port/$fixture/tandem.db" "$home/tandem.db"
  chmod u+w "$home/tandem.db"
fi
mkdir -p "$home/notes"

cargo build --quiet --manifest-path "$root/rust/Cargo.toml" --bin tandem

echo "scratch-serve: home $home (fixture $fixture)"
export TANDEM_HOME="$home"
export TANDEM_LOG="${TANDEM_LOG:-info,tandem_core=debug}"
# The scratch server never takes a token or an operator key from the caller's
# shell: it is loopback only and must not reach a real deployment's secrets.
unset TANDEM_TOKEN TANDEM_OPERATOR_KEY TANDEM_OPERATOR_KEY_NEXT TANDEM_SERVER
exec "$root/rust/target/debug/tandem" serve --host 127.0.0.1 --port "$port"
