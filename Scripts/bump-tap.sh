#!/bin/sh
# The tap bump (ADR 0036): the tag run pushes the cask too, so no release waits
# for a human commit. Run from the tandem checkout after `dist/SHA256SUMS` is
# written; the release workflow's last step does exactly that.
#
# Usage: Scripts/bump-tap.sh <tag> <checksums-file> [--tap-dir <dir>] [--verify] [--remote <url>]
#
# <tag> is vX.Y.Z and <checksums-file> is `shasum -a 256`'s output for the
# artifact (`<64 hex>  Tandem-<version>.zip`). The script rewrites exactly two
# lines of the tap's Casks/tandem.rb, `version` and `sha256`, leaving every
# other byte alone, including the tap-prefixed `url` the token collision with
# Homebrew core's cask depends on, commits as github-actions[bot] with the tap's
# own subject (`tandem <version>`), and pushes `main`.
#
# The tap clone is brew's: `$(brew --prefix)/Library/Taps/gianniacquisto/
# homebrew-tap`, because brew loads a cask only from the tap tree (`brew audit`
# refuses a bare path). Absent, it is cloned from `--remote`, whose default
# carries TAP_TOKEN in the URL GitHub masks; the push then authenticates through
# that same tokenised URL, and a trap strips the token from the clone before the
# script exits on any path, so it lives nowhere on disk afterwards. Present, it
# is fetched and reset to `origin/main`, the path a re-bump of a past tag takes
# on a developer machine.
#
# TAP_TOKEN is required only for the clone: unset or empty with no `--remote`
# fails before anything is cloned, naming the secret. A missing token is a red
# step, never a silent skip: the stall this replaces.
#
# --verify runs the release's gates on the runner before the push: `brew trust
# --cask gianniacquisto/tap/tandem` (a fresh clone is an untrusted tap and the
# audit refuses to load its cask), then `brew style Casks/tandem.rb`, then
# `brew audit --cask --strict gianniacquisto/tap/tandem`.
#
# A cask already carrying this version and digest prints "up to date", commits
# nothing and pushes nothing, so a re-cut of the same tag stays green.
set -eu

usage() {
  echo "usage: bump-tap.sh <tag> <checksums-file> [--tap-dir <dir>] [--verify] [--remote <url>]" >&2
}

if [ "$#" -lt 2 ]; then
  usage
  exit 2
fi

tag=$1
sums=$2
shift 2

tap_dir=
remote=
verify=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --tap-dir)
      [ "$#" -ge 2 ] || { usage; exit 2; }
      tap_dir=$2
      shift 2
      ;;
    --remote)
      [ "$#" -ge 2 ] || { usage; exit 2; }
      remote=$2
      shift 2
      ;;
    --verify)
      verify=1
      shift
      ;;
    *)
      echo "bump-tap.sh: unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

# The tag, before anything touches the filesystem.
case "$tag" in
  v*) ;;
  *)
    echo "bump-tap.sh: the tag must be vX.Y.Z, got '$tag'" >&2
    exit 2
    ;;
esac
version=${tag#v}
case "$version" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *)
    echo "bump-tap.sh: the tag must be vX.Y.Z, got '$tag'" >&2
    exit 2
    ;;
esac
if printf '%s' "$version" | grep -Eqv '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "bump-tap.sh: the tag must be vX.Y.Z, got '$tag'" >&2
  exit 2
fi

# The digest, from the one line whose filename is the artifact.
if [ ! -f "$sums" ]; then
  echo "bump-tap.sh: no such checksums file: $sums" >&2
  exit 2
fi
archive="Tandem-$version.zip"
count=$(awk -v want="$archive" '$2 == want' "$sums" | wc -l | tr -d ' ')
if [ "$count" -ne 1 ]; then
  echo "bump-tap.sh: $sums carries $count lines for $archive, expected exactly one" >&2
  exit 2
fi
digest=$(awk -v want="$archive" '$2 == want { print $1; exit }' "$sums")
case "$digest" in
  *[!0-9a-f]*)
    echo "bump-tap.sh: the digest for $archive is not 64 lowercase hex: '$digest'" >&2
    exit 2
    ;;
esac
if [ "${#digest}" -ne 64 ]; then
  echo "bump-tap.sh: the digest for $archive is not 64 lowercase hex: '$digest'" >&2
  exit 2
fi

