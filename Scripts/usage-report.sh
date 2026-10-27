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
    # What the ended record carried, so the pass's record is attributed the
    # same way: its agent (the usage comment's signature), its project, its
    # tickets. The brief is this wrapper's own — the ended record keeps the
    # brief its pass wrote.
    project=$(printf '%s\n' "$state" | sed -n 's/.*"project":"\([^"]*\)".*/\1/p')
    agent=$(printf '%s\n' "$state" | sed -n 's/.*"agent":"\([^"]*\)".*/\1/p')
    carried=$(printf '%s\n' "$state" \
        | sed -n 's/.*"tickets":\[\([^]]*\)\].*/\1/p' | tr ',' '\n' | tr -d '"')

    # The record this pass gets: the same opencode id with the first `-<n>`
    # suffix that is free. Free is decided by minting it, not by reading the
    # list first — `start` on an id another pass has just taken returns that
    # record with a note at exit 0 (`Sources/TandemCore/Session.swift:414`), so
    # the loser of a race reads the note and takes the next suffix instead of
    # writing its usage into the winner's record. The attribution flags are
    # passed whole: a project name with a space is one argument, and an empty
    # `--agent` is the core's own "no agent" (`start` trims it to nil).
    brief="Re-dispatched pass in opencode session $SESSION: its record had already ended, so this record carries this pass's usage."
    n=2
    while :; do
        cont="$SESSION-$n"
        minted=$(printf '%s\n' "$brief" \
            | "$CLI" session start --session "$cont" --project "$project" --agent "$agent" --brief -) \
            || fail "could not open the continuation record $cont."
        case "$minted" in
        *'already recorded'*) n=$((n + 1)) ;;
        *) break ;;
        esac
    done

    for ticket in $carried; do
        # A failed link is the one failure that keeps the usage comment off a
        # ticket — the board attribution this record exists for. It is named on
        # stderr, and the usage is still written: a success code with no
        # comment is the defect, an end with the reason said out loud is not.
        "$CLI" session link --ticket "$ticket" --session "$cont" >/dev/null \
            || echo "usage-report: warning: could not link $ticket to $cont — its usage comment will not post." >&2
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
