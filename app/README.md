# The app

Tandem's window: an Electron shell around the Rust core's `rpc` stream
(`docs/port.md`). The renderer holds no domain logic; every fact arrives over
the stream, and `app/CONVENTIONS.md` is the house style for everything drawn.

## Run it

```sh
npm ci
npm run binary   # builds rust/target/debug/tandem
npm run dev
```

Development spawns `rust/target/debug/tandem rpc` against the fixture home,
`Fixtures/port/Home`, unless `TANDEM_HOME` names another home. The Swift app
still owns `~/.tandem` until the cutover, so the shell never opens it by
accident.

`TANDEM_BIN` overrides the binary path. `npm run dev` reads the workspace
build; a packaged app reads the copy beside its resources
(`process.resourcesPath/tandem`, `tandem.exe` on Windows), and the artifact
carries it: `npm run pack` and `npm run dist` build the core in release
configuration and electron-builder copies it in.

## Checks

```sh
npm run typecheck        # the Node and the browser programs
npm test                 # the seams: tokens, bridge, spawn, window, shell
npm run build            # out/main, out/preload, out/renderer
```

## Package

```sh
npm run dist
```

`electron-builder.yml` builds a dmg on macOS, an NSIS installer on Windows and
an AppImage on Linux. `.github/workflows/app.yml` runs the same on the three
hosted runners and uploads one artifact per platform.

The artifact carries the core it runs: `scripts/prepare-binary.mjs` builds
`tandem` in release configuration and electron-builder copies it beside the
app's resources, so an installed app spawns nothing from a checkout. The macOS
arm64 dmg is about 118 MB, of which the core is about 6 MB.

Two gates check that. `npm run check:artifact` asserts the binary sits where
`src/main/binary.ts` resolves it; `npm run smoke` launches the packaged app
against a copy of `Fixtures/port/Home` and requires `ticket.list` to answer a
revision. CI runs the second on macOS and Linux, under Xvfb on Linux.

**Code signing is not set up.** The artifacts are unsigned on all three
platforms: macOS ships with `identity: null` (no notarization), and Windows and
Linux are unsigned because no certificate is configured. Signing needs an
Apple Developer account and a Windows certificate, so it is left for the human
and filed as its own backlog ticket.

## A picture

`TANDEM_SHOT=<path.png>` on a dev run writes the window's own PNG after the
first render and quits. It refuses unless `TANDEM_HOME` is a home under
`Fixtures/`, the rule in `docs/screenshots/README.md`.
