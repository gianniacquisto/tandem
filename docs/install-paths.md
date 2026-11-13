# Installing Tandem

**Status**: the standalone binary is the only install path (CUT-16, 2026-11-13). The Homebrew cask and its tap were retired with the Electron tree ([ADR 0046](adr/0046-tandem-serve-and-single-binary-distribution.md)); this page replaces the 2026-11-01 comparison between the cask, a piped script and a hand-run zip.

A release publishes three archives, `install.sh` and one `SHA256SUMS`:

| asset | for | verified by |
| --- | --- | --- |
| `tandem-<version>-darwin-arm64.tar.gz` | macOS on Apple silicon | a line in `SHA256SUMS` |
| `tandem-<version>-linux-x86_64.tar.gz` | Linux on x86_64 | a line in `SHA256SUMS` |
| `tandem-<version>-windows-x86_64.zip` | Windows on x86_64 | a line in `SHA256SUMS` |
| `install.sh` | macOS and Linux, one line | a line in `SHA256SUMS`, and `DEFAULT_VERSION` stamped to the tag |
| `SHA256SUMS` | the manifest | the `release` job runs the digest check over `dist` before publishing |

## The one-liner

```sh
curl -fsSL https://github.com/gianniacquisto/tandem/releases/latest/download/install.sh | sh
```

The script asks GitHub for the newest tag, downloads this machine's archive, and verifies its SHA-256 line from the release's own `SHA256SUMS` before it writes anything. A mismatch, a missing line, or a platform with no archive refuses with exit `1` and leaves the install directory untouched.

It installs `tandem` into `~/.local/bin` and links `tandem-cli` to it, so both names reach the same binary. The destination moves with `TANDEM_INSTALL_DIR` or `--bin-dir <dir>`.

```sh
sh install.sh --version 0.33.0          # a specific tag
sh install.sh --bin-dir "$HOME/bin"     # another destination
sh install.sh --reinstall               # replace an existing tandem
sh install.sh --uninstall               # remove the binary and the link
```

`--uninstall` never touches `~/.tandem`: the store and the notes are the user's data, not the binary's. An existing `tandem` is left alone unless `--reinstall` is named.

On macOS the script strips `com.apple.quarantine` from the installed binary; an archive unpacked by hand needs `xattr -dr com.apple.quarantine <path>` once.

## Windows

Download `tandem-<version>-windows-x86_64.zip`, extract `tandem.exe`, and put it on PATH. There is no PowerShell installer yet; the archive's digest is the same `SHA256SUMS` line, checked with `Get-FileHash -Algorithm SHA256`.

## Verify by hand

```sh
curl -fsSLO https://github.com/gianniacquisto/tandem/releases/download/v0.33.0/tandem-0.33.0-darwin-arm64.tar.gz
curl -fsSLO https://github.com/gianniacquisto/tandem/releases/download/v0.33.0/SHA256SUMS
grep 'tandem-0.33.0-darwin-arm64.tar.gz$' SHA256SUMS | shasum -a 256 -c
```

The manifest covers every platform's archive and `install.sh`, so the `grep` narrows it to the file this machine downloaded before `shasum` checks it.

## Why the cask went

The cask existed because the shipped artifact was a macOS app bundle. Once the UI is served by the binary itself, the bundle, the tap repository and the `TAP_TOKEN` secret only add a second distribution path to keep correct. One archive per platform, one manifest and one script is the whole surface, and the binary is the same executable the CI gates test.

The old comparison's concerns still hold where they apply: the release's manifest is fetched from the same release as the archive, so TLS to GitHub is the transport's authentication, not a second repository's digest. That is the trade the standalone binary makes, and the digest check is the guarantee it keeps: nothing is written before the archive matches its published line.
