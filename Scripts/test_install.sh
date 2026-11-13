#!/usr/bin/env bash
# Tests for Scripts/install.sh, Scripts/stage-release-assets.sh and the syntax
# of Scripts/smoke-binary.sh (CUT-16). The install is the standalone `tandem`
# binary now, not a macOS app bundle.
#
# Run from anywhere: `bash Scripts/test_install.sh`. Each case builds a fixture
# release (an archive plus SHA256SUMS under file://) and drives the installer
# with the seams it documents: TANDEM_BASE_URL, TANDEM_INSTALL_DIR,
# TANDEM_FAKE_UNAME_S/_M and TANDEM_SKIP_QUARANTINE_STRIP. Nothing touches the
# network, a real home directory or the repository.
set -u

repo=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
installer=$repo/Scripts/install.sh
stager=$repo/Scripts/stage-release-assets.sh
smoke=$repo/Scripts/smoke-binary.sh

# Hermetic PATH: /usr/bin plus /bin only, so a developer machine's tools cannot
# leak into a test.
emptybin=$(mktemp -d)
sealed_path="$emptybin:/usr/bin:/bin"

pass=0
fail=0

ok() { pass=$((pass + 1)); echo "ok: $1"; }
no() { fail=$((fail + 1)); echo "NOT OK: $1" >&2; }

# sum256: the one digest tool the harness has, spelled for both platforms.
sum256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$@"
  else
    shasum -a 256 "$@"
  fi
}

# make_fixture <dir> <version> <suffix>: writes a fake `tandem` executable,
# packs it as tandem-<version>-<suffix>, and writes the served SHA256SUMS under
# download/v<version>/. The suffix is the platform part of the archive's name.
make_fixture() {
  local dir=$1
  local ver=$2
  local suffix=$3
  local archive="tandem-$ver-$suffix"
  mkdir -p "$dir/src"
  printf '#!/bin/sh\nif [ "$1" = "--version" ]; then echo "tandem %s"; exit 0; fi\necho fixture\n' "$ver" \
    > "$dir/src/tandem"
  chmod +x "$dir/src/tandem"
  tar -czf "$dir/$archive" -C "$dir/src" tandem
  local rel="$dir/download/v$ver"
  mkdir -p "$rel"
  cp "$dir/$archive" "$rel/"
  (cd "$rel" && sum256 "$archive" > SHA256SUMS)
}

# installer_env <install-dir> <fixture-dir> <os> <arch>: the common test seams.
installer_env() {
  echo "TANDEM_BASE_URL=file://$2"
  echo "TANDEM_INSTALL_DIR=$1"
  echo "TANDEM_FAKE_UNAME_S=$3"
  echo "TANDEM_FAKE_UNAME_M=$4"
  echo "TANDEM_SKIP_QUARANTINE_STRIP=1"
  echo "PATH=$sealed_path"
}

run_installer() {
  while IFS='=' read -r k v; do export "$k=$v"; done < <(installer_env "$1" "$2" "$3" "$4")
  shift 4
  sh "$installer" "$@" 2>&1
}

# --- syntax ---

if sh -n "$installer" 2>/dev/null; then ok "install.sh parses under sh"; else no "install.sh parses under sh"; fi
if sh -n "$stager" 2>/dev/null; then ok "stage-release-assets.sh parses under sh"; else no "stage-release-assets.sh parses under sh"; fi
if sh -n "$smoke" 2>/dev/null; then ok "smoke-binary.sh parses under sh"; else no "smoke-binary.sh parses under sh"; fi

# --- a tampered archive refuses before anything is written ---

t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
make_fixture "$t/tamper" "9.9.9" "darwin-arm64.tar.gz"
printf 'x' >> "$t/tamper/download/v9.9.9/tandem-9.9.9-darwin-arm64.tar.gz"
mkdir -p "$t/apps"
out=$(run_installer "$t/apps" "$t/tamper" Darwin arm64 --version 9.9.9; echo "exit=$?")
case "$out" in
  *"exit=0"*) no "tampered checksum refuses (exit was 0)" ;;
  *) ok "tampered checksum refuses" ;;
esac
case "$out" in
  *[Mm]"ismatch"*|*"refus"*) ok "tampered checksum says why" ;;
  *) no "tampered checksum says why" ;;
esac
if [ -e "$t/apps/tandem" ]; then no "tampered checksum writes nothing"; else ok "tampered checksum writes nothing"; fi

# --- a missing digest line refuses before anything is written ---

t2=$(mktemp -d)
make_fixture "$t2/missing" "9.9.10" "darwin-arm64.tar.gz"
printf 'deadbeef  tandem-0.0.0-darwin-arm64.tar.gz\n' > "$t2/missing/download/v9.9.10/SHA256SUMS"
mkdir -p "$t2/apps"
out2=$(run_installer "$t2/apps" "$t2/missing" Darwin arm64 --version 9.9.10; echo "exit=$?")
case "$out2" in
  *"exit=0"*) no "missing digest refuses" ;;
  *) ok "missing digest refuses" ;;