# The tap clone: brew's tree by default, because brew loads a cask only there.
if [ -z "$tap_dir" ]; then
  if ! command -v brew >/dev/null 2>&1; then
    echo "bump-tap.sh: no brew on PATH for the default tap directory; pass --tap-dir" >&2
    exit 2
  fi
  tap_dir="$(brew --prefix)/Library/Taps/gianniacquisto/homebrew-tap"
fi

if [ "$verify" -eq 1 ] && ! command -v brew >/dev/null 2>&1; then
  echo "bump-tap.sh: --verify needs brew on PATH, and none is" >&2
  exit 2
fi

# The clone's remote carries the token, so the push must authenticate through it
# before it goes away. The trap strips it on every exit path, failed clone
# included: a developer re-bump with a `--remote` that carries the token leaves
# no credential in the clone's .git/config.
token_remote=
strip_token() {
  [ -n "$token_remote" ] || return 0
  if [ -d "$tap_dir/.git" ]; then
    git -C "$tap_dir" remote set-url origin https://github.com/gianniacquisto/homebrew-tap.git || :
  fi
  token_remote=
}

if [ ! -d "$tap_dir" ]; then
  if [ -z "$remote" ]; then
    if [ -z "${TAP_TOKEN:-}" ]; then
      echo "bump-tap.sh: TAP_TOKEN is not set, so the tap cannot be cloned and the cask cannot be bumped." >&2
      echo "bump-tap.sh: create a fine-grained personal access token with Contents: Read and write on gianniacquisto/homebrew-tap alone, and set it as the TAP_TOKEN repository secret on gianniacquisto/tandem (Settings > Secrets and variables > Actions)." >&2
      exit 1
    fi
    remote="https://x-access-token:${TAP_TOKEN}@github.com/gianniacquisto/homebrew-tap.git"
  fi
  mkdir -p "$(dirname "$tap_dir")"
  token_remote=$remote
  trap strip_token EXIT
  git clone --quiet "$remote" "$tap_dir"
else
  git -C "$tap_dir" fetch --quiet origin
  git -C "$tap_dir" reset --hard --quiet origin/main
fi

cask="$tap_dir/Casks/tandem.rb"
if [ ! -f "$cask" ]; then
  echo "bump-tap.sh: no cask at $cask" >&2
  exit 1
fi
if ! grep -Eq '^  version "[^"]+"$' "$cask"; then
  echo "bump-tap.sh: $cask has no '  version \"X.Y.Z\"' line" >&2
  exit 1
fi
if ! grep -Eq '^  sha256 "[^"]+"$' "$cask"; then
  echo "bump-tap.sh: $cask has no '  sha256 \"<digest>\"' line" >&2
  exit 1
fi

current_version=$(sed -n 's/^  version "\([^"]*\)"$/\1/p' "$cask")
current_digest=$(sed -n 's/^  sha256 "\([^"]*\)"$/\1/p' "$cask")
if [ "$current_version" = "$version" ] && [ "$current_digest" = "$digest" ]; then
  echo "bump-tap.sh: the cask is already at $version with this digest; up to date, nothing pushed"
  exit 0
fi

# Exactly two lines change; every other byte stays.
tmp="$cask.tmp.$$"
sed -e "s/^  version \"[^\"]*\"$/  version \"$version\"/" \
    -e "s/^  sha256 \"[^\"]*\"$/  sha256 \"$digest\"/" "$cask" > "$tmp"
mv "$tmp" "$cask"

if [ "$verify" -eq 1 ]; then
  brew trust --cask gianniacquisto/tap/tandem
  (cd "$tap_dir" && brew style Casks/tandem.rb)
  brew audit --cask --strict gianniacquisto/tap/tandem
fi

git -C "$tap_dir" add Casks/tandem.rb
git -C "$tap_dir" \
  -c user.name=github-actions[bot] \
  -c user.email=41898282+github-actions[bot]@users.noreply.github.com \
  commit --quiet -m "tandem $version"
if [ -n "$token_remote" ]; then
  # The clone's push authenticates through the tokenised URL; the EXIT trap
  # rewrites origin to the plain one afterwards, on success or failure alike.
  git -C "$tap_dir" push --quiet "$token_remote" HEAD:main
else
  git -C "$tap_dir" push --quiet origin HEAD:main
fi

echo "bump-tap.sh: pushed tandem $version to the tap"
