#!/usr/bin/env bash
# Scripts/shot.sh — the one screenshot tool, and the rule itself.
#
# Photographs a window of the running Tandem app in process: it compiles
# Scripts/shot.m into a dylib, injects it into the app with lldb (dlopen +
# call), and the app renders its own window with cacheDisplay — `screencapture`
# has no Screen Recording permission on this Mac, so the app photographs
# itself instead (docs/screenshots/README.md).
#
# The rule (docs/screenshots/README.md): every picture is shot against a
# fixture home. The script refuses to run when TANDEM_HOME is unset, resolves
# to a real home (~/.tandem, ~/.tandem-dev), or points anywhere outside this
# repo's own fixtures — and it refuses an app whose own environment is not
# pointed at the same fixture home, because the camera shoots whatever the
# app is showing, not what the harness says. A refusal exits non-zero and
# writes nothing: no PNG, no dylib, no log.
#
# Usage:
#   TANDEM_HOME=Fixtures/screenshots/Home Scripts/shot.sh list
#       list the running app's windows (id, size, title) — pick one.
#   TANDEM_HOME=Fixtures/screenshots/Home Scripts/shot.sh <window-id> <output.png>
#       photograph window <window-id> into <output.png>.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd -P)"
fixtures="$root/Fixtures"

die() {
  local code="$1"
  shift
  printf 'shot: %s\n' "$*" >&2
  exit "$code"
}

