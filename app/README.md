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

The screenshots are `app/tests/shots/`, the port's replacement for
`Scripts/shot.sh`: one spec per scene, one PNG per scene, the catalogue under
`docs/screenshots/` rewritten by the same run.

```sh
npm run binary
TANDEM_SHOTS_HOME=../Fixtures/port/Home npm run shots
```

`TANDEM_SHOTS_HOME` is required and must name a home under `Fixtures/`; a run
against `~/.tandem` is refused before a window opens, and the app opens a
scratch copy (`/tmp/tandem-shots/Home`) so the committed fixture is never
written to. The seed is that copy plus a fixed clock (`shots/clock.ts`) and a
fixed fixture user home, so a run writes the same picture it wrote last time. A
scene is one row in `shots/scenes.ts` and one spec in `tests/shots/`.

`TANDEM_SHOT=<path.png>` is the shell's older in-process rig (one PNG after the
first render, then quit); it goes with `Scripts/shot.sh` at the cutover.
