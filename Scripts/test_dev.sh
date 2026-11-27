#!/usr/bin/env bash
# Tests for Scripts/dev.sh (TAN-455). Each case runs the script as a
# contributor calls it, from a scratch copy of the repo root (the script, the
# Board fixture and a symlink to rust/), so the suite never touches a
# contributor's own .dev-home. Run: `bash Scripts/test_dev.sh`.
set -u

repo=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
# A scratch HOME must not hide the toolchain: pin cargo and rustup to the real ones.
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"

pass=0
fail=0
ok() { pass=$((pass + 1)); echo "ok: $1"; }
no() { fail=$((fail + 1)); echo "NOT OK: $1" >&2; }

tmp=$(mktemp -d)
cleanup() { stop_serve; rm -rf "$tmp"; }
trap cleanup EXIT

# fake_root <dir>: a scratch root whose dev.sh resolves to it.
fake_root() {
  mkdir -p "$1/Scripts" "$1/Fixtures/port"
  cp "$repo/Scripts/dev.sh" "$1/Scripts/dev.sh"
  cp -R "$repo/Fixtures/port/Board" "$1/Fixtures/port/Board"
  ln -s "$repo/rust" "$1/rust"
}

served_port=""
# stop_serve: kills whatever listens on the port the last case served on.
stop_serve() {
  [[ -n "$served_port" ]] || return 0
  local pid
  pid=$(lsof -ti "tcp:$served_port" -sTCP:LISTEN 2>/dev/null)
  [[ -n "$pid" ]] && kill $pid 2>/dev/null
  served_port=""
}

# start_serve <root> <log> [env args]: runs dev.sh serve --port 0 in the
# background and prints the address once the log shows it.
start_serve() {
  local root=$1 log=$2
  shift 2
  (env "$@" bash "$root/Scripts/dev.sh" serve --port 0 >"$log" 2>&1 &)
  local i addr
  for i in $(seq 1 600); do
    addr=$(grep -o 'http://127.0.0.1:[0-9]*' "$log" 2>/dev/null | head -1)
    if [[ -n "$addr" ]]; then
      served_port=${addr##*:}
      echo "$addr"
      return 0
    fi
    sleep 0.5
  done
  return 1
}

# Case 1: default home, real HOME untouched.
r1=$tmp/r1; fake_root "$r1"; h1=$tmp/home1; mkdir -p "$h1"
addr=$(start_serve "$r1" "$tmp/log1" -u TANDEM_HOME HOME="$h1")
served_port=${addr##*:}
code=$(curl -sL -o /dev/null -w '%{http_code}' "$addr/" 2>/dev/null)
if [[ -n "$addr" && "$code" == 200 && -f "$r1/.dev-home/tandem.db" && ! -e "$h1/.tandem" ]]; then
  ok "default run serves a seeded .dev-home and leaves HOME/.tandem alone"
else
  no "default run (addr=$addr code=$code)"; cat "$tmp/log1" >&2
fi
stop_serve

# Case 2: TANDEM_HOME opt-out.
r2=$tmp/r2; fake_root "$r2"; h2=$tmp/home2; mkdir -p "$h2"
addr=$(start_serve "$r2" "$tmp/log2" HOME="$h1" TANDEM_HOME="$h2")
served_port=${addr##*:}
code=$(curl -sL -o /dev/null -w '%{http_code}' "$addr/" 2>/dev/null)
if [[ "$code" == 200 && ! -e "$r2/.dev-home" ]]; then
  ok "TANDEM_HOME is used and .dev-home is not created"
else
  no "opt-out (addr=$addr code=$code)"; cat "$tmp/log2" >&2
fi
stop_serve

# Case 3: seeding copies the fixture and leaves it byte-identical.
if cmp -s "$r1/.dev-home/tandem.db" "$repo/Fixtures/port/Board/tandem.db" \
  && git -C "$repo" diff --exit-code --quiet -- Fixtures/port/Board/tandem.db \
  && [[ ! -e "$r1/.dev-home/README.md" ]]; then
  ok "seeding copies the fixture and leaves it byte-identical"
else
  no "seed differs from the fixture, or the fixture changed"
fi

# Case 4: --watch with neither tool on PATH.
out=$(PATH=/usr/bin:/bin bash "$repo/Scripts/dev.sh" --watch 2>&1); st=$?
if [[ $st -eq 1 && "$out" == *"cargo install cargo-watch"* ]]; then
  ok "--watch without a watcher exits 1 with the hint"
else
  no "--watch hint (status $st: $out)"
fi

echo "$pass passed, $fail failed"
[[ $fail -eq 0 ]]
