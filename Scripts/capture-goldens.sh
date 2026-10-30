#!/usr/bin/env bash
# capture-goldens.sh: the port's CLI goldens, captured from the Swift binary.
#
# `docs/port.md` (Parity, item 1): the CLI goldens captured from the Swift binary
# pass against the Rust binary, message for message. This script is the capturing
# half. `Fixtures/goldens/manifest.json` is the case list: the CLI's contract,
# one deliberate line per verb shape, and each case is run against a fresh copy
# of `Fixtures/port/Home`; the run's exit code, its JSON on stdout and its text on
# stderr are written to `Fixtures/goldens/<group>/<case>.json`.
#
# The other half is `rust/cli/tests/goldens.rs`: it reads the same manifest, runs
# the same cases through the Rust binary and compares the result to the same
# files. Nothing in that test says which binary captured a golden, and nothing
# here knows how to compare two of them.
#
# **The sandbox, not the human's home.** Every run gets a `HOME` and a
# `TANDEM_HOME` this script created under one `mktemp -d`, with the fixture home
# copied into it; the child environment is `env -i`, so nothing inherited from
# the shell (a real `TANDEM_HOME`, a real session id) reaches the CLI. A run that
# writes anywhere else is a bug in the CLI, not a thing this script can do.
#
# **Normalized, not massaged.** Ids, timestamps and the two sandbox paths are
# replaced with the names the manifest declares (`<id>`, `<timestamp>`,
# `<tandem-home>`, `<home>`), because they differ per machine and per day. Nothing
# else is touched: a golden that normalizes the shape of the answer proves
# nothing. `doctor`'s `app` and `skills` rows are the one exception, and the case
# says so itself (`mask`): those rows read the machine, not the fixture home, so
# the check names still compare and their detail is `<environment>`.
#
# **Which Swift binary.** The default is this checkout's build,
# `.build/debug/tandem-cli`, because the repo is what the port ports: `Sources/`
# is the reference, and it carries fixes the shipped app predates (`setup
# uninstall`'s `no-bundle` warning is `info` here and `warn` in 0.36.0, so a
# golden captured from the app would pin the bug). Build it first:
#
#   swift build --product tandem-cli
#   Scripts/capture-goldens.sh
#
# `--binary /Applications/Tandem.app/Contents/MacOS/tandem-cli` captures from the
# shipped app instead, which is what a release's behaviour is; the two agree
# except where `Sources/` has moved since that build.
#
# Usage:
#   Scripts/capture-goldens.sh [--binary <path>] [--keep]
#
#   --binary  the Swift CLI to capture from (default: `.build/debug/tandem-cli`,
#             else $TANDEM_CLI)
#   --keep    leave the sandboxes on disk and print where they are.
#
# Exits 1 on the first case whose exit code is not the one the manifest declares,
# so a stale expectation is caught by the capture and not by a later reader.

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
manifest=$root/Fixtures/goldens/manifest.json
goldens=$root/Fixtures/goldens
binary=${TANDEM_CLI:-$root/.build/debug/tandem-cli}
keep=0

die() {
    printf 'capture-goldens: %s\n' "$1" >&2
    exit 1
}

while [ $# -gt 0 ]; do
    case $1 in
    --binary)
        [ $# -ge 2 ] || die "--binary needs a path"
        binary=$2
        shift 2
        ;;
    --keep)
        keep=1
        shift
        ;;
    *)
        die "unknown argument: $1"
        ;;
    esac
done

[ -x "$binary" ] || die "no executable at $binary (build it with `swift build --product tandem-cli`, or pass --binary)"
# Absolute from here on: every run happens with the sandbox home as its cwd.
binary=$(cd "$(dirname "$binary")" && pwd)/$(basename "$binary")
command -v jq > /dev/null || die "jq is required"
[ -f "$manifest" ] || die "no manifest at $manifest"

face=$root/$(jq -r '.face' "$manifest")
home=$root/$(jq -r '.home' "$manifest")
[ -d "$home/bikeshed" ] || die "the fixture home is not at $home"
[ -d "$face/skills" ] || die "the fixture agent face is not at $face"

work=$(mktemp -d "${TMPDIR:-/tmp}/tandem-goldens.XXXXXX")
# `mktemp` leaves the template's `//` in place when TMPDIR ends in a slash; the
# CLI standardizes the paths it reports, so the sandbox spelling must match.
work=$(cd "$work" && pwd)
cleanup() {
    if [ "$keep" = 1 ]; then
        printf 'capture-goldens: sandboxes kept at %s\n' "$work" >&2
    else
        rm -rf "$work"
    fi
}
trap cleanup EXIT

# MARK: - The manifest is the contract, so it is checked before it is obeyed

