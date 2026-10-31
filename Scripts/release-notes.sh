#!/bin/sh
# The release notes (7B005267, BCE67BAE): what shipped, not a fixed tag line,
# grouped by kind — features, bugs and fixes, docs and infra — with an
# explicit "Other (unclassified)" fallback.
#
# Usage: Scripts/release-notes.sh <new-tag> [--capture] — run from a checkout
# that has the tags. Prints the body to stdout: the fixed lead line plus one
# section per kind, in the fixed order above, empty sections omitted. The
# GitHub release body is this output.
#
# --capture also files the same note into the tandem bikeshed through the one
# sanctioned agent write path — `tandem-cli note capture --kind fact --project
# tandem --session release-notes` — so the note lands in the saddlebag
# (projects/tandem/.raw/release-notes/<utc>-fact-release-<version>-what-
# shipped.md) and the consolidation pass, the curated layer's only writer,
# promotes it into a curated "Releases" note. This script never writes under
# projects/** itself and never edits a curated note. Retrofiling a release is
# the same call: Scripts/release-notes.sh v0.25.0 --capture.
#
# Classification is mechanical, first match wins: the PR's labels, then the
# words of its title. A title whose words say nothing falls back to the merge
# subject's branch area — the last segment of `board/<area>`, the slug the
# board names the work by — so a title like "todo archive --id: archive one
# ticket" still lands through `board/sessions-migration`. A PR whose labels,
# words and branch area all say nothing lands in Other, never in a guess.
# Words that place a PR:
#   bugs/fixes — fix(es|ed), bug(s), repair(s), correct(s), regression,
#                crash(es), harden(s), tighten(s), reject(s), restore(s)
#   features   — add(s), feat|feature, introduce(s), new, support(s), expose(s),
#                wire(s), teach(es), ship(s), retire(s), remove(s), rename(s),
#                store(s), sqlite, database(s), migration(s), cli, app(s),
#                window(s), watcher(s), board
#   docs/infra — doc(s), readme, prose, skill(s), adr, ci, workflow, release,
#                script(s), help, contract, test(s), build, sprint(s)
#
# The area words are the product's own nouns — the store, the app and its
# window, the board — so they read as features; `sprint` names the skill and
# its records, which is prose, so it reads as infra. The area is scanned
# without its `board/` prefix: the prefix is every branch's, not a word.
set -eu

new=${1:?usage: release-notes.sh <new-tag> [--capture]}
shift
capture=0
for argument in "$@"; do
  case "$argument" in
    --capture) capture=1 ;;
    *)
      echo "release-notes.sh: unknown argument: $argument (usage: release-notes.sh <new-tag> [--capture])" >&2
      exit 2
      ;;
  esac
done

version=${new#v}

# The previous release tag: the nearest tag behind the new one.
prev=$(git describe --tags --abbrev=0 "$new^" 2>/dev/null || true)
if [ -z "$prev" ]; then
  echo "release-notes.sh: no tag precedes $new" >&2
  exit 1
fi

# The lead line is user-facing and always fully qualified: the token `tandem`
# collides with Homebrew core's unrelated cask, and renaming it would break
# existing installs' upgrade path (PRT-36), so the tap prefix is the contract.
lead="Tandem $version for macOS 27 (Apple silicon). Install: brew install --cask gianniacquisto/tap/tandem"

# The integration tip a tag names. The tags before the flip sit on `dev`'s
# head, which is their own tip, or on a `release/vX.Y.Z` merge of `dev` into
# `main`, which needs its second parent. The tags after it sit on `main`'s
# head, the merge the batch landed on, and resolve to themselves: `main` is
# the integration branch now, so there is no second parent to walk. Every
# shape is read, because the tags before the flip are history the notes still
# describe.
integration_tip() {
  case "$(git log -1 --format=%s "$1")" in
    "Merge pull request #"*" from "*"/dev" \
      | "Merge pull request #"*" from "*"/release/"* \
      | "Merge branch 'dev'"* \
      | "Merge branch 'release/"*)
      git rev-parse "$1^2" ;;
    *)
      git rev-parse "$1" ;;
  esac
}

prevTip=$(integration_tip "$prev")
newTip=$(integration_tip "$new")

