#!/usr/bin/env bash
# Tests for Scripts/install.sh and Scripts/stage-release-assets.sh (TAN-253).
#
# Run from anywhere: `bash Scripts/test_install.sh`. Each test builds a fixture
# release (a zip plus SHA256SUMS under file://) and drives the installer with
# the test seams it documents: TANDEM_BASE_URL, TANDEM_APPLICATIONS_DIR,
# TANDEM_FAKE_UNAME_S/_M, TANDEM_ALLOW_NON_MACOS and
# TANDEM_SKIP_QUARANTINE_STRIP. Nothing touches /Applications or the network.
set -u

repo=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
installer=$repo/Scripts/install.sh
stager=$repo/Scripts/stage-release-assets.sh

# Hermetic PATH: /usr/bin plus /bin only, so a developer machine's brew (and
# its cask) cannot leak into a test. Tests that need a brew provide a fake one
# earlier on PATH.
emptybin=$(mktemp -d)
sealed_path="$emptybin:/usr/bin:/bin"

pass=0
fail=0

ok() { pass=$((pass + 1)); echo "ok: $1"; }
no() { fail=$((fail + 1)); echo "NOT OK: $1" >&2; }

# make_fixture <dir> <version>: writes a minimal Tandem.app, zips it, and
# writes SHA256SUMS with the zip's real digest under download/<tag>/.
make_fixture() {
  dir=$1
  ver=$2
  tag="v$ver"
  app="$dir/src/Tandem.app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/agent"
  printf '#!/bin/sh\necho fixture\n' > "$app/Contents/MacOS/Tandem"
  chmod +x "$app/Contents/MacOS/Tandem"
  printf 'fixture-core' > "$app/Contents/Resources/tandem"
  ln -s "../Resources/tandem" "$app/Contents/MacOS/tandem-cli"
  printf '# agent face\n' > "$app/Contents/Resources/agent/AGENTS.md"
  (cd "$dir/src" && zip -qry "$dir/Tandem-$ver.zip" Tandem.app)
  rel="$dir/download/$tag"
  mkdir -p "$rel"
  cp "$dir/Tandem-$ver.zip" "$rel/"
  (cd "$rel" && shasum -a 256 "Tandem-$ver.zip" > SHA256SUMS)
}

# installer_env <apps-dir> <fixture-dir>: the common test seams.
installer_env() {
  echo "TANDEM_BASE_URL=file://$2"
  echo "TANDEM_APPLICATIONS_DIR=$1"
  echo "TANDEM_FAKE_UNAME_S=Darwin"
  echo "TANDEM_FAKE_UNAME_M=arm64"
  echo "TANDEM_ALLOW_NON_MACOS=1"
  echo "TANDEM_SKIP_QUARANTINE_STRIP=1"
  echo "PATH=$sealed_path"
}

# --- syntax ---

if sh -n "$installer" 2>/dev/null; then ok "install.sh parses under sh"; else no "install.sh parses under sh"; fi
if sh -n "$stager" 2>/dev/null; then ok "stage-release-assets.sh parses under sh"; else no "stage-release-assets.sh parses under sh"; fi

# --- tampered checksum refuses before anything is written ---

t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
make_fixture "$t/fix" "9.9.9"
# Tamper: append a byte to the served zip without touching SHA256SUMS.
printf 'x' >> "$t/fix/download/v9.9.9/Tandem-9.9.9.zip"
mkdir -p "$t/apps"
out=$(
  while IFS='=' read -r k v; do export "$k=$v"; done < <(installer_env "$t/apps" "$t/fix")
  sh "$installer" --version 9.9.9 >"$t/out.txt" 2>&1
  echo "exit=$?"
  cat "$t/out.txt"
)
case "$out" in
  *"exit=0"*) no "tampered checksum refuses (exit was 0)" ;;
  *) ok "tampered checksum refuses" ;;
esac
case "$out" in
  *[Mm]"ismatch"*|*"refus"*) ok "tampered checksum says why" ;;
  *) no "tampered checksum says why" ;;
esac
if [ -e "$t/apps/Tandem.app" ]; then no "tampered checksum writes nothing"; else ok "tampered checksum writes nothing"; fi

# --- a good checksum installs and reports the tandem-cli link state ---

