#!/bin/sh
# usage-report.sh — end this session's Tandem record with the usage opencode
# recorded (spec F68948C0).
#
# One harness adapter: sqlite3 reads this session's row from opencode's
# database, then `tandem-cli session end` is exec'd with the summary on stdin
# (passed straight through) plus the usage flags — the record gains tokens,
# cost and model, and the core's `end` posts one `usage` comment on every
# ticket the session linked. Other harnesses write their own wrapper against
# the same flags; nothing here is opencode-specific in the CLI's eyes.
#
# The row missing is not a failure of the session: the wrapper exits 1 with a
# one-line message and the pass falls back to a plain `tandem-cli session end
# --summary -`. stdout carries the end call's JSON only — every message this
# script adds goes to stderr, where no JSON parser is reading.
#
# The record is $OPENCODE_SESSION_ID's; `--session <id>` names another — the
# record a mid-session verify minted, say — and is read from its own row. The
# flag is consumed here, never forwarded: `session end` has no such argument.
#
# Environment:
#   OPENCODE_SESSION_ID  the session's id — the db row's key (required unless
#                        `--session` names one)
#   OPENCODE_DB          the database, default ~/.local/share/opencode/opencode.db
#   TANDEM_CLI           the CLI to run, default `tandem-cli` from PATH — a dev
#                        build can be named here while the shipped app owns PATH
#   TANDEM_HOME          passes through to the CLI untouched
# Extra arguments are passed to `session end` verbatim (an `--outcome`, say).

set -eu

DB="${OPENCODE_DB:-$HOME/.local/share/opencode/opencode.db}"
SESSION="${OPENCODE_SESSION_ID:-}"
SOURCE="OPENCODE_SESSION_ID"
CLI="${TANDEM_CLI:-tandem-cli}"

fail() {
    echo "usage-report: $1" >&2
    echo "usage-report: fall back to a plain end: $CLI session end --summary -" >&2
    exit 1
}

# `--session <id>` first, as the one argument this wrapper reads for itself;
# every other argument stays the caller's and reaches `session end` verbatim.
if [ "${1:-}" = "--session" ]; then
    [ $# -ge 2 ] && [ -n "$2" ] || fail "--session needs a session id."
    SESSION="$2"
    SOURCE="--session"
    shift 2
fi

[ -n "$SESSION" ] || fail "OPENCODE_SESSION_ID is unset — there is no session row to read."
# The id goes into one SQL string; a quote in it is refused, never escaped.
case "$SESSION" in
    *"'"*|*'"'*) fail "$SOURCE contains a quote — refusing to query the database." ;;
esac
[ -f "$DB" ] || fail "no opencode database at $DB."

row=$(sqlite3 -readonly -separator "$(printf '\t')" "$DB" \
    "SELECT tokens_input, tokens_output, tokens_reasoning,
            tokens_cache_read, tokens_cache_write, cost,
            COALESCE(json_extract(model, '\$.id'), model)
     FROM session_v2 WHERE id = '$SESSION' LIMIT 1;") \
    || fail "sqlite3 could not read $DB."
[ -n "$row" ] || fail "no session row for $SESSION in $DB."

input=$(printf '%s\n' "$row" | cut -f1)
output=$(printf '%s\n' "$row" | cut -f2)
reasoning=$(printf '%s\n' "$row" | cut -f3)
cache_read=$(printf '%s\n' "$row" | cut -f4)
cache_write=$(printf '%s\n' "$row" | cut -f5)
cost=$(printf '%s\n' "$row" | cut -f6)
model=$(printf '%s\n' "$row" | cut -f7)

# A column the row does not carry (a NULL cost, say) contributes no flag — the
# CLI stores what is there, never an invented zero. The values are opencode's
# own integers, a real and a model id; none carries whitespace.
flags=""
[ -n "$input" ] && flags="$flags --tokens-input $input"
[ -n "$output" ] && flags="$flags --tokens-output $output"
[ -n "$reasoning" ] && flags="$flags --tokens-reasoning $reasoning"
[ -n "$cache_read" ] && flags="$flags --tokens-cache-read $cache_read"
[ -n "$cache_write" ] && flags="$flags --tokens-cache-write $cache_write"
[ -n "$cost" ] && flags="$flags --cost $cost"
[ -n "$model" ] && flags="$flags --model $model"

# The summary travels on stdin, untouched; the usage flags follow; the caller's
# own arguments go last. exec so the CLI is this script's exit code and stdout.
exec "$CLI" session end --summary - $flags "$@"
