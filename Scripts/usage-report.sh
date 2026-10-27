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
# **A re-dispatched pass gets its own record (RET-43).** The harness resumes an
# opencode session whose tandem record the pass before it already ended — a
# record stamps once, so that one cannot take this pass's usage, and ending
# into it exits 0 having written nothing. When the record is already `ended`,
# the wrapper mints this pass its own record instead: the same opencode id with
# a `-<n>` suffix (the first `-2`, `-3`, … not taken), carrying the ended
# record's agent, project and tickets, so the usage lands there and its `usage`
# comment posts on the board the pass carried. A running record takes the usage
# itself, as it always did.
#
# Environment:
#   OPENCODE_SESSION_ID  the session's id — the db row's key (required)
#   OPENCODE_DB          the database, default ~/.local/share/opencode/opencode.db
#   TANDEM_CLI           the CLI to run, default `tandem-cli` from PATH — a dev
#                        build can be named here while the shipped app owns PATH
#   TANDEM_HOME          passes through to the CLI untouched
# Extra arguments are passed to `session end` verbatim (an `--outcome`, say).

set -eu

DB="${OPENCODE_DB:-$HOME/.local/share/opencode/opencode.db}"
SESSION="${OPENCODE_SESSION_ID:-}"
CLI="${TANDEM_CLI:-tandem-cli}"

fail() {
    echo "usage-report: $1" >&2
    echo "usage-report: fall back to a plain end: $CLI session end --summary -" >&2
    exit 1
}

[ -n "$SESSION" ] || fail "OPENCODE_SESSION_ID is unset — there is no session row to read."
# The id goes into one SQL string; a quote in it is refused, never escaped.
case "$SESSION" in
    *"'"*|*'"'*) fail "OPENCODE_SESSION_ID contains a quote — refusing to query the database." ;;
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

# The record this opencode session already has: `running` takes the usage
# itself (the exec below); `ended` cannot — it stamps once — so this pass gets
# its own record. A `session show` failure (no record at all) is not this
# wrapper's business: the `end` below reports it.
state=$("$CLI" session show --session "$SESSION" 2>/dev/null) || state=""
case "$state" in
*'"status":"ended"'*)
    # The first `-<n>` suffix this opencode session has not used.
    ids=$("$CLI" session list 2>/dev/null | grep -o '"id":"[^"]*"' | cut -d'"' -f4) || ids=""
    n=2
    while printf '%s\n' "$ids" | grep -Fqx "$SESSION-$n"; do n=$((n + 1)); done
    cont="$SESSION-$n"

    # What the ended record carried, so the pass's record is attributed the
    # same way: its agent (the usage comment's signature), its project, its
    # tickets. The brief is this wrapper's own — the ended record keeps the
    # brief its pass wrote.
    project=$(printf '%s\n' "$state" | sed -n 's/.*"project":"\([^"]*\)".*/\1/p')
    agent=$(printf '%s\n' "$state" | sed -n 's/.*"agent":"\([^"]*\)".*/\1/p')
    carried=$(printf '%s\n' "$state" \
        | sed -n 's/.*"tickets":\[\([^]]*\)\].*/\1/p' | tr ',' '\n' | tr -d '"')

    who=""
    [ -n "$project" ] && who="$who --project $project"
    [ -n "$agent" ] && who="$who --agent $agent"
    printf '%s\n' "Re-dispatched pass in opencode session $SESSION: its record had already ended, so this record carries this pass's usage." \
        | "$CLI" session start --session "$cont" $who --brief - >/dev/null \
        || fail "could not open the continuation record $cont."

    for ticket in $carried; do
        "$CLI" session link --ticket "$ticket" --session "$cont" >/dev/null 2>&1 || true
    done

    # stdin is the summary and has not been read yet; it is read here because
    # the mint above could not share it. A failed end is the script's exit code
    # (set -e), so the pass sees it.
    summary=$(cat)
    printf '%s\n' "$summary" | "$CLI" session end --session "$cont" --summary - $flags "$@"
    exit 0
    ;;
esac

# The summary travels on stdin, untouched; the usage flags follow; the caller's
# own arguments go last. exec so the CLI is this script's exit code and stdout.
exec "$CLI" session end --summary - $flags "$@"