for duplicate in $(jq -r '[.cases[] | .group + "/" + .case]
    | group_by(.) | map(select(length > 1) | .[0]) | .[]' "$manifest"); do
    die "the manifest names $duplicate twice"
done

missing_groups=$(jq -r '(["ticket", "lane", "note", "session", "sprint", "setup", "doctor"]
    - ([.cases[].group] | unique)) | join(" ")' "$manifest")
[ -z "$missing_groups" ] || die "the manifest has no case in: $missing_groups"

# The verb list is read from `--help`, so a verb with no case is a missing case:
# the manifest is the CLI's grammar, and a verb the binary answers but the
# manifest does not name would go untested in both implementations.
grammar=$(env -i PATH=/usr/bin:/bin:/usr/sbin:/sbin "$binary" --help) \
    || die "$binary --help failed"
verbs=$(printf '%s\n' "$grammar" | awk '
    /^  tandem-cli / {
        group = $2
        if (group !~ /^[a-z]+$/) next
        verb = $3
        if (verb !~ /^[a-z]+$/) verb = ""
        print (verb == "" ? group : group " " verb)
    }')
[ -n "$verbs" ] || die "$binary --help carries no grammar to compare the manifest to"

help_cases=$(jq -r '.cases[]
    | select((.argv | type) == "array" and (.argv[-1] // "") == "--help")
    | .argv | join(" ")' "$manifest")
missing_help=""
groups_seen=""
while IFS= read -r verb; do
    [ -n "$verb" ] || continue
    group=${verb%% *}
    case $groups_seen in
    *" $group "*) ;;
    *) groups_seen="$groups_seen $group " ;;
    esac
    printf '%s\n' "$help_cases" | grep -Fxq "$verb --help" \
        || missing_help="$missing_help $verb"
done <<< "$verbs"
for group in $groups_seen; do
    printf '%s\n' "$help_cases" | grep -Fxq "$group --help" \
        || missing_help="$missing_help $group"
done
[ -z "$missing_help" ] \
    || die "the manifest has no help case for:$missing_help (read from $binary --help)"

# MARK: - The readers, as jq

# One definition per normalization rule, applied to strings only, in manifest
# order. `prefix` is a literal split/join, not a regex: the sandbox path holds
# characters a regex would read as its own.
normalize_jq='
def normalized($rules):
  walk(
    if type == "string" then
      reduce $rules[] as $r (.;
        if $r.kind == "prefix" then split($r.value) | join($r.as)
        elif $r.kind == "uuid" then
          gsub("[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}"; $r.as)
        elif $r.kind == "timestamp" then
          gsub("[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\\.[0-9]+)?Z"; $r.as)
          | gsub("[0-9]{8}T[0-9]{6}Z"; $r.as)
        else . end)
    else . end);

def masked($mask):
  if $mask == null then .
  else
    (if ((.[$mask.at] | type) == "array") then
       .[$mask.at] |= map(
         if (.name as $name | $mask.names | index($name)) then .detail = $mask.as else . end)
     else . end)
  end;

def lines_($rules):
  split("\n")
  | if length > 0 and .[-1] == "" then .[:-1] else . end
  | map(normalized($rules));
'

# MARK: - The cases

captured=0
while IFS= read -r case_json; do
    group=$(jq -r '.group' <<< "$case_json")
    name=$(jq -r '.case' <<< "$case_json")
    case $group$name in
    *[!a-z0-9-]*) die "a case name is a path segment: $group/$name" ;;
    esac
    case_id=$group/$name

    sb=$work/$group/$name
    mkdir -p "$sb/user"
    cp -R "$home" "$sb/Home"
    case $sb in
    "$work"/*) ;;
    *) die "refusing a sandbox outside $work: $sb" ;;
    esac

    # The child's whole environment: a fixed PATH and TZ, the two sandbox paths,
    # the fixture face, then the manifest's env and the case's own (a null value
    # means the variable is not set at all).
    env_lines=$(jq -r --arg tandem_home "$sb/Home" --arg home "$sb/user" --arg face "$face" \
        --slurpfile m "$manifest" <<< "$case_json" '
        . as $case
        | ({"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "TZ": "UTC",
            "HOME": $home, "TANDEM_HOME": $tandem_home, "TANDEM_AGENT_BUNDLE": $face}
           + ($m[0].env | map_values(if . == "$face" then $face else . end))
           + ($case.env // {}))
        | to_entries[] | select(.value != null) | "\(.key)=\(.value)"
    ')
    envlist=()
    while IFS= read -r line; do
        envlist+=("$line")
    done <<< "$env_lines"

    rules_json=$(jq -c --arg tandem_home "$sb/Home" --arg home "$sb/user" '
        .normalize
        | map(if .kind == "prefix" then
                .value = (if .value == "$TANDEM_HOME" then $tandem_home
                          elif .value == "$HOME" then $home
                          else .value end)
              else . end)' "$manifest")

    # The setup runs first, unseen: it is the fixture the case reads, not the
    # case. Each step is a CLI run (its exit code must be 0) or a file op inside
    # the sandbox home.
    printf '%s' "$(jq -j '.stdin // ""' <<< "$case_json")" > "$sb/stdin"
    while IFS= read -r step; do
        [ -n "$step" ] || continue
        kind=$(jq -r 'if has("argv") then "argv" elif has("rm") then "rm"
                      elif has("write") then "write" else "" end' <<< "$step")
        case $kind in
        argv)
            argv=()
            while IFS= read -r arg; do
                argv+=("$arg")
            done < <(jq -r '.argv[]' <<< "$step")
            printf '%s' "$(jq -j '.stdin // ""' <<< "$step")" > "$sb/setup-stdin"
            (cd "$sb/Home" && env -i ${envlist[@]+"${envlist[@]}"} "$binary" \
                ${argv[@]+"${argv[@]}"} \
                < "$sb/setup-stdin" > "$sb/setup-out" 2> "$sb/setup-err") || {
                printf 'capture-goldens: %s: setup exited %s\n' "$case_id" "$?" >&2
                sed 's/^/  /' "$sb/setup-err" >&2
                exit 1
            }
            ;;
        rm)
            rm -rf "$sb/Home/$(jq -r '.rm' <<< "$step")" || die "$case_id: setup rm failed"
            ;;
        write)
            at=$sb/Home/$(jq -r '.write.at' <<< "$step")
            mkdir -p "$(dirname "$at")"
            jq -j '.write.text' <<< "$step" > "$at" || die "$case_id: setup write failed"
            ;;
        *)
            die "$case_id: unknown setup step: $step"
            ;;
        esac
    done <<< "$(jq -c '.setup // [] | .[]' <<< "$case_json")"

    expected=$(jq -r '.exit // 0' <<< "$case_json")
    argv=()
    while IFS= read -r arg; do
        argv+=("$arg")
    done < <(jq -r '.argv[]' <<< "$case_json")
    code=0
    (cd "$sb/Home" && env -i ${envlist[@]+"${envlist[@]}"} "$binary" \
        ${argv[@]+"${argv[@]}"} < "$sb/stdin" > "$sb/out" 2> "$sb/err") || code=$?
    if [ "$code" -ne "$expected" ]; then
        printf 'capture-goldens: %s exited %s, the manifest declares %s\n' \
            "$case_id" "$code" "$expected" >&2
        printf '  stdout:\n' >&2
        sed 's/^/    /' "$sb/out" >&2
        printf '  stderr:\n' >&2
        sed 's/^/    /' "$sb/err" >&2
        exit 1
    fi

    mkdir -p "$goldens/$group"
    jq -S -n \
        --argjson case "$case_json" \
        --argjson rules "$rules_json" \
        --argjson mask "$(jq -c '.mask // null' <<< "$case_json")" \
        --rawfile out "$sb/out" \
        --rawfile err "$sb/err" \
        "$normalize_jq"'
        {argv: $case.argv, stdin: ($case.stdin // ""), exit: ($case.exit // 0)}
        + (($out | try fromjson catch null) as $parsed
           | if ($parsed | type) == "object" or ($parsed | type) == "array"
             then {stdoutKind: "json", stdout: ($parsed | normalized($rules) | masked($mask))}
             else {stdoutKind: "text", stdout: ($out | lines_($rules))}
             end)
        + {stderr: ($err | lines_($rules))}
        ' > "$goldens/$group/$name.json"
    captured=$((captured + 1))
done < <(jq -c '.cases[]' "$manifest")

# A case the manifest no longer names is a golden no one reads: drop it, so the
# tree says exactly what the manifest says.
expected_files=$(jq -r '.cases[] | .group + "/" + .case + ".json"' "$manifest")
stale=""
while IFS= read -r golden; do
    rel=${golden#"$goldens"/}
    printf '%s\n' "$expected_files" | grep -Fxq "$rel" || stale="$stale $rel"
done < <(find "$goldens" -mindepth 2 -name '*.json' | sort)
for rel in $stale; do
    rm "$goldens/$rel"
    printf 'capture-goldens: dropped %s, the manifest does not name it\n' "$rel" >&2
done
find "$goldens" -mindepth 1 -type d -empty -delete

printf 'capture-goldens: %s case(s) from %s\n' "$captured" "$binary"