# resolve_path P — expand ~, make absolute, and resolve symlinks of the part
# that exists. A fixture home may not exist yet (the harness seeds it), so the
# tail of a non-existent path is kept verbatim.
resolve_path() {
  local p="$1" tail="" abs
  case "$p" in
    "~") p="$HOME" ;;
    "~"/*) p="$HOME/${p#\~}" ;;
  esac
  case "$p" in
    /*) ;;
    *) p="$PWD/$p" ;;
  esac
  abs="$p"
  while [ ! -e "$abs" ]; do
    tail="/$(basename "$abs")$tail"
    abs="$(dirname "$abs")"
    [ "$abs" = "/" ] && break
  done
  if abs="$(cd "$abs" 2>/dev/null && pwd -P)"; then
    if [ "$abs" = "/" ]; then abs=""; fi
    printf '%s%s\n' "$abs" "$tail"
  else
    printf '%s\n' "$p" # unresolvable: use as written; the checks below fail closed
  fi
}

# The gate. Refuses any home that is not the repo's fixture home: exits
# non-zero, writes nothing.
require_fixture_home() {
  local raw="${TANDEM_HOME-}" resolved real real_resolved
  if [ -z "$raw" ]; then
    die 1 "TANDEM_HOME is not set. Pictures are shot against a fixture home (docs/screenshots/README.md) — set it to a home under $fixtures"
  fi
  resolved="$(resolve_path "$raw")"
  for real in "$HOME/.tandem" "$HOME/.tandem-dev"; do
    real_resolved="$(resolve_path "$real")"
    if [ "$resolved" = "$real_resolved" ]; then
      die 1 "TANDEM_HOME=$raw resolves to $resolved — a real home. Refused: shoot the fixture home under $fixtures"
    fi
  done
  case "$resolved/" in
    "$fixtures"/*) ;;
    *)
      die 1 "TANDEM_HOME=$raw resolves to $resolved — outside this repo's fixtures ($fixtures). Refused: every picture comes from a fixture home"
      ;;
  esac
  FIXTURE_HOME="$resolved"
}

# The Tandem processes running on this Mac, one pid per line.
tandem_pids() {
  pgrep -x Tandem || true
}

# The TANDEM_HOME value in one pid's environment, empty when the app resolved
# its own home (a real one, by ADR 0001 — the shipped app's ~/.tandem or the
# dev build's ~/.tandem-dev).
app_tandem_home() {
  ps eww -p "$1" 2>/dev/null | tr ' ' '\n' | grep '^TANDEM_HOME=' | cut -d= -f2- || true
}

# Pick the one running app that is pointed at the fixture home. The camera
# shoots what the app shows, so the app's own environment is what counts —
# a harness pointed at a fixture home can still be photographing an app
# whose real vault is on screen (the exact hole the fixture rule closes).
find_app() {
  local pids p vals app_home matches=0 pid=""
  pids="$(tandem_pids)"
  [ -n "$pids" ] || die 1 "no running Tandem app — launch it against the fixture home first (TANDEM_HOME=$FIXTURE_HOME <Tandem> &)"
  while IFS= read -r p; do
    [ -n "$p" ] || continue
    vals="$(app_tandem_home "$p")"
    if [ "$(printf '%s\n' "$vals" | grep -c . || true)" -eq 1 ]; then
      app_home="$(resolve_path "$vals")"
      if [ "$app_home" = "$FIXTURE_HOME" ]; then
        matches=$((matches + 1))
        pid="$p"
      fi
    fi
  done <<<"$pids"
  if [ "$matches" -eq 0 ]; then
    die 1 "no running Tandem app is pointed at the fixture home ($FIXTURE_HOME). Launch it with TANDEM_HOME=$FIXTURE_HOME — the script refuses to photograph an app whose home it cannot verify"
  elif [ "$matches" -gt 1 ]; then
    die 1 "$matches running Tandem apps point at the fixture home — quit all but one before shooting"
  fi
  APP_PID="$pid"
}

[ "$#" -ge 1 ] || die 2 "usage: Scripts/shot.sh list  |  Scripts/shot.sh <window-id> <output.png>"

mode="$1"
case "$mode" in
  list)
    [ "$#" -eq 1 ] || die 2 "usage: Scripts/shot.sh list"
    ;;
  *)
    [ "$#" -eq 2 ] || die 2 "usage: Scripts/shot.sh <window-id> <output.png>  (or: Scripts/shot.sh list)"
    window_id="$1"
    out="$2"
    case "$window_id" in
      '' | *[!0-9]*) die 2 "window id must be a number — Scripts/shot.sh list to find one" ;;
    esac
    ;;
esac

require_fixture_home
find_app

tmp="$(mktemp -d "${TMPDIR:-/tmp}/tandem-shot.XXXXXX")" || die 1 "mktemp failed"
trap 'rm -rf "$tmp"' EXIT

if [ "$mode" = list ]; then
  tab="$(printf '\t')"
  printf 'shot: windows of pid %s (fixture home %s)\n' "$APP_PID" "$FIXTURE_HOME"
  # The rows come back as the expression's value (`-O`), not as printf from the
  # app: on attach lldb does not own the app's stdout, so anything the injected
  # code prints lands wherever the app was launched from, never in this log.
  lldb --batch --no-lldbinit -p "$APP_PID" \
    -o "expr -- @import AppKit;" \
    -o "expr -O -- ({ NSMutableArray *rows = [NSMutableArray array]; for (NSWindow *w in [(NSApplication *)NSApp windows]) [rows addObject:[NSString stringWithFormat:@\"%ld\\t%.0f\\t%.0f\\t%@\", (long)w.windowNumber, w.frame.size.width, w.frame.size.height, w.title ? w.title : @\"(untitled)\"]]; [rows componentsJoinedByString:@\"\\n\"]; })" \
    -o "detach" -o "quit" >"$tmp/lldb.log" 2>&1 \
    || { sed 's/^/lldb: /' "$tmp/lldb.log" >&2; die 1 "lldb attach failed — the app may have quit, or developer tools access is not granted"; }
  grep -q "$tab" "$tmp/lldb.log" || { sed 's/^/lldb: /' "$tmp/lldb.log" >&2; die 1 "no windows listed (see lldb output above)"; }
  grep "$tab" "$tmp/lldb.log"
  exit 0
fi

dylib="$tmp/tandem_shot.dylib"
clang -dynamiclib -framework AppKit -framework Foundation -o "$dylib" "$root/Scripts/shot.m" 2>&1 |
  sed 's/^/clang: /' || die 1 "compiling Scripts/shot.m failed"

printf 'shot: window %s of pid %s -> %s\n' "$window_id" "$APP_PID" "$out"
# The dylib is loaded at runtime, so no declaration of tandem_shot exists in the
# app at compile time: resolve it with dlsym and call it through a pointer of
# the right signature. The return code goes back as the expression's value
# (`-O`) for the same reason the list rows do — the app's stdout is not this log.
lldb --batch --no-lldbinit -p "$APP_PID" \
  -o "thread select 1" \
  -o "expr -- @import Foundation;" \
  -o "expr -- @import Darwin;" \
  -o "expr -O -- ({ void *h = dlopen(\"$dylib\", 2); int (*shot)(long, const char *) = h ? (int (*)(long, const char *))dlsym(h, \"tandem_shot\") : 0; NSString *r = !h ? @\"tandem_shot=dlopen-failed\" : !shot ? @\"tandem_shot=dlsym-failed\" : [NSString stringWithFormat:@\"tandem_shot=%d\", shot($window_id, \"$out\")]; r; })" \
  -o "detach" -o "quit" >"$tmp/lldb.log" 2>&1 \
  || { sed 's/^/lldb: /' "$tmp/lldb.log" >&2; die 1 "lldb attach failed — the app may have quit, or developer tools access is not granted"; }

grep -q 'tandem_shot=0' "$tmp/lldb.log" ||
  { sed 's/^/lldb: /' "$tmp/lldb.log" >&2; die 1 "the capture failed (see lldb output above)"; }
[ -s "$out" ] || die 1 "the capture wrote no file to $out"
printf 'shot: wrote %s (%s bytes)\n' "$out" "$(wc -c <"$out" | tr -d ' ')"
