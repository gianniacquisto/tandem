#!/bin/sh
# The scripted install (TAN-253): `curl -fsSL <release>/install.sh | sh`.
# Published by the same release run that publishes the zip and the DMG: the
# build job stages this file into dist with the tag version stamped in (see
# Scripts/stage-release-assets.sh), and the release job attaches it beside the
# zip, the DMG and SHA256SUMS. The copy in the repo installs `latest`; a staged
# copy installs the tag it shipped with unless --version or TANDEM_VERSION
# says otherwise, so no hand-run step and no second artifact to drift.
#
# Usage: install.sh [--version <1.2.3|v1.2.3|latest>] [--reinstall] [--uninstall] [--help]
#   --version X   install tag vX (default: the staged tag, or latest from repo)
#   --reinstall   replace an existing /Applications/Tandem.app, cask or not
#   --uninstall   remove the app the script installed; defers to the cask
#
# The installer takes the zip, never the DMG: only the zip has a line in
# SHA256SUMS, and the DMG stays unverifiable until the workflow sums it. The
# zip's digest line is checked before anything is written, and a mismatch or
# a missing line refuses. The bundle is copied to /Applications, the
# quarantine flag is stripped (every download is quarantined until then), and
# the tandem-cli link state is reported, never changed: the cask owns its own
# link and this script does not touch it. Host links and ~/.tandem stay on
# uninstall; that is the user's data, not the app's.
#
# Test seams (the suite Scripts/test_install.sh drives these; production never
# sets them): TANDEM_BASE_URL, TANDEM_APPLICATIONS_DIR, TANDEM_FAKE_UNAME_S,
# TANDEM_FAKE_UNAME_M, TANDEM_ALLOW_NON_MACOS, TANDEM_SKIP_QUARANTINE_STRIP.
set -eu

# Stamped by Scripts/stage-release-assets.sh at release time. In the repo this
# reads "latest"; a staged copy names its tag's version.
DEFAULT_VERSION="latest"

BASE_URL=${TANDEM_BASE_URL:-https://github.com/gianniacquisto/tandem/releases}
APPS_DIR=${TANDEM_APPLICATIONS_DIR:-/Applications}

version=""
reinstall=0
uninstall=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || { echo "install.sh: --version needs a value" >&2; exit 2; }
      version=$2
      shift 2
      ;;
    --reinstall) reinstall=1; shift ;;
    --uninstall) uninstall=1; shift ;;
    --help|-h)
      sed -n '2,22p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "install.sh: unknown argument: $1 (try --help)" >&2
      exit 2
      ;;
  esac
done
if [ -z "$version" ]; then
  version=${TANDEM_VERSION:-$DEFAULT_VERSION}
fi

os=${TANDEM_FAKE_UNAME_S:-$(uname -s)}
arch=${TANDEM_FAKE_UNAME_M:-$(uname -m)}
if [ "$os" != "Darwin" ] || [ "$arch" != "arm64" ]; then
  if [ -z "${TANDEM_ALLOW_NON_MACOS:-}" ]; then
    echo "install.sh: Tandem needs macOS on Apple silicon (found $os $arch); this script installs macOS only." >&2
    exit 1
  fi
fi

cask_present() {
  command -v brew >/dev/null 2>&1 && brew list --cask gianniacquisto/tap/tandem >/dev/null 2>&1
}

if [ "$uninstall" -eq 1 ]; then
  if cask_present; then
    echo "install.sh: the Homebrew cask owns this install; remove it with:" >&2
    echo "install.sh:   brew uninstall --cask gianniacquisto/tap/tandem" >&2
    exit 1
  fi
  if [ -e "$APPS_DIR/Tandem.app" ]; then
    rm -rf "$APPS_DIR/Tandem.app"
    echo "install.sh: removed $APPS_DIR/Tandem.app; ~/.tandem (the store, the notes) and the host links stay, they are yours."
  else
    echo "install.sh: nothing to remove at $APPS_DIR/Tandem.app."
  fi
  exit 0
fi

