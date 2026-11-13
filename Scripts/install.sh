#!/bin/sh
# The standalone install (CUT-16): `curl -fsSL <release>/install.sh | sh`.
#
# A tag publishes one archive per platform, `install.sh` and `SHA256SUMS`; this
# script downloads the archive for this machine, verifies its digest from the
# release's own manifest before writing anything, and installs the one `tandem`
# binary into a directory on PATH. There is no app bundle, no Homebrew cask and
# no Node toolchain in the path (ADR 0045).
#
# Usage: install.sh [--version <1.2.3|v1.2.3|latest>] [--bin-dir <dir>] [--reinstall] [--uninstall] [--help]
#   --version X   install tag vX (default: the staged tag, or latest from the repo)
#   --bin-dir D   install into D (default: $TANDEM_INSTALL_DIR, else ~/.local/bin)
#   --reinstall   replace an existing tandem in the install directory
#   --uninstall   remove tandem and the tandem-cli link; the tandem home stays
#
# Platforms: macOS on Apple silicon (darwin-arm64) and Linux on x86_64
# (linux-x86_64). Any other pair exits 1 before downloading.
#
# Test seams (Scripts/test_install.sh drives these; production never sets them):
# TANDEM_BASE_URL, TANDEM_INSTALL_DIR, TANDEM_FAKE_UNAME_S, TANDEM_FAKE_UNAME_M,
# TANDEM_SKIP_QUARANTINE_STRIP.
set -eu

# Stamped by Scripts/stage-release-assets.sh at release time. In the repo this
# reads "latest"; a staged copy names its tag's version.
DEFAULT_VERSION="latest"

BASE_URL=${TANDEM_BASE_URL:-https://github.com/gianniacquisto/tandem/releases}

version=""
bin_dir=""
reinstall=0
uninstall=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || { echo "install.sh: --version needs a value" >&2; exit 2; }
      version=$2
      shift 2
      ;;
    --bin-dir)
      [ "$#" -ge 2 ] || { echo "install.sh: --bin-dir needs a directory" >&2; exit 2; }
      bin_dir=$2
      shift 2
      ;;
    --reinstall) reinstall=1; shift ;;
    --uninstall) uninstall=1; shift ;;
    --help|-h)
      sed -n '2,24p' "$0" | sed 's/^# \?//'
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

install_dir=${bin_dir:-${TANDEM_INSTALL_DIR:-$HOME/.local/bin}}

# The two files this script leaves behind, and the home it never touches.
if [ "$uninstall" -eq 1 ]; then
  if [ -e "$install_dir/tandem" ] || [ -L "$install_dir/tandem-cli" ]; then
    rm -f "$install_dir/tandem" "$install_dir/tandem-cli"
    echo "install.sh: removed $install_dir/tandem and $install_dir/tandem-cli."
  else
    echo "install.sh: nothing to remove in $install_dir."
  fi
  echo "install.sh: your tandem home (~/.tandem, the store and the notes) stays untouched; it is yours."
  exit 0
fi

os=${TANDEM_FAKE_UNAME_S:-$(uname -s)}
arch=${TANDEM_FAKE_UNAME_M:-$(uname -m)}
case "$os/$arch" in
  Darwin/arm64) suffix=darwin-arm64.tar.gz ;;
  Linux/x86_64) suffix=linux-x86_64.tar.gz ;;
  *)
    echo "install.sh: no standalone tandem archive for $os $arch; releases cover macOS arm64 and Linux x86_64." >&2
    exit 1
    ;;
esac

# Resolve `latest` through the releases API: the one-liner's URL cannot name a
# version, so the script asks GitHub which tag is newest.
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
case "$ver" in
  ""|*[!0-9A-Za-z.-]*)
    echo "install.sh: '$version' is not a version this script can install" >&2
    exit 2
    ;;
esac
archive="tandem-$ver-$suffix"

if [ -e "$install_dir/tandem" ] && [ "$reinstall" -eq 0 ]; then
  echo "install.sh: $install_dir/tandem already exists; leaving it alone."
  echo "install.sh: to replace it with $tag, re-run with --reinstall."
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

# The digest tool: sha256sum where the platform ships it, shasum elsewhere.
sum256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$@"
  else
    shasum -a 256 "$@"
  fi
}

echo "install.sh: fetching tandem $tag for $os $arch ..."
curl -fsSL -o "$tmp/SHA256SUMS" "$BASE_URL/download/$tag/SHA256SUMS"
curl -fsSL -o "$tmp/$archive" "$BASE_URL/download/$tag/$archive"

# The digest line for this archive, and only it: the manifest also covers
# install.sh, which is not on this machine, so the whole file cannot verify.
digest=$(awk -v want="$archive" '$2 == want { print $1 }' "$tmp/SHA256SUMS")
if [ -z "$digest" ]; then
  echo "install.sh: SHA256SUMS has no digest line for $archive; refusing." >&2
  exit 1
fi
case "$digest" in
  *[!0-9a-f]*) echo "install.sh: the digest for $archive is not 64 lowercase hex; refusing." >&2; exit 1 ;;
esac
if [ "${#digest}" -ne 64 ]; then
  echo "install.sh: the digest for $archive is not 64 lowercase hex; refusing." >&2
  exit 1
fi
printf '%s  %s\n' "$digest" "$archive" > "$tmp/check"
if ! (cd "$tmp" && sum256 -c check >/dev/null); then
  echo "install.sh: checksum mismatch for $archive; refusing before anything is written." >&2
  exit 1
fi

mkdir -p "$tmp/extract"
# The archive carries exactly one member; extract that name, never the
# archive's own paths, so a malformed archive cannot write outside the temp dir.
tar -xzf "$tmp/$archive" -C "$tmp/extract" tandem
if [ ! -f "$tmp/extract/tandem" ]; then
  echo "install.sh: $archive carries no tandem executable; refusing." >&2
  exit 1
fi

mkdir -p "$install_dir"
if ! install -m 755 "$tmp/extract/tandem" "$install_dir/tandem"; then
  echo "install.sh: could not write to $install_dir." >&2
  exit 1
fi
ln -sfn tandem "$install_dir/tandem-cli"

if [ "$os" = "Darwin" ] && [ -z "${TANDEM_SKIP_QUARANTINE_STRIP:-}" ]; then
  xattr -dr com.apple.quarantine "$install_dir/tandem" 2>/dev/null \
    || echo "install.sh: warning: the quarantine strip failed; run 'xattr -dr com.apple.quarantine $install_dir/tandem' by hand." >&2
fi

echo "install.sh: installed $install_dir/tandem ($tag)."
echo "install.sh: $install_dir/tandem-cli links to it, so both names work."
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *)
    echo "install.sh: $install_dir is not on PATH; add it, for example:"
    echo "install.sh:   export PATH=\"$install_dir:\$PATH\""
    ;;
esac
echo "install.sh: next: tandem serve (the board) or tandem doctor (the health check)."
