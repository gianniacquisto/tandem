# Releasing

One repo, one workflow: a tag's Release run builds and verifies three standalone `tandem` archives, then publishes them with `install.sh` and one `SHA256SUMS`. There is no Homebrew tap, no Electron package and no Node toolchain in the path (CUT-16, [ADR 0046](adr/0046-tandem-serve-and-single-binary-distribution.md)).

A release cuts only on the human's explicit words naming the version: a batch close cuts no release by default and never prompts for one. The manual acts below live in [`examples/skills/tandem-sprint/SKILL.md`](../examples/skills/tandem-sprint/SKILL.md) §5 as the same steps, run by hand when the human asks.

To cut a release:

1. Tag the trunk: `main`'s head is the release.
   - Tag `main`'s head and push the tag, a lightweight tag, the repo's convention: `git tag v0.33.0 origin/main && git push origin v0.33.0`. There is no release line to move, because `main` takes the merges, so the tag names the commit that is actually on `main` ([ADR 0019](adr/0019-integration-branch.md)).
   - Watch the [Release workflow](https://github.com/gianniacquisto/tandem/blob/main/.github/workflows/release.yml) run for the tag until the GitHub release publishes.
2. The Release workflow is four jobs, all cargo. `build-macos` (macOS, Apple silicon), `build-linux` (Ubuntu, x86_64) and `build-windows` (Windows, x86_64) each run `cargo build --release -p tandem` with `TANDEM_VERSION` set to the version, package the one executable, and smoke-test it: `tandem --version` names the stamp, `tandem serve --port 0` answers `GET /` and `GET /notes` with `200 OK` and the server's `Content-Security-Policy` header, and the server shuts down cleanly. The archives are `tandem-<version>-darwin-arm64.tar.gz`, `tandem-<version>-linux-x86_64.tar.gz` and `tandem-<version>-windows-x86_64.zip`, each carrying the executable at its root.
   - `release` waits for all three. It downloads the archives, runs `Scripts/stage-release-assets.sh <version> dist` to stamp `install.sh` with the tag's version and write one `SHA256SUMS` covering the three archives and the script, verifies the manifest with `sha256sum -c` (or `shasum -a 256 -c`), and on a tag run publishes the GitHub release with those five assets. A re-push of the same tag re-cuts the release.
   - The job is named `release` because `main` requires a status check by that name; renaming it would leave the branch protection waiting for a check that never reports, so the name stays and the protection is untouched. Pull-request runs build and verify the same three archives and never publish, so a packaging change is a red check on the PR instead of a broken tag.
3. Users install with the one-liner in [`README.md`](../README.md) (`curl -fsSL .../releases/latest/download/install.sh | sh`), or with the archive for their platform. [`install-paths.md`](install-paths.md) describes what each route asks of the user and how it verifies the download.

The installer line is a contract in two places: `README.md` and the release-notes lead in `Scripts/release-notes.sh`. Change one and the other moves in the same commit.
