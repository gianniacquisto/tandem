#!/bin/sh
# The archive's smoke test (CUT-16): the one binary answers its version and
# serves the board from an ephemeral port with no external files.
#
# Usage: Scripts/smoke-binary.sh <tandem-binary> <expected-version>
#
# Run by `release.yml`'s macOS and Linux build jobs against the binary that was
# just packaged, and usable by hand after `cargo build --release -p tandem`:
#
#   Scripts/smoke-binary.sh rust/target/release/tandem 0.1.0
#
# It asserts three things a missing asset or a broken route would break:
# `tandem --version` names the release stamp, `GET /` and `GET /notes` answer
# 200, and every answer carries the server's Content-Security-Policy header.
# The home is a throwaway directory under the system temp dir and is removed on
# every exit path.
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: smoke-binary.sh <tandem-binary> <expected-version>" >&2
  exit 2
fi
binary=$1
want=$2

if [ ! -x "$binary" ]; then
  echo "smoke-binary.sh: no executable binary at $binary" >&2
  exit 1
fi

printed=$("$binary" --version)
case "$printed" in
  *"$want"*) ;;
  *)
    echo "smoke-binary.sh: --version printed '$printed', which does not name '$want'" >&2
    exit 1
    ;;
esac

scratch=$(mktemp -d)
pid=
cleanup() {
  if [ -n "$pid" ]; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  rm -rf "$scratch"
}
trap cleanup EXIT HUP INT TERM

"$binary" serve --port 0 --home "$scratch/home" >"$scratch/serve.log" 2>&1 &
pid=$!

address=
for _ in $(seq 1 100); do
  address=$(grep -o 'http://[0-9][0-9.]*:[0-9][0-9]*' "$scratch/serve.log" 2>/dev/null | head -1 || true)
  [ -n "$address" ] && break
  if ! kill -0 "$pid" 2>/dev/null; then
    cat "$scratch/serve.log" >&2
    echo "smoke-binary.sh: the server exited before it listened" >&2
    exit 1
  fi
  sleep 0.1
done
if [ -z "$address" ]; then
  cat "$scratch/serve.log" >&2
  echo "smoke-binary.sh: the server never printed its address" >&2
  exit 1
fi

for path in / /notes; do
  status=$(curl -fsS -o /dev/null -w '%{http_code}' "$address$path")
  if [ "$status" != "200" ]; then
    echo "smoke-binary.sh: GET $path answered $status, not 200" >&2
    exit 1
  fi
  if ! curl -fsS -D "$scratch/headers" -o /dev/null "$address$path"; then
    echo "smoke-binary.sh: GET $path failed" >&2
    exit 1
  fi
  if ! grep -qi '^content-security-policy:' "$scratch/headers"; then
    echo "smoke-binary.sh: GET $path carries no Content-Security-Policy header" >&2
    exit 1
  fi
done

echo "smoke-binary.sh: $binary answered $want, served / and /notes with a CSP on $address"