t2=$(mktemp -d)
make_fixture "$t2/fix" "9.9.10"
mkdir -p "$t2/apps"
out2=$(
  while IFS='=' read -r k v; do export "$k=$v"; done < <(installer_env "$t2/apps" "$t2/fix")
  sh "$installer" --version 9.9.10 >"$t2/out.txt" 2>&1
  echo "exit=$?"
  cat "$t2/out.txt"
)
case "$out2" in
  *"exit=0"*) ok "good checksum installs" ;;
  *) no "good checksum installs: $out2" ;;
esac
if [ -x "$t2/apps/Tandem.app/Contents/MacOS/Tandem" ] && [ -L "$t2/apps/Tandem.app/Contents/MacOS/tandem-cli" ]; then
  ok "installed bundle keeps its layout"
else
  no "installed bundle keeps its layout"
fi
case "$out2" in
  *"tandem-cli"*) ok "installer reports the tandem-cli link state" ;;
  *) no "installer reports the tandem-cli link state" ;;
esac
case "$out2" in
  *"doctor"*) ok "installer points at tandem-cli doctor" ;;
  *) no "installer points at tandem-cli doctor" ;;
esac

# --- an existing install is left alone without --reinstall ---

printf 'mine' > "$t2/apps/Tandem.app/Contents/Resources/agent/AGENTS.md"
out3=$(
  while IFS='=' read -r k v; do export "$k=$v"; done < <(installer_env "$t2/apps" "$t2/fix")
  sh "$installer" --version 9.9.10 >"$t2/out3.txt" 2>&1
  echo "exit=$?"
  cat "$t2/out3.txt"
)
case "$out3" in
  *"exit=0"*) ok "existing install exits zero" ;;
  *) no "existing install exits zero: $out3" ;;
esac
if [ "$(cat "$t2/apps/Tandem.app/Contents/Resources/agent/AGENTS.md")" = "mine" ]; then
  ok "existing install is left alone"
else
  no "existing install is left alone"
fi
case "$out3" in
  *"already"*) ok "existing install is told plainly" ;;
  *) no "existing install is told plainly" ;;
esac

# --- --reinstall overwrites ---

out4=$(
  while IFS='=' read -r k v; do export "$k=$v"; done < <(installer_env "$t2/apps" "$t2/fix")
  sh "$installer" --version 9.9.10 --reinstall >"$t2/out4.txt" 2>&1
  echo "exit=$?"
)
case "$out4" in
  *"exit=0"*) ok "--reinstall exits zero" ;;
  *) no "--reinstall exits zero: $out4" ;;
esac
if [ "$(cat "$t2/apps/Tandem.app/Contents/Resources/agent/AGENTS.md")" = "# agent face" ]; then
  ok "--reinstall overwrites"
else
  no "--reinstall overwrites"
fi

# --- --uninstall removes the app and leaves data to the user ---

out5=$(
  while IFS='=' read -r k v; do export "$k=$v"; done < <(installer_env "$t2/apps" "$t2/fix")
  sh "$installer" --uninstall >"$t2/out5.txt" 2>&1
  echo "exit=$?"
  cat "$t2/out5.txt"
)
case "$out5" in
  *"exit=0"*) ok "--uninstall exits zero" ;;
  *) no "--uninstall exits zero: $out5" ;;
esac
if [ ! -e "$t2/apps/Tandem.app" ]; then ok "--uninstall removes the app"; else no "--uninstall removes the app"; fi
case "$out5" in
  *".tandem"*|*"leaves"*) ok "--uninstall says data stays" ;;
  *) no "--uninstall says data stays" ;;
esac

# --- --uninstall defers to the cask when brew owns it ---

t3=$(mktemp -d)
mkdir -p "$t3/apps" "$t3/bin"
printf '#!/bin/sh\nexit 0\n' > "$t3/bin/brew"
chmod +x "$t3/bin/brew"
mkdir -p "$t3/apps/Tandem.app"
out6=$(
  while IFS='=' read -r k v; do export "$k=$v"; done < <(installer_env "$t3/apps" "$t2/fix")
  export PATH="$t3/bin:$PATH"
  sh "$installer" --uninstall >"$t3/out6.txt" 2>&1
  echo "exit=$?"
  cat "$t3/out6.txt"
)
case "$out6" in
  *"brew uninstall"*) ok "--uninstall defers to the cask" ;;
  *) no "--uninstall defers to the cask: $out6" ;;
esac
if [ -e "$t3/apps/Tandem.app" ]; then ok "--uninstall does not take the cask's app"; else no "--uninstall does not take the cask's app"; fi

