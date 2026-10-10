# App distribution: formula builds the bundle, cask links it into /Applications

Launchpad only indexes `.app` bundles in `/Applications`, and the GUI was a bare SwiftPM executable on `PATH` — so a human had to type `tandem` in a terminal to start it. We decided: the Homebrew **formula** stays the single builder — after `swift build` it assembles `Tandem.app` (`Scripts/make-app-bundle.sh`: the binary as `Contents/MacOS/Tandem`, a versioned `Info.plist` from `Resources/Tandem-Info.plist`, `Resources/AppIcon.icns`, ad-hoc signed) and installs it under `share/` in the Cellar — and a **cask** of the same name depends on the formula and does exactly one thing: a symlink `/Applications/Tandem.app → $(brew --prefix)/opt/tandem/share/Tandem.app`. One build, one upgrade path (`brew upgrade tandem`); because the link points at the versionless `opt` path, the app in `/Applications` is never stale. The bundle identity is `com.gianniacquisto.tandem`.

## Why a cask link, not the alternatives

- **The formula installs the app into `/Applications`** — violates Homebrew's layout policy (formulae own the Cellar); `brew audit --strict` fails.
- **The cask builds from source** — duplicates the formula's build logic and creates a second upgrade path for the same code; the tap's tag/autobump flow would have to serve two independent artifacts.
- **A per-user wrapper command** (`tandem install-app` copying a bundle into `~/Applications`) — an extra per-machine command for every user, and the copy goes stale on `brew upgrade tandem` (a symlinked executable inside the bundle is more exotic than a cask link).
- A cask is Homebrew's sanctioned channel for "an app in `/Applications`"; making it a thin link over the formula's output keeps the policy happy without moving the build.

## Consequences

- The cask's `url`/`sha256` mirror the formula's (the same tag tarball) even though the cask never downloads from it — the cask exists for the `/Applications` artifact, not the download. The tap's `autobump` workflow therefore bumps both files on every release.
- A formula-only install has no Launchpad presence. The README leads with the cask command; the two installs are complementary, not competing (the cask pulls in the formula via `depends_on`).
- The bundle and the bare `tandem` binary remain distinct identities to LaunchServices; the app's pidfile guard (`SingleInstance`, `~/.tandem/app.pid`) makes them one visible instance regardless of launch path.
- `brew uninstall --cask tandem` removes only the link; the bundle itself lives and dies with the formula.
