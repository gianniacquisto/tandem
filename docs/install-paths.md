# Installing Tandem: the cask, a piped script, or a zip and `install.sh`

**Status**: investigated 2026-11-01, `TAN-248`. The recommendation at the end is the ticket's escalation, so the choice is the human's. No code, no installer and no workflow change rides this decision: the page is the comparison, and the follow-up ticket is filed from the answer.

The install is one line today: `brew install --cask gianniacquisto/tap/tandem`, a cask in [`gianniacquisto/homebrew-tap`](https://github.com/gianniacquisto/homebrew-tap) that the tag run writes itself ([ADR 0036](adr/0036-the-tag-bumps-the-tap.md), [`docs/releasing.md`](releasing.md) step 4). The question is whether a downloaded script serves better. An install path is outward-facing and hard to withdraw once people depend on it, so it is worth deciding on the artifact as it actually is, not on how a scripted install looks in principle.

## The artifact

A tag publishes three assets and nothing else ([`release.yml`](../.github/workflows/release.yml)):

| asset | size at v1.0.2 | verified by |
| --- | --- | --- |
| `Tandem-<version>.zip` | 119,223,329 bytes | a line in `SHA256SUMS`, and the cask's `sha256` |
| `Tandem-<version>.dmg` | 122,951,599 bytes | nothing published |
| `SHA256SUMS` | 83 bytes, one line | `release` job runs `shasum -a 256 -c` before publishing |

Four properties of those bytes decide most of the comparison:

- **The zip is the install unit, and its name is a contract.** The cask downloads `Tandem-<version>.zip`, the build job fails if that name is absent, and `Scripts/bump-tap.sh` reads exactly that filename out of `SHA256SUMS` (an extra line for another filename is ignored, a missing or doubled one is a red run). A second install path inherits the same name, not a new one.
- **Only the zip has a published digest.** `SHA256SUMS` carries one line, so the DMG is a download nobody can verify against the release's own manifest. Any path that installs from the DMG would need a workflow change first, which puts the DMG out of scope here.
- **The bundle is ad-hoc signed, sealed, and not notarized.** `codesign -dv /Applications/Tandem.app` reads `Signature=adhoc`, `TeamIdentifier=not set`, `Sealed Resources version=2`; `check:artifact` and the release job refuse a bundle that does not verify and seal. Developer ID signing and notarization are `PRT-32`, on hold for the human.
- **Every download is quarantined, so every path pays the strip.** Homebrew applies `com.apple.quarantine` to a cask download (observed on this Mac at 1.0.2), and until it is cleared the app is refused and the in-bundle `tandem-cli` is killed (exit 137) as well. The documented fix is one line, `xattr -dr com.apple.quarantine /Applications/Tandem.app`, once per install or upgrade, and each upgrade also warns that the signer changed ([ADR 0003](adr/0003-distribution.md)).

The install is also two acts, not one, on every path: the app, then the agent face. The cask puts `Tandem.app` in `/Applications` and links the bundled `tandem-cli` into Homebrew's `bin`, and `tandem-cli setup install` (or the wizard's one press) links the skills, the four prompts and `AGENTS.md` into the host ([ADR 0034](adr/0034-agent-face-in-the-artifact.md)). Nothing in the artifact configures a host by itself.

## The three paths

### 1. The Homebrew cask, as it stands

```sh
brew install --cask gianniacquisto/tap/tandem
xattr -dr com.apple.quarantine /Applications/Tandem.app
tandem-cli setup install
```