# --- a non-macOS machine is refused ---

t4=$(mktemp -d)
mkdir -p "$t4/apps"
out7=$(env -u TANDEM_ALLOW_NON_MACOS TANDEM_BASE_URL="file://$t2/fix" TANDEM_APPLICATIONS_DIR="$t4/apps" TANDEM_FAKE_UNAME_S=Linux TANDEM_FAKE_UNAME_M=x86_64 TANDEM_SKIP_QUARANTINE_STRIP=1 sh "$installer" --version 9.9.10 >"$t4/out7.txt" 2>&1; echo "exit=$?"; cat "$t4/out7.txt")
case "$out7" in
  *"exit=0"*) no "non-macOS is refused" ;;
  *) ok "non-macOS is refused" ;;
esac
if [ -e "$t4/apps/Tandem.app" ]; then no "non-macOS writes nothing"; else ok "non-macOS writes nothing"; fi

# --- a zip with no digest line is refused ---

t5=$(mktemp -d)
make_fixture "$t5/fix" "9.9.11"
printf 'deadbeef  Tandem-0.0.0.zip\n' > "$t5/fix/download/v9.9.11/SHA256SUMS"
mkdir -p "$t5/apps"
out8=$(
  while IFS='=' read -r k v; do export "$k=$v"; done < <(installer_env "$t5/apps" "$t5/fix")
  sh "$installer" --version 9.9.11 >"$t5/out8.txt" 2>&1
  echo "exit=$?"
  cat "$t5/out8.txt"
)
case "$out8" in
  *"exit=0"*) no "missing digest refuses" ;;
  *) ok "missing digest refuses" ;;
esac
if [ -e "$t5/apps/Tandem.app" ]; then no "missing digest writes nothing"; else ok "missing digest writes nothing"; fi

# --- the stager stamps the script and sums the zip plus the script ---

t6=$(mktemp -d)
mkdir -p "$t6/dist"
printf 'zip-bytes' > "$t6/dist/Tandem-1.2.3.zip"
printf 'dmg-bytes' > "$t6/dist/Tandem-1.2.3.dmg"
out9=$(sh "$stager" 1.2.3 "$t6/dist" >"$t6/out9.txt" 2>&1; echo "exit=$?"; cat "$t6/out9.txt")
case "$out9" in
  *"exit=0"*) ok "stager exits zero" ;;
  *) no "stager exits zero: $out9" ;;
esac
if [ -f "$t6/dist/install.sh" ]; then ok "stager stages install.sh"; else no "stager stages install.sh"; fi
case "$(cat "$t6/dist/install.sh" 2>/dev/null)" in
  *'DEFAULT_VERSION="1.2.3"'*) ok "stager stamps the tag version" ;;
  *) no "stager stamps the tag version" ;;
esac
if [ -f "$t6/dist/SHA256SUMS" ] && [ "$(wc -l < "$t6/dist/SHA256SUMS" | tr -d ' ')" = "2" ] \
  && grep -q "Tandem-1.2.3.zip" "$t6/dist/SHA256SUMS" && grep -q "install.sh" "$t6/dist/SHA256SUMS"; then
  ok "SHA256SUMS covers the zip and the script"
else
  no "SHA256SUMS covers the zip and the script"
fi
if (cd "$t6/dist" && shasum -a 256 -c SHA256SUMS >/dev/null 2>&1); then
  ok "staged checksums verify"
else
  no "staged checksums verify"
fi
if [ -f "$t6/dist/Tandem-1.2.3.dmg" ] && [ "$(cat "$t6/dist/Tandem-1.2.3.dmg")" = "dmg-bytes" ]; then
  ok "stager leaves the dmg alone"
else
  no "stager leaves the dmg alone"
fi

# --- the default download base matches the real release URL shape ---

if grep -q 'TANDEM_BASE_URL:-https://github.com/gianniacquisto/tandem/releases}' "$installer" \
  && ! grep -q 'TANDEM_BASE_URL:-https://github.com/gianniacquisto/tandem/releases/download}' "$installer"; then
  ok "default base builds .../releases/download/<tag>/..."
else
  no "default base builds .../releases/download/<tag>/..."
fi

if sh "$installer" --help >/dev/null 2>&1; then ok "--help exits zero"; else no "--help exits zero"; fi

rm -rf "$t" "$t2" "$t3" "$t4" "$t5" "$t6" "$emptybin"

echo
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
