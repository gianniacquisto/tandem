# The port: one binary, one app, three platforms

**Status**: decided 2026-10-30. The epic is `PRT-1` on the board; the decision record is [ADR 0037](adr/0037-one-stack-rust-core-electron-ui.md). This file is the spec: what the target looks like, what is deliberately not built, and what "done" means. The epic mirrors it and carries the work.

Tandem today is a macOS-only product by construction: `Package.swift` targets `.macOS(.v27)`, the GUI is 8,934 lines of SwiftUI and AppKit, and nothing in the domain needs the Mac. The core is 11,377 lines with no AppKit or SwiftUI import, the CLI is 1,995 lines of Foundation over it, the store is SQLite behind a version seam, and the agent face speaks JSON. Only the presentation layer is Mac-bound, so only the presentation layer gets rewritten.

## Target shape

```
tandem (one binary: Rust)
  - agent verbs           session, ticket, note, lane, sprint, setup, doctor
  - rpc over stdio        the app's only door to the core; same message shapes as HTTPS later
        ^
        | spawn + pipes
        v
Tandem.app (Electron shell) -> React renderer -> CodeMirror 6 bundle (moved verbatim)
```

The app holds no domain logic. The binary the agents call and the core the app trusts are the same executable, so they cannot drift.

## Stack

| piece | choice | why |
|---|---|---|
| store engine | `rusqlite`, `bundled` | one SQLite on every platform, no distro library |
| markdown | `comrak` | GFM, closest to the `cmark-gfm` the Swift core uses, so the health report and editor agree |
| grammar | `clap` | the CLI's verb surface, unchanged |
| wire | `serde`, `serde_json` | the JSON the skills already parse |
| hashing | `sha2` | note content hashes |
| shell | Electron, electron-vite, electron-builder | one Chromium on all three platforms |
| renderer | React + TypeScript | agent throughput and ecosystem depth; `app/CONVENTIONS.md` pins the idioms |
| message types | `ts-rs` | generated from the Rust structs, so a shape cannot drift without a compile error on both sides |
| UI screenshots | Playwright against the real app | retires `Scripts/shot.sh`, answers `TAN-210`, unblocks `TAN-217` |

No ORM and no async runtime: the store is one writer on one file.

## Store

- **Fresh schema, version 1.** The Rust core refuses a v5 file with a message naming the import. No v5 reader, no dual writer, no coexistence test.
- **An actor on every write, from the first migration.** Every mutation records who made it, and every row keeps its author. Authorship stops being inferred from session records.
- **One home everywhere**: `~/.tandem` on macOS, Windows and Linux, with `TANDEM_HOME` the only override. Agents are the primary consumer of these paths.
- **Migrations** continue from v1 through the same `schema_migrations` seam, in the store adapter only. No SQL outside the adapter, and no table shape leaking into the wire: the messages are domain verbs, not rows.

## Transport and the UI contract

- stdio locally, HTTPS remotely later; one message set, two stream adapters. No daemon: the app spawns the child, the child dies with the app, and the CLI opens `tandem.db` directly, with the engine's `BEGIN IMMEDIATE` and ~2 s busy window as the only contention bound (ADR 0031).
- **No UI-only writes.** Every UI mutation maps to a message the CLI also exposes, with a test that fails when a UI-only write appears.
- Listings are a full snapshot plus a revision number; the core pushes change events on the stream.

## What is kept, what is pruned