- **Upgrade**: `brew update && brew upgrade --cask gianniacquisto/tap/tandem`, then the strip again. `livecheck` against `releases/latest` is what makes `brew outdated` honest, and the tag run keeps the cask's version and digest current in the same run that publishes (ADR 0036), so there is no lag window left.
- **Uninstall**: `brew uninstall --cask gianniacquisto/tap/tandem` removes the app and the `bin` link. The cask declares no `uninstall` and no `zap` stanza, so `~/.tandem` (the store, the bikeshed, the agent copy) and the host links `setup install` wrote stay, which is the right default: that is the user's data, not the app's.
- **Verification**: brew downloads the zip and refuses it unless it matches the cask's `sha256`. That digest lives in the *tap* repo and is written by the tag run from `dist/SHA256SUMS`, so the manifest is not fetched from the same release the artifact is: a compromise has to reach two repositories. Both are one account, so this is a second step for an attacker, not a trust boundary, but it is a step a self-verifying script does not have.
- **What it costs**: Homebrew is a prerequisite nobody states (`README.md` lists macOS 27 and Apple silicon, never brew), and the token `tandem` collides with Homebrew core's unrelated `tandem`, so the tap has to be spelled out in every command. A bare `brew upgrade --cask tandem` once replaced this app with core's (`PRT-36`, the first install of v1.0.0; the full-name rule is the last paragraph of [`docs/releasing.md`](releasing.md)).
- **What it buys**: upgrades, `brew outdated`, clean uninstall of the artifacts, `depends_on macos: :golden_gate` refusing cleanly on an unsupported Mac, and `brew style` plus `brew audit --cask --strict` as gates in the tag run. The tap's own CI checks style and tap syntax only: no path in this repo install-tests the cask either.

### 2. `curl -fsSL <url> | sh`

A script served from the repo or as a release asset, run straight out of the download, which fetches the zip, verifies it, copies the bundle to `/Applications`, strips quarantine, and runs `setup install`.

- **What the user types**: one line, and the whole app *and* agent face arrive in one act. That is the honest win: it is the only path where a single command finishes the setup.
- **Upgrade and uninstall**: both become the script's job. Upgrade is a re-run against the latest release; uninstall needs a subcommand the script must define, or the user removes the bundle and the host links by hand. Neither exists until someone writes, versions and tests them.
- **Verification**: the artifact can be checked against `SHA256SUMS`, but the script cannot check anything about itself. `| sh` executes bytes whose only authentication is TLS to GitHub and the tag URL in the README. The release's own manifest sitting beside the release's own artifact proves the two agree, not who made them, so this is strictly less assurance than the cask's second-repo digest.
- **Signing state**: the script can run the `xattr` strip, which is the one friction it removes. It would then be a downloaded, unread script clearing a Gatekeeper flag on a bundle macOS has already refused, which is the act [ADR 0003](adr/0003-distribution.md) rejected when the app proposed doing it to itself. The friction belongs to `PRT-32`, and changing the install channel does not settle it.
- **Support load**: a fourth surface to keep correct on macOS versions, arm64, the quarantine states, an existing install and admin versus non-admin users, and then on Linux and Windows once `PRT-32` ships artifacts for them; plus a shell gate (`sh -n`, and the repo has no shell linter today) and a workflow edit if the script is to be published as an asset with a digest of its own.

### 3. A zip plus a hand-run `install.sh`

The same script, shipped inside the zip or the repo, run by hand after a manual download.

- **What the user types**: download, unzip, `./install.sh`, or the four commands it wraps.
- **Verification**: the only path where a human can see the digest before running anything, by checking `shasum -a 256 -c SHA256SUMS` first. A human who skips that step gets no more than the piped script, with more keystrokes.
- **Upgrade and uninstall**: the same code as path 2, invoked manually, so it carries the same support load and the same workflow question about publishing the script.
- **What it adds over the DMG**: the DMG is already the GUI download, and it is the weakest asset on the release (no digest). A zip plus `install.sh` is the manual route formalized, not a new channel. Its real use case is the user with no Homebrew, which is a gap in the docs today, not a reason for an installer.

## What the release workflow constrains

Whatever the answer, these stay true, and they are the cost side of any new path:

- **One run, one act.** The `release` job publishes and then bumps the cask as its last step; a red tap step cannot take a published release with it. A script's work happens at install time on the user's machine, so this job does not grow for it. Publishing the script would grow it: `SHA256SUMS` is verified with `shasum -a 256 -c` over `dist`, so any file it lists must also be an asset there, and the script would need its own line to be verifiable at all.
- **The name is the contract.** `Tandem-<version>.zip` is checked in the build job, read by the bump script, and pointed at by the cask's `url`. A second path that renames or repackages the artifact breaks the cask, so it must install the same zip.
- **No new credential.** Signing a script (minisign, GPG) or hosting it anywhere else adds a key or a token; the release already carries one credential, `TAP_TOKEN`, and one is enough.
- **The install line is a contract in four places**: `README.md`, `docs/releasing.md` step 6, the release-notes lead in `Scripts/release-notes.sh`, and the cask itself. Changing the path moves all four in step, and a path documented in only some of them is how the token collision got its first victim.

