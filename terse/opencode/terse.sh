#!/usr/bin/env bash
# The terse switch. Read by the plugin on every model request; written here.
#
#   terse.sh          # report
#   terse.sh on       # terse from the next request
#   terse.sh off      # prose from the next request
#   terse.sh toggle
set -euo pipefail

STATE="${XDG_CONFIG_HOME:-$HOME/.config}/opencode/terse.json"

read_state() {
  if [ -f "$STATE" ] && grep -q '"enabled"[[:space:]]*:[[:space:]]*false' "$STATE"; then
    echo off
  else
    echo on
  fi
}

write_state() { # write_state true|false
  mkdir -p "$(dirname "$STATE")"
  printf '{\n  "enabled": %s\n}\n' "$1" >"$STATE"
}

case "${1:-}" in
  "") read_state ;;
  on) write_state true && echo "terse on" ;;
  off) write_state false && echo "terse off" ;;
  toggle)
    if [ "$(read_state)" = on ]; then write_state false && echo "terse off"; else write_state true && echo "terse on"; fi
    ;;
  status | -s) read_state ;;
  *) echo "usage: terse.sh [on|off|toggle|status]" >&2; exit 2 ;;
esac