esac
if [ -e "$t2/apps/tandem" ]; then no "missing digest writes nothing"; else ok "missing digest writes nothing"; fi

# --- an unsupported platform is refused before downloading ---

t3=$(mktemp -d)
mkdir -p "$t3/apps"
out3=$(run_installer "$t3/apps" "$t2/missing" FreeBSD x86_64 --version 9.9.10; echo "exit=$?")
case "$out3" in
  *"exit=0"*) no "unsupported platform is refused" ;;
  *) ok "unsupported platform is refused" ;;
esac
case "$out3" in
  *"FreeBSD"*|*"no standalone archive"*) ok "unsupported platform says which target" ;;
  *) no "unsupported platform says which target: $out3" ;;
esac
if [ -e "$t3/apps/tandem" ]; then no "unsupported platform writes nothing"; else ok "unsupported platform writes nothing"; fi

# --- a valid archive installs on Darwin arm64 and on Linux x86_64 ---

for target in "Darwin arm64 darwin-arm64.tar.gz" "Linux x86_64 linux-x86_64.tar.gz"; do
  set -- $target
  os=$1
  arch=$2
  suffix=$3
  dir=$(mktemp -d)
  make_fixture "$dir/fix" "9.9.11" "$suffix"
  mkdir -p "$dir/bin"
  out4=$(run_installer "$dir/bin" "$dir/fix" "$os" "$arch" --version 9.9.11; echo "exit=$?")
  case "$out4" in
    *"exit=0"*) ok "$os $arch installs" ;;
    *) no "$os $arch installs: $out4" ;;
  esac
  if [ -x "$dir/bin/tandem" ]; then ok "$os $arch installs the executable"; else no "$os $arch installs the executable"; fi
  if [ -L "$dir/bin/tandem-cli" ] && [ "$(readlink "$dir/bin/tandem-cli")" = "tandem" ]; then
    ok "$os $arch links tandem-cli"
  else
    no "$os $arch links tandem-cli"
  fi
  case "$out4" in
    *"tandem-cli"*) ok "$os $arch reports the tandem-cli link" ;;
    *) no "$os $arch reports the tandem-cli link" ;;
  esac
  rm -rf "$dir"
done

# --- an existing install is left alone without --reinstall ---

t4=$(mktemp -d)
make_fixture "$t4/fix" "9.9.12" "darwin-arm64.tar.gz"
mkdir -p "$t4/bin"
run_installer "$t4/bin" "$t4/fix" Darwin arm64 --version 9.9.12 >/dev/null
printf 'mine' > "$t4/bin/tandem"
out5=$(run_installer "$t4/bin" "$t4/fix" Darwin arm64 --version 9.9.12; echo "exit=$?")
case "$out5" in
  *"exit=0"*) ok "existing install exits zero" ;;
  *) no "existing install exits zero: $out5" ;;
esac
if [ "$(cat "$t4/bin/tandem")" = "mine" ]; then ok "existing install is left alone"; else no "existing install is left alone"; fi
case "$out5" in
  *"already"*|*"--reinstall"*) ok "existing install is told plainly" ;;
  *) no "existing install is told plainly" ;;
esac

# --- --reinstall overwrites ---

out6=$(run_installer "$t4/bin" "$t4/fix" Darwin arm64 --version 9.9.12 --reinstall; echo "exit=$?")
case "$out6" in
  *"exit=0"*) ok "--reinstall exits zero" ;;
  *) no "--reinstall exits zero: $out6" ;;
esac
if [ -x "$t4/bin/tandem" ] && [ "$("$t4/bin/tandem" --version)" = "tandem 9.9.12" ]; then
  ok "--reinstall overwrites"
else
  no "--reinstall overwrites"
fi

# --- --bin-dir chooses the destination over TANDEM_INSTALL_DIR ---

t5=$(mktemp -d)
make_fixture "$t5/fix" "9.9.13" "linux-x86_64.tar.gz"
mkdir -p "$t5/env" "$t5/chosen"
out7=$(run_installer "$t5/env" "$t5/fix" Linux x86_64 --version 9.9.13 --bin-dir "$t5/chosen"; echo "exit=$?")
case "$out7" in
  *"exit=0"*) ok "--bin-dir exits zero" ;;
  *) no "--bin-dir exits zero: $out7" ;;
esac
if [ -x "$t5/chosen/tandem" ] && [ ! -e "$t5/env/tandem" ]; then ok "--bin-dir wins over TANDEM_INSTALL_DIR"; else no "--bin-dir wins over TANDEM_INSTALL_DIR"; fi

# --- --uninstall removes the binary and the link, and leaves the home ---

