#!/usr/bin/env bash
# dev.sh: run `tandem` from this checkout against a throwaway dev home, so a
# contributor goes from clone to a running board with one command and never
# touches ~/.tandem.
#
# Usage:
#   Scripts/dev.sh                 serve --open on <repo>/.dev-home
#   Scripts/dev.sh serve --port 0  any args pass through to `tandem` verbatim
#   Scripts/dev.sh mcp             the agent face on the same dev home
#   Scripts/dev.sh --watch [args]  rebuild and restart on a change under rust/
#                                  (needs cargo-watch or watchexec)
#
# The dev home is <repo>/.dev-home (gitignored), seeded on first run with a
# copy of Fixtures/port/Board/tandem.db. Delete it to reseed. The committed
# fixture is only ever copied, never run against.
#
# Opt out: when TANDEM_HOME is set, or --home is in the args, that home is used
# and .dev-home is neither created nor seeded. TANDEM_HOME=~/.tandem
# Scripts/dev.sh reaches the real store.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="$root/rust/Cargo.toml"

watch=0
if [[ "${1:-}" == "--watch" ]]; then
  watch=1
  shift
fi
if [[ $# -eq 0 ]]; then
  set -- serve --open
fi

tool=""
if ((watch)); then
  if command -v cargo-watch >/dev/null 2>&1; then
    tool=cargo-watch
  elif command -v watchexec >/dev/null 2>&1; then
    tool=watchexec
  else
    echo "dev.sh: --watch needs cargo-watch (cargo install cargo-watch) or watchexec" >&2
    exit 1
  fi
fi

own_home=0
for arg in "$@"; do
  if [[ "$arg" == "--home" || "$arg" == --home=* ]]; then
    own_home=1
  fi
done
if [[ -n "${TANDEM_HOME:-}" ]]; then
  own_home=1
fi

if ((!own_home)); then
  home="$root/.dev-home"
  if [[ ! -e "$home/tandem.db" ]]; then
    mkdir -p "$home"
    cp "$root/Fixtures/port/Board/tandem.db" "$home/tandem.db"
    chmod u+w "$home/tandem.db"
  fi
  mkdir -p "$home/notes"
  export TANDEM_HOME="$home"
  # The dev home never takes a token or operator key from the caller's shell.
  unset TANDEM_TOKEN TANDEM_OPERATOR_KEY TANDEM_OPERATOR_KEY_NEXT TANDEM_SERVER
  echo "dev.sh: home $home" >&2
fi

cmd=(cargo run --manifest-path "$manifest" -p tandem --)
cmd+=("$@")

if [[ "$tool" == cargo-watch ]]; then
  printf -v line '%q ' "${cmd[@]}"
  exec cargo watch -w "$root/rust" -i "$root/rust/target" -s "$line"
elif [[ "$tool" == watchexec ]]; then
  exec watchexec -r -w "$root/rust" -i "rust/target/**" -- "${cmd[@]}"
fi

exec "${cmd[@]}"