# The kind a merged PR belongs to. Labels outrank the title's words because a
# label is someone's stated intent; the words are the fallback.
classify() {
  labels=$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')
  case "$labels" in
    *bug*|*fix*) printf 'fixes\n'; return ;;
    *feature*|*enhancement*) printf 'features\n'; return ;;
    *doc*|*infra*|*ci*|*chore*|*refactor*|*test*) printf 'infra\n'; return ;;
  esac
  # Everything that is not a letter or a digit becomes a space, so the scan
  # below reads whole words and cannot match inside one.
  words=$(printf '%s' "$2" | tr '[:upper:]' '[:lower:]' | tr -c '[:alnum:]' ' ')
  for word in $words; do
    case "$word" in
      fix|fixes|fixed|bug|bugs|repair|repairs|correct|corrects|regression|crash|crashes|harden|hardens|tighten|tightens|reject|rejects|restore|restores)
        printf 'fixes\n'; return ;;
      add|adds|feat|feature|introduce|introduces|new|support|supports|expose|exposes|wire|wires|teach|teaches|ship|ships|retire|retires|remove|removes|rename|renames|store|stores|sqlite|database|databases|migration|migrations|cli|app|apps|window|windows|watcher|watchers|board)
        printf 'features\n'; return ;;
      doc|docs|readme|prose|skill|skills|adr|ci|workflow|release|script|scripts|help|contract|test|tests|build|sprint|sprints)
        printf 'infra\n'; return ;;
    esac
  done
  printf 'other\n'
}

# The integration branch's own merges between the tags: the PR merges.
# --first-parent keeps the union merges a PR branch carries when the
# integration branch moves under it ("Merge dev into board/…") out of the
# list, and the tip resolution above keeps the release merge itself out.
subjects=$(git log "$prevTip..$newTip" --first-parent --merges --format='%s')

if [ -z "$subjects" ]; then
  printf '%s\n' "$lead"
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
for kind in features fixes infra other; do : > "$tmp/$kind"; done
tab=$(printf '\t')

printf '%s\n' "$subjects" | while IFS= read -r subject; do
  [ -n "$subject" ] || continue
  case "$subject" in
    "Merge pull request #"*" from "*)
      num=${subject#*#}; num=${num%% *}
      branch=${subject##*from }
      branch=${branch#*/}
      meta=""
      gh_cmd=${RELEASE_NOTES_GH:-gh}
      if [ -n "$gh_cmd" ] && command -v "$gh_cmd" >/dev/null 2>&1; then
        # `<title>\t<label,label>` — one call per PR for both fields.
        meta=$("$gh_cmd" pr view "$num" --json title,labels \
          --jq '[.title, ([.labels[].name] | join(","))] | @tsv' 2>/dev/null || true)
      fi
      title=""
      labels=""
      if [ -n "$meta" ]; then
        title=${meta%%"$tab"*}
        labels=${meta#*"$tab"}
        [ "$labels" = "$meta" ] && labels=""
      fi
      # The area the branch names — `board/<area>`, so its last segment. The
      # prefix is every board branch's and is never scanned as a word.
      area=${branch##*/}
      if [ -n "$title" ]; then
        bullet="- #$num $title"
        kind=$(classify "$labels" "$title")
        if [ "$kind" = other ]; then
          # The title's words said nothing: the branch area gets the next say.
          kind=$(classify "" "$area")
        fi
      else
        bullet="- #$num ($branch)"
        kind=$(classify "" "$area")
      fi
      printf '%s\n' "$bullet" >> "$tmp/$kind"
      ;;
    *)
      kind=$(classify "" "$subject")
      printf -- '- %s\n' "$subject" >> "$tmp/$kind"
      ;;
  esac
done

: > "$tmp/sections"
for kind in features fixes infra other; do
  [ -s "$tmp/$kind" ] || continue
  case "$kind" in
    features) heading="## Features" ;;
    fixes) heading="## Bugs and fixes" ;;
    infra) heading="## Docs and infra" ;;
    other) heading="## Other (unclassified)" ;;
  esac
  printf '\n%s\n\n' "$heading" >> "$tmp/sections"
  cat "$tmp/$kind" >> "$tmp/sections"
done

printf '%s\n' "$lead"
cat "$tmp/sections"

# The bikeshed note: the same content, filed through the verb, never written
# into the curated layer by this script. The verb prints the path it used.
if [ "$capture" = 1 ]; then
  verb=${RELEASE_NOTES_TANDEM:-tandem-cli}
  if ! command -v "$verb" >/dev/null 2>&1; then
    echo "release-notes.sh: --capture needs $verb on PATH — the bikeshed note goes through note capture" >&2
    exit 1
  fi
  {
    printf '# Release %s: what shipped\n\n' "$version"
    printf '%s\n' "$lead"
    cat "$tmp/sections"
  } | "$verb" note capture --kind fact --project tandem --session release-notes > "$tmp/filed"
  echo "release-notes.sh: filed $(cat "$tmp/filed")" >&2
fi