## Recommendation: the cask stands

One path, unchanged, with the strip and the tap-prefixed spelling as its two documented costs.

- **The security lens decides it.** Tandem is ad-hoc signed and not notarized. The cask installs a bundle brew checks against a digest held in a second repo and audited by `brew style` and `brew audit --cask --strict`; a piped script asks the user to execute unverified code to perform an install macOS is actively refusing, and its verification sits on the same release as the artifact it verifies. Shipping `| sh` for a pre-1.0 app buys convenience by spending the one property the install path has, which is that installing Tandem never runs a downloaded program.
- **The friction a script would remove is not the cask's to fix.** The Gatekeeper strip, the per-upgrade signer prompt and the "damaged" wording all come from the missing notarization. `PRT-32` owns that, and once it lands, both paths lose the strip and the script's remaining advantage is one command instead of three.
- **The collision is the cask's one real cost, and it is a documented hazard, not an install failure.** The cure that would remove it for good is renaming the token, already decided against because it breaks every existing install's upgrade path. A script would sidestep the token, but adding a second channel to avoid a command spelling is a support surface bought to hide a doc line.
- **The workflow already gates the cask end to end**, at no marginal cost: artifact name, digest, style, audit, all in the tag run. A script would be gated by nothing but a run somebody wrote for it.
- **The audience has brew.** The documented prerequisites are macOS 27 and Apple silicon; the install line is a brew one-liner and the agent face is one more command. The user who needs the scripted route is the user with no Homebrew, and today that user is undocumented rather than unserved: the zip is right there on the release.

### The gap this leaves

A user without Homebrew has no documented path at all. The cheapest fix is a short manual section in `README.md` (download the zip, verify the line in `SHA256SUMS` with `shasum -a 256 -c`, drag `Tandem.app` to `/Applications`, strip, `tandem-cli setup install`), not an installer. It is a follow-up ticket, not part of this decision.

## Decided against

- **The piped script (`curl -fsSL <url> | sh`) as the install path.** It spends the install path's only security property (nothing downloaded is executed), its verification cannot exceed the release it fetches from, its real user-facing win is automating a quarantine strip that `PRT-32` retires, and it adds a fourth tested surface plus a publishing question to the release workflow for a command whose whole content is "download the zip, copy it, run setup". Revisit only on the triggers below.
- **A zip plus a hand-run `install.sh`.** Strictly dominated: the same unverified-script cost as the piped path, with a manual download and a manual `chmod` in front of it, and no discovery. Its one use case, the no-brew user, is served better by documents and a manual section than by a script that has to be shipped, versioned and kept in step with the artifact.
- **A DMG-only install.** Out of scope on the evidence: the DMG has no line in `SHA256SUMS`, so a path that installs from it cannot verify the download until the workflow computes a second digest. A path that cannot verify its download is not a candidate.
- **An installer in this change.** The decision first; a script written before the choice argues for itself.

## What would have to change for a scripted path to make sense

Any one of these reopens the investigation, with the piped script as the candidate:

- **`PRT-32` lands a Developer ID and notarization.** Then Gatekeeper, not a hash, vouches for the payload, the strip disappears from the docs, and the piped script's residual risk is executing a short, auditable bootstrap rather than an unverifiable app. That is the shape the objection above is weakest against.
- **A managed or fleet target arrives**, where Homebrew is not allowed on the machine and the install has to be run by a provisioning tool. One non-developer machine makes a script the only single-command path.
- **Windows and Linux artifacts ship** (`PRT-32`'s NSIS installer and AppImage). brew is macOS-only, so on those platforms an installer script is not an alternative to the cask, it is the only one, and the macOS path would then be the odd one out.
- **The token collision causes a regression that documentation does not hold**: a user whose bare-token upgrade replaced this app, reported after the README and the release notes already spell the tap out. A script that never names the token is then the mitigation that does not break existing installs.

Any such change wants the same discipline this decision took: one page, the artifact's facts, and a test that installs the freshly built zip into a temporary prefix before the path is announced.