# Resolve `latest` through the releases API: the one-liner's URL cannot name
# a version, so the script asks GitHub which tag is newest.
case "$version" in
  latest)
    tag=$(curl -fsSL https://api.github.com/repos/gianniacquisto/tandem/releases/latest \
      | sed -n 's/.*"tag_name": *"\(v[0-9][^"]*\)".*/\1/p' | head -1)
    if [ -z "$tag" ]; then
      echo "install.sh: could not resolve the latest release" >&2
      exit 1
    fi
    ;;
  v*) tag=$version ;;
  *) tag="v$version" ;;
esac
ver=${tag#v}

if [ -e "$APPS_DIR/Tandem.app" ] && [ "$reinstall" -eq 0 ]; then
  echo "install.sh: $APPS_DIR/Tandem.app already exists; leaving it alone."
  if cask_present; then
    echo "install.sh: the Homebrew cask provides it; prefer 'brew update && brew upgrade --cask gianniacquisto/tap/tandem'."
  fi
  echo "install.sh: to replace it with $tag, re-run with --reinstall."
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

echo "install.sh: fetching Tandem $tag ..."
curl -fsSL -o "$tmp/SHA256SUMS" "$BASE_URL/download/$tag/SHA256SUMS"
curl -fsSL -o "$tmp/Tandem-$ver.zip" "$BASE_URL/download/$tag/Tandem-$ver.zip"

# The digest line for this zip, and only it: the sums file also covers
# install.sh, which is not on this machine, so the whole file cannot verify.
if ! grep -E -q "^[0-9a-f]{64}  Tandem-$ver\\.zip$" "$tmp/SHA256SUMS"; then
  echo "install.sh: SHA256SUMS has no digest line for Tandem-$ver.zip; refusing." >&2
  exit 1
fi
grep -E "^[0-9a-f]{64}  Tandem-$ver\\.zip$" "$tmp/SHA256SUMS" > "$tmp/check"
if ! (cd "$tmp" && shasum -a 256 -c check >/dev/null); then
  echo "install.sh: checksum mismatch for Tandem-$ver.zip; refusing before anything is written." >&2
  exit 1
fi

unzip -q "$tmp/Tandem-$ver.zip" -d "$tmp/unzipped"
bundle="$tmp/unzipped/Tandem.app"
if [ ! -x "$bundle/Contents/MacOS/Tandem" ] \
  || [ ! -f "$bundle/Contents/Resources/tandem" ] \
  || [ ! -L "$bundle/Contents/MacOS/tandem-cli" ] \
  || [ ! -f "$bundle/Contents/Resources/agent/AGENTS.md" ]; then
  echo "install.sh: the zip's bundle is not the expected layout; refusing." >&2
  exit 1
fi

if [ "$reinstall" -eq 1 ] && [ -e "$APPS_DIR/Tandem.app" ]; then
  if cask_present; then
    echo "install.sh: replacing the cask's app with $tag; a later 'brew upgrade --cask gianniacquisto/tap/tandem' restores the cask's own bytes."
  fi
  rm -rf "$APPS_DIR/Tandem.app"
fi
if ! cp -R "$bundle" "$APPS_DIR/Tandem.app"; then
  echo "install.sh: could not write to $APPS_DIR; re-run with sudo." >&2
  exit 1
fi

if [ -z "${TANDEM_SKIP_QUARANTINE_STRIP:-}" ]; then
  xattr -dr com.apple.quarantine "$APPS_DIR/Tandem.app" \
    || echo "install.sh: warning: the quarantine strip failed; run 'xattr -dr com.apple.quarantine /Applications/Tandem.app' by hand." >&2
fi

echo "install.sh: installed $APPS_DIR/Tandem.app ($tag)."
if command -v tandem-cli >/dev/null 2>&1; then
  echo "install.sh: tandem-cli is on PATH at $(command -v tandem-cli)."
else
  echo "install.sh: tandem-cli is not on PATH; the bundle carries it at"
  echo "install.sh:   $APPS_DIR/Tandem.app/Contents/MacOS/tandem-cli"
  echo "install.sh: link it with:"
  echo "install.sh:   ln -s \"$APPS_DIR/Tandem.app/Contents/MacOS/tandem-cli\" /usr/local/bin/tandem-cli"
fi
echo "install.sh: next: tandem-cli setup install (the agent face), then tandem-cli doctor (the health check)."
