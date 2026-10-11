# Releasing

Two repos: the main repo holds the source; [`gianniacquisto/homebrew-tap`](https://github.com/gianniacquisto/homebrew-tap) holds the [Homebrew formula](https://github.com/gianniacquisto/homebrew-tap/blob/main/Formula/tandem.rb) that builds and installs it. brew only reads taps from its fixed tap directory, so on any dev machine the working copy is wherever brew keeps the clone — `brew tap gianniacquisto/tap` creates it at `$(brew --prefix)/Library/Taps/gianniacquisto/homebrew-tap`. That directory is an ordinary git clone of the tap repo: branch, commit, and push from there (`brew update` keeps it current).

To cut a release:

1. Tag the main repo: `git tag v0.4.0 && git push origin v0.4.0`.
2. Bump the formula **and the cask** — `version`, the tag in `url`, and `sha256` (`curl -L <url> | shasum -a 256`; the cask mirrors the formula's). The tap's `autobump` workflow watches for new tags and opens the bump PR for you; merging it is the whole release. The workflow needs the tap's `HOMEBREW_GITHUB_API_TOKEN` secret (a token with `repo` scope) — without a working token the run fails after pushing the bump branch, so the PR has to be opened by hand.
3. If bumping by hand, verify before pushing: `brew style Formula/tandem.rb Casks/tandem.rb && brew audit --strict --formula gianniacquisto/tap/tandem && brew audit --cask --strict gianniacquisto/tap/tandem && brew test gianniacquisto/tap/tandem && brew install --build-from-source gianniacquisto/tap/tandem`.
4. Users get it with `brew update && brew upgrade tandem`.