t6=$(mktemp -d)
make_fixture "$t6/fix" "9.9.14" "darwin-arm64.tar.gz"
mkdir -p "$t6/bin" "$t6/state"
printf 'keep' > "$t6/state/marker"
run_installer "$t6/bin" "$t6/fix" Darwin arm64 --version 9.9.14 >/dev/null
out8=$(
  while IFS='=' read -r k v; do export "$k=$v"; done < <(installer_env "$t6/bin" "$t6/fix" Darwin arm64)
  export TANDEM_HOME="$t6/state"
  sh "$installer" --uninstall 2>&1
  echo "exit=$?"
)
case "$out8" in
  *"exit=0"*) ok "--uninstall exits zero" ;;
  *) no "--uninstall exits zero: $out8" ;;
esac
if [ ! -e "$t6/bin/tandem" ] && [ ! -L "$t6/bin/tandem-cli" ]; then ok "--uninstall removes the binary and the link"; else no "--uninstall removes the binary and the link"; fi
if [ "$(cat "$t6/state/marker" 2>/dev/null)" = "keep" ]; then ok "--uninstall leaves the tandem home"; else no "--uninstall leaves the tandem home"; fi
case "$out8" in
  *".tandem"*|*"home"*|*"state"*) ok "--uninstall says the home stays" ;;
  *) no "--uninstall says the home stays" ;;
esac

# --- the stager stamps the script and sums every archive plus the script ---

t7=$(mktemp -d)
mkdir -p "$t7/dist"
printf 'darwin-bytes' > "$t7/dist/tandem-1.2.3-darwin-arm64.tar.gz"
printf 'linux-bytes' > "$t7/dist/tandem-1.2.3-linux-x86_64.tar.gz"
printf 'windows-bytes' > "$t7/dist/tandem-1.2.3-windows-x86_64.zip"
printf 'unrelated' > "$t7/dist/notes.txt"
out9=$(sh "$stager" 1.2.3 "$t7/dist" >"$t7/out9.txt" 2>&1; echo "exit=$?")
case "$out9" in
  *"exit=0"*) ok "stager exits zero" ;;
  *) no "stager exits zero: $out9" ;;
esac
if [ -f "$t7/dist/install.sh" ]; then ok "stager stages install.sh"; else no "stager stages install.sh"; fi
case "$(cat "$t7/dist/install.sh" 2>/dev/null)" in
  *'DEFAULT_VERSION="1.2.3"'*) ok "stager stamps the tag version" ;;
  *) no "stager stamps the tag version" ;;
esac
if [ -f "$t7/dist/SHA256SUMS" ] && [ "$(wc -l < "$t7/dist/SHA256SUMS" | tr -d ' ')" = "4" ] \
  && grep -q "tandem-1.2.3-darwin-arm64.tar.gz" "$t7/dist/SHA256SUMS" \
  && grep -q "tandem-1.2.3-linux-x86_64.tar.gz" "$t7/dist/SHA256SUMS" \
  && grep -q "tandem-1.2.3-windows-x86_64.zip" "$t7/dist/SHA256SUMS" \
  && grep -q "install.sh" "$t7/dist/SHA256SUMS"; then
  ok "SHA256SUMS covers the three archives and the script"
else
  no "SHA256SUMS covers the three archives and the script"
fi
if (cd "$t7/dist" && sum256 -c SHA256SUMS >/dev/null 2>&1); then
  ok "staged checksums verify"
else
  no "staged checksums verify"
fi
if [ "$(cat "$t7/dist/notes.txt" 2>/dev/null)" = "unrelated" ]; then
  ok "stager leaves an unrelated file alone"
else
  no "stager leaves an unrelated file alone"
fi

# --- the stager refuses a directory with no matching archive ---

t8=$(mktemp -d)
mkdir -p "$t8/dist"
out10=$(sh "$stager" 1.2.3 "$t8/dist" >"$t8/out10.txt" 2>&1; echo "exit=$?")
case "$out10" in
  *"exit=0"*) no "stager without archives refuses" ;;
  *) ok "stager without archives refuses" ;;
esac
if [ -f "$t8/dist/SHA256SUMS" ]; then no "stager without archives writes no sums"; else ok "stager without archives writes no sums"; fi

# --- the default download base matches the release URL shape ---

if grep -q 'TANDEM_BASE_URL:-https://github.com/gianniacquisto/tandem/releases}' "$installer" \
  && ! grep -q 'TANDEM_BASE_URL:-https://github.com/gianniacquisto/tandem/releases/download}' "$installer"; then
  ok "default base builds .../releases/download/<tag>/..."
else
  no "default base builds .../releases/download/<tag>/..."
fi

if sh "$installer" --help >/dev/null 2>&1; then ok "--help exits zero"; else no "--help exits zero"; fi

rm -rf "$t" "$t2" "$t3" "$t4" "$t5" "$t6" "$t7" "$t8" "$emptybin"

echo
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
