# Installing Tandem

**Status**: the standalone binary is the only install path (CUT-16, 2026-11-13). The Homebrew cask and its tap were retired with the Electron tree ([ADR 0046](adr/0046-tandem-serve-and-single-binary-distribution.md)); this page replaces the 2026-11-01 comparison between the cask, a piped script and a hand-run zip.

A release publishes five archives, `install.sh` and one `SHA256SUMS`:

| asset | for | verified by |
| --- | --- | --- |
| `tandem-<version>-darwin-arm64.tar.gz` | macOS on Apple silicon | a line in `SHA256SUMS` |
| `tandem-<version>-darwin-x86_64.tar.gz` | macOS on Intel (cross-built) | a line in `SHA256SUMS` |
| `tandem-<version>-linux-x86_64.tar.gz` | Linux on x86_64, static musl | a line in `SHA256SUMS` |
| `tandem-<version>-linux-arm64.tar.gz` | Linux on arm64, static musl | a line in `SHA256SUMS` |
| `tandem-<version>-windows-x86_64.zip` | Windows on x86_64 | a line in `SHA256SUMS` |
| `install.sh` | macOS and Linux, one line | a line in `SHA256SUMS`, and `DEFAULT_VERSION` stamped to the tag |
| `SHA256SUMS` | the manifest | the `release` job runs the digest check over `dist` before publishing |

## The one-liner

```sh
curl -fsSL https://github.com/gianniacquisto/tandem/releases/latest/download/install.sh | sh
```

The script asks GitHub for the newest tag, downloads this machine's archive, and verifies its SHA-256 line from the release's own `SHA256SUMS` before it writes anything. A mismatch, a missing line, or a platform with no archive refuses with exit `1` and leaves the install directory untouched.

It installs `tandem` into `~/.local/bin` and links `tandem-cli` to it, so both names reach the same binary. The destination moves with `TANDEM_INSTALL_DIR` or `--bin-dir <dir>`.

## Install, upgrade, reinstall, uninstall

The script has three modes. Each one only ever touches `tandem` and the `tandem-cli` link in the install directory, never `~/.tandem`.

| you want | run | what happens |
| --- | --- | --- |
| first install | the one-liner above | installs the newest release; exits `0` |
| upgrade, or repair a broken binary | the one-liner with `--reinstall` | replaces `tandem` with the newest release |
| a specific version, including a downgrade | `--reinstall --version <x.y.z>` | replaces `tandem` with that tag |
| remove | `--uninstall` | removes `tandem` and the `tandem-cli` link |

**Running the installer again is not an upgrade.** If `tandem` is already in the install directory and `--reinstall` is not named, the script prints `already exists; leaving it alone`, downloads nothing, and exits `0`. That exit code is a success, so a script or a second paste of the one-liner looks like it worked while changing nothing. Name `--reinstall` to replace the binary.

Flags reach the script through `sh -s --` when you pipe it from `curl`:

```sh
# upgrade to the newest release
curl -fsSL https://github.com/gianniacquisto/tandem/releases/latest/download/install.sh | sh -s -- --reinstall

# pin a version (also how you roll back)
curl -fsSL https://github.com/gianniacquisto/tandem/releases/latest/download/install.sh | sh -s -- --reinstall --version 2.4.0

# remove the binary and the link
curl -fsSL https://github.com/gianniacquisto/tandem/releases/latest/download/install.sh | sh -s -- --uninstall
```

With the script saved to disk the same flags go straight on it:

```sh
sh install.sh --version 2.4.0           # a specific tag (first install)
sh install.sh --bin-dir "$HOME/bin"     # another destination
sh install.sh --reinstall               # replace an existing tandem
sh install.sh --uninstall               # remove the binary and the link
```

Things to know around an upgrade:

- `--bin-dir` (or `TANDEM_INSTALL_DIR`) must match on every run. The script looks only in that directory, so installing into a second directory leaves the first copy in place, and whichever comes first on PATH wins. `which -a tandem` lists every copy; `tandem --version` names the one you are running.
- A running `tandem serve` keeps the old binary until you stop it and start it again. Restart it after an upgrade, or the board and your agents keep talking to the old build ([upgrading the store](store.md#upgrading)).
- The newer binary migrates the store on its first write, and an older binary then refuses the store. A rollback with `--version` after that needs the backup, not just the old binary: see [the backup](store.md#the-backup).
- `--uninstall` refuses to remove a `tandem` that does not identify itself as this project's binary, and says so.
- Exit codes: `0` installed, removed, or left alone; `1` a refusal (checksum, platform, write failure, no latest release found); `2` a bad argument; a failed download, such as a version with no release, exits with curl's own nonzero code.

`--uninstall` never touches `~/.tandem`: the store and the notes are the user's data, not the binary's. Delete the folder by hand only if you mean to lose them.

On macOS the script strips `com.apple.quarantine` from the installed binary; an archive unpacked by hand needs `xattr -dr com.apple.quarantine <path>` once.

## Windows

`install.sh` does not run on Windows. Download `tandem-<version>-windows-x86_64.zip`, extract `tandem.exe`, and put it on PATH. To upgrade, stop `tandem serve` first (Windows will not overwrite a running executable), then replace `tandem.exe`. There is no PowerShell installer yet; the archive's digest is the same `SHA256SUMS` line, checked with `Get-FileHash -Algorithm SHA256`.

## Verify by hand

```sh
curl -fsSLO https://github.com/gianniacquisto/tandem/releases/download/v2.4.0/tandem-2.4.0-darwin-arm64.tar.gz
curl -fsSLO https://github.com/gianniacquisto/tandem/releases/download/v2.4.0/SHA256SUMS
grep 'tandem-2.4.0-darwin-arm64.tar.gz$' SHA256SUMS | shasum -a 256 -c
```

The manifest covers every platform's archive and `install.sh`, so the `grep` narrows it to the file this machine downloaded before `shasum` checks it.

## Verify the build's provenance

`SHA256SUMS` proves the download is whole; it does not say who built it. Each tag's five archives also carry a signed build-provenance attestation, made by the Release workflow with GitHub's `actions/attest-build-provenance`. With the GitHub CLI:

```sh
gh attestation verify tandem-2.4.0-darwin-arm64.tar.gz --repo gianniacquisto/tandem
```

A success names the workflow run and the commit that built the archive; a failure means the file did not come from this repository's release workflow. `install.sh` itself is not signed and does not run this check.

## Why the cask went

The cask existed because the shipped artifact was a macOS app bundle. Once the UI is served by the binary itself, the bundle, the tap repository and the `TAP_TOKEN` secret only add a second distribution path to keep correct. One archive per platform, one manifest and one script is the whole surface, and the binary is the same executable the CI gates test.

The old comparison's concerns still hold where they apply: the release's manifest is fetched from the same release as the archive, so TLS to GitHub is the transport's authentication, not a second repository's digest. That is the trade the standalone binary makes, and the digest check is the guarantee it keeps: nothing is written before the archive matches its published line.
