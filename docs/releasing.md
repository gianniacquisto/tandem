# Releasing

Two repos: the main repo holds the source; [`gianniacquisto/homebrew-tap`](https://github.com/gianniacquisto/homebrew-tap) holds the [Homebrew cask](https://github.com/gianniacquisto/homebrew-tap/blob/main/Casks/tandem.rb) that installs the prebuilt app. brew only reads taps from its fixed tap directory, so on any dev machine the working copy is wherever brew keeps the clone — `brew tap gianniacquisto/tap` creates it at `$(brew --prefix)/Library/Taps/gianniacquisto/homebrew-tap`. That directory is an ordinary git clone of the tap repo: branch, commit, and push from there (`brew update` keeps it current).

To cut a release:

1. Tag the main repo: `git tag v0.5.0 && git push origin v0.5.0`.
2. The [Release workflow](https://github.com/gianniacquisto/tandem/blob/main/.github/workflows/release.yml) runs the test suite, builds and verifies the artifact, and publishes the GitHub release for the tag with `Tandem-<version>.zip` and `SHA256SUMS` attached. It also runs on pull requests — build-and-verify only, never publishing — so a release can be dry-run on a PR. Re-pushing the same tag re-cuts the release.
3. The tap's `brew bump` workflow livechecks the release assets and opens a bump PR for the single cask file with the artifact's real sha256 — merging it is the whole release. It needs the tap's `HOMEBREW_GITHUB_API_TOKEN` secret (a token with `repo` scope) — without a working token the run fails after pushing the bump branch, so the PR has to be opened by hand.
4. If bumping by hand, verify before pushing: `brew style Casks/tandem.rb && brew audit --cask --strict gianniacquisto/tap/tandem`.
5. Users get it with `brew update && brew upgrade --cask tandem`.