Pruned before a line is ported (about 2,000 of the app's 8,934 Swift lines):

| surface | reason |
|---|---|
| `MenuBar.swift`, `PopoverFace.swift` | window-first; no cross-platform popover |
| Sessions tab (`SessionsFace.swift`) | read-only over `session list` / `session show`; data stays, Usage is the surface |
| Sprints tab (`SprintsFace.swift`) | read-only over `sprint list`; data stays |
| Board as its own tab (`BoardView.swift`) | becomes a list/board toggle in the Tickets face |
| Setup wizard as a window (`SetupWizard.swift`) | folded into one Settings surface with a repair action (`TAN-216`) |
| Inbox tab | the human queue moves to the harness; the `human` assignee and `ticket answer` stay |
| Quick-open palette (`QuickOpenPalette.swift`) | never used |

Kept and ported: the Tickets face (list plus board layout, scopes, search, quick-add, lanes), the Bikeshed face (tree, editor, backlinks, health), Usage (`TAN-208`, then `TAN-234`), Settings with the setup section, the ticket sheet.

Not built: a hosted service for several humans (a deployment shape later, not a feature), a tray, and any CLI prune.

## Parity, the definition of done

1. The CLI goldens captured from the Swift binary pass against the Rust binary, message for message.
2. The ported domain tests pass. Ported, not rewritten: they encode decisions the code does not restate (lock retry, atomic rename, orphan temps, ordered writes).
3. Every kept surface is reachable, and every write goes through a message the CLI also exposes.
4. The one-off import of the real home produces a board the human accepts: 78 tickets by ref, status, assignee, title and `createdAt`.
5. One week of dogfooding on the Rust app with no data loss.
6. The cutover commit: delete the Swift targets, flip the release workflow, tag v1.0.0.

## How the work lands

- **Additive on `main`**: new directories `rust/` and `app/`. `Package.swift` and `Sources/` keep building and the release keeps shipping the Swift app until cutover. No long-lived port branch: the repo retired `dev` on 2026-10-29 for this shape ([ADR 0019](adr/0019-integration-branch.md)), and every agent contract branches from `main`.
- **Fixture homes until cutover**: the Rust binary runs against `Fixtures/` homes and never touches `~/.tandem` while the Swift app owns it.
- **Merges, not rebases**, per the repo's git rule.

## Migration

One script, run once, kept in `Scripts/` and deleted after use. Tickets, refs, timestamps, comments, dependency edges, sessions, sprints, usage and settings seed the new store by SQL from the old `tandem.db`; the bikeshed is copied as files. Not through the CLIs: `ticket set` writes none of `number`/`ref`, `createdAt`, `completedAt`, `origin`, `lane`, `archived` or edges, and a `session end` replay stamps "now", which falsifies the dates the Usage tab rolls up.

The old home is copied to `legacy.db` and kept. The Swift app stays installed and runnable until the import has been accepted, so the comparison is a diff and not a memory test.

## Phases

1. **Contract harness.** Capture CLI JSON goldens from the Swift binary; stand up the ported domain tests as a Rust suite; one fixture home both can read.
2. **Rust core and CLI.** Agent verbs first: daily value, and what proves the goldens.
3. **Shell and two faces.** Electron, then Tickets and Bikeshed.
4. **Remaining surfaces.** Usage over `TAN-208`'s report, Settings with setup, the ticket sheet, lanes.
5. **Import, dogfooding, cutover.**

## Where the old code goes

| today | becomes |
|---|---|
| `Sources/TandemCore/` (11,377) | `rust/core` (domain, store port, SQLite adapter) |
| `Sources/tandem-cli/` (1,995) | `rust/cli` in the same binary: agent verbs plus `rpc` |
| `Sources/Tandem/` (8,934) | `app/` (Electron main, preload) and `app/renderer` (React), minus the prunes |
| `Sources/Tandem/Resources/Editor/` | `app/renderer` resource, unchanged |
| `Scripts/shot.sh` | a Playwright spec in CI |
| `Scripts/usage-report.sh` | `tandem session end --usage-from-harness` |
| `Tests/TandemCoreTests/` | carried over as the goldens and the ported suite |
| `skills/`, `agents/`, `AGENTS.md` | unchanged: they speak the CLI's JSON |

## Open

- Whether `TANDEM_AGENT` is the actor value on agent writes, or the actor is a separate field the CLI fills from it.
- Whether the Usage tab absorbs the per-session and per-sprint detail sheets the cut faces carried, or lists them as rows with the same detail on expand.
