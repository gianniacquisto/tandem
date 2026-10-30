# The tag bumps the tap: the release job rewrites the cask and pushes it

Half the release act was CI and half was a human at a terminal. A `v*` tag runs [`release.yml`](../../.github/workflows/release.yml), which verifies the build, publishes the GitHub release with `Tandem-<version>.zip` and `SHA256SUMS`, and writes the body from [`Scripts/release-notes.sh`](../../Scripts/release-notes.sh). The other half was `Casks/tandem.rb` in [`gianniacquisto/homebrew-tap`](https://github.com/gianniacquisto/homebrew-tap): a human bumped its `version` and `sha256` and ran `brew style` and `brew audit --cask --strict` before pushing (`docs/releasing.md` steps 4 and 5, done by hand for every release, tap PRs #16 to #32).

That hand step is the lag, and the lag is unbounded: v0.32.0 had no cask at all while 0.33.0 shipped, and v0.30.0 published while the tap still served 0.31.0. The tap's own `autobump.yml` cannot close it: it fires on a 06:55 UTC daily cron and on a push to itself, has no `workflow_dispatch` and no release trigger, and runs `brew bump --no-fork --open-pr`, so even when it fires it opens a PR a human merges. The human, 2026-10-29: "i want releases to be immediately available after tagging".

Everything the step needs already exists in the tagged run: the release is published, and `dist/SHA256SUMS` is the digest of the bytes just published. What was missing is permission to write the second repo, because `GITHUB_TOKEN` is scoped to the repo that mints it. That is the whole cost of immediacy.

## Decision

- **The tag run's `release` job bumps the cask itself, as its last step.** One step, tag-gated (`if: startsWith(github.ref, 'refs/tags/v')`), after the publish step: pull-request runs of the job never touch another repo, and a red tap step can never take a published release with it, because the release is already out. The job's `permissions:` is untouched: the write to the tap comes from a PAT, not from `GITHUB_TOKEN`.
- **One script owns the bump: [`Scripts/bump-tap.sh`](../../Scripts/bump-tap.sh)**, `bump-tap.sh <tag> <checksums-file> [--tap-dir <dir>] [--verify] [--remote <url>]`. The workflow is one line over it, and the same script is the recovery path for a past tag on a developer machine, so there is one code path and no `workflow_dispatch` variant.
- **The tag is parsed before anything touches the filesystem.** It must be `v<digits>.<digits>.<digits>`; the version is the tag minus the `v`. Anything else exits non-zero naming the argument.
- **The digest is the `SHA256SUMS` line for this tag's artifact.** `shasum -a 256` writes `<64 hex>  Tandem-<version>.zip`; no line for that filename, more than one, or a first field that is not 64 lowercase hex exits non-zero naming the expected filename. A digest from another release is the one wrong value that would poison a cask silently, so it is checked against the tag rather than trusted.
- **The clone is brew's, and only the clone needs the token.** The default `--tap-dir` is `$(brew --prefix)/Library/Taps/gianniacquisto/homebrew-tap`, because brew loads a cask only from its tap tree (`brew audit` refuses a bare path: `Error: Calling brew audit [path ...] is disabled!`). Absent, it is cloned from `--remote`, whose default is `https://x-access-token:$TAP_TOKEN@github.com/gianniacquisto/homebrew-tap.git`; the push authenticates through that same tokenised URL, and a trap rewrites the clone's `origin` to the plain URL on every exit path, so the token lives nowhere on disk afterwards. Present, the directory is fetched and reset to `origin/main`, the path a re-bump of a past tag takes on a developer machine.
- **`TAP_TOKEN` unset or empty with no `--remote` fails before any clone**, naming the secret, what to create and where to set it. A warning or a silent skip would be the stall this replaces.
- **Exactly two lines change.** `  version "X.Y.Z"` and `  sha256 "<digest>"`; every other byte of the cask stays, including the tap-prefixed `url` that the token collision with Homebrew core's `tandem` depends on. A cask missing either line exits non-zero naming the file, with nothing committed.
- **The gates run on the runner, before the push, in one order**: `brew trust --cask gianniacquisto/tap/tandem`, then `brew style Casks/tandem.rb`, then `brew audit --cask --strict gianniacquisto/tap/tandem`. Trust is not optional on a fresh runner: a clone brew has never tapped is an untrusted tap and the audit refuses to load its cask (`Error: Refusing to load cask gianniacquisto/tap/tandem from untrusted tap`), while `brew trust --cask` is non-interactive and exits 0.
- **A cask already at the tag's version and digest prints up to date and exits 0**, committing and pushing nothing, so re-cutting the same tag stays green.
- **Otherwise it commits as `github-actions[bot]`** (`user.name=github-actions[bot]`, `user.email=41898282+github-actions[bot]@users.noreply.github.com`) with the tap's own subject convention, `tandem <version>`, and pushes `main`. A rejected push exits non-zero with git's message.

## Why

- **Immediacy is the requirement, and the tag run is the only place the artifact and its digest already exist.** A release is available to brew in the same run that publishes it, with no window in which the tap lags.
- **The script, not inline YAML, because the bump has two callers.** The runner runs it once per tag; a human runs it for a tag the workflow missed. A shell script is the smallest thing both can call, and it is testable locally against fixture taps (`Tests/TandemCoreTests/TapBumpTests.swift`), which a workflow body is not.
- **A PAT is the smallest credential that writes one file in one repo.** `GITHUB_TOKEN` cannot cross repos; a GitHub App or a deploy key is also a human-created credential plus somewhere to fetch it from.
- **The failure is loud because the failure mode is a stall.** A missing or expired token is exactly the state a hand bump used to hide, so it stops the run and names the secret and where to set it.
- **Two lines and a whole-file assertion keep the rest of the cask safe.** The tap's cask carries the token-collision rule in its `url`; a rewrite that reformatted it would break installs in a way neither `brew style` nor the audit would catch.

## Amends ADR 0003

ADR 0003's consequence that "the tap's `autobump` (daily `brew bump`) livechecks the release-asset URL and opens the bump PR for the single cask file" is retired: the tag run writes the cask directly, and `autobump.yml` is background upkeep at most. The one-cask distribution decision is unchanged.

## Decided against

- **The tap-side route: a cron in the tap reading `releases/latest`.** GitHub's shortest cron is 5 minutes and routinely late, and the tap's `autobump.yml` opens a PR a human merges, so no configuration of it is immediate. It needs a cross-repo token anyway.
- **A separate `tap.yml` triggered by `release: published`.** Events raised by `GITHUB_TOKEN` do not start new workflow runs, and `release.yml` publishes with `GITHUB_TOKEN`, so the trigger would never fire; a `repository_dispatch` would need a second scope (`actions: write`) on top of the tap's `contents: write`.
- **Checking the token before the publish step.** It would let a missing secret block the release itself, a regression: today the release publishes and only the cask waits.
- **The sprint close bumping the cask from the developer's brew tap clone** (over the existing SSH remote, no new credential). It writes a machine path into a skill that stays portable ([ADR 0026](0026-agent-prompts-and-portable-setup.md)), and the bump would depend on the sprint surviving the tag; any other tag push would leave the cask behind.
- **A GitHub App installation token or a deploy key instead of the PAT.** Both are human-created credentials too, and both add a store to fetch a secret from.
- **`brew bump --open-pr` from the tag run.** It still leaves a PR for a human to merge, which is the latency this removes.
- **A warning instead of a failure when the token is missing or rejected.** A silent skip is the stall this replaces.
- **A `workflow_dispatch` path that bumps an old tag.** The script is that path, so the workflow keeps one trigger and one code path.
- **Renaming or transferring the tap's `tandem` token away from Homebrew core's cask.** Out of scope, and it would not shorten the lag; `docs/releasing.md`'s full-name rule covers the collision.

## Consequences

- A tag push leaves the tap's cask on the new version with the release's digest, with `brew style` and `brew audit --cask --strict` green in the run's log, and no human commit in the tap.
- A tag run with `TAP_TOKEN` unset or expired stops at the tap step naming the secret, after the release is published: today's outcome, now loud.
- The credential is one fine-grained PAT with `Contents: Read and write` on `gianniacquisto/homebrew-tap` alone, held by `gianniacquisto/tandem` (TAN-235); its expiry is a red tap step, and the fix is `gh secret set TAP_TOKEN`.
- `docs/releasing.md`, this ADR, ADR 0003 and the sprint skill's release paragraph now state the same act: the tag ships app and cask in one run.
