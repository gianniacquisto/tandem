# 06 - Core: `Bikeshed` type in TandemCore + core test suites

Type: implementation
Status: resolved
Blocked by: none (decision tickets 01–05 resolved; spec ready-for-agent)

## What to build

The `Bikeshed` core type — sibling of `Store` — holding all vault, wikilink, backlink, and search logic, plus the full core test suite. The GUI (ticket 08) builds on this; the MCP target stays untouched.

Scope = the spec's sections: "Bikeshed core — operations", "Vault protocol", "Wikilink model", "Search and backlinks" (data side), "Testing Decisions" (the nine suites). The spec is the only normative input.

Context (read in your worktree):
- Spec: `.scratch/bikeshed/spec.md`
- ADRs: `docs/adr/0004-vault-protocol.md`, `docs/adr/0005-wikilink-rename-rewriting.md`; reuse ADR 0001's sidecar-lock + orphan-temp pattern on a separate lockfile
- Vocabulary: `GLOSSARY.md`
- Conventions to mirror: `Sources/TandemCore/` (Store, Operations, OperationResult, Errors), `Tests/TandemCoreTests/` (temp dirs, real files, threads for contention)
- Exploration notes: `/private/var/folders/4x/kz_77l9s0lddszts3p0k6b940000gn/T/opencode/bikeshed/notes/core-patterns.md`

## Acceptance

- [ ] `Bikeshed`: `Sendable`, directory-rooted value type; default vault `~/.tandem/bikeshed`; `init(directory:)` for tests
- [ ] Operations per the spec's contract: list / read (text + on-disk hash + mtime) / create / write (records path + hash for self-write dedup) / rename (one locked transaction: rewrite referencing notes first — per-file atomic — then `rename(2)` the note file; target name already taken → refused, nothing written) / delete (permanent primitive; the GUI trashes first) / search / backlinks / the single link resolver shared by backlinks and rename-rewrite
- [ ] Vault protocol per ADR 0004: `.md` (case-insensitive) = note; hidden files ignored; non-`.md` = attachments, never in the tree; name validation rejects `/`, `#`, `|` on create/rename (pre-existing violators tolerated); frontmatter opaque — the core never parses note text; one-sided `flock` on the `bikeshed.lock` sidecar with ~2 s bounded retry then a clear "vault locked" error; atomic temp + fsync + rename; orphan temps older than 60 s cleaned after acquiring the lock; missing folder = empty vault, never an error
- [ ] `swift-markdown` added to `Package.swift` for the in-process vault index (the GFM floor); its NOTICE stays with the app
- [ ] The nine test suites from the spec's "Testing Decisions", green, against temp dirs only (no real `~/.tandem`, no network): note operations; wikilink resolution; backlinks; rename + link rewrite; crash recovery (writer killed mid-sequence → no torn files, re-run self-heals); locking and atomicity (threads, external lock-less writer, orphan cleanup); search; conflict detection (hash-based); scale sentinel (hundreds of notes up to ~100 MB total, completes in seconds, non-flaky)
- [ ] `swift build -c release` clean; the existing suites stay green

## Resolution

Built on `bikeshed/ticket-06-core` (base `main`), fast-forward merged into `bikeshed/implementation` on 2026-10-15. Delivered via PR #7 — https://github.com/gianniacquisto/tandem/pull/7.

**What was built**

- `Sources/TandemCore/Bikeshed*.swift` + `Wikilink.swift` — the `Bikeshed` value type (sibling of `Store`): vault operations (`list` / `read` / `create` / `write` / `rename` / `delete` / `search` / `backlinks`), the single shared link resolver, and `links(in:)` for scanning arbitrary text (code blocks + inline code exempt, embeds skipped). One-sided `flock` on the `bikeshed.lock` sidecar with ~2 s bounded retry then a clear "vault locked" error; atomic temp + fsync + `rename(2)`; orphan temps older than 60 s cleaned after acquiring the lock; missing folder = empty vault, never an error.
- `Tests/TandemCoreTests/Bikeshed*Tests.swift` — the nine suites from the spec's "Testing Decisions": note ops; wikilink resolution; backlinks; rename + link rewrite; crash recovery; locking + atomicity; search; conflict detection (hash-based); scale sentinel (94 MB / 280 notes ≈ 11 s, 45 s bound).
- `Package.swift`: `swift-markdown` added for the in-process vault index (the GFM floor); its NOTICE stays with the app.

**Gates** — `swift build -c release` clean; `swift test` 32 → 130/130 green (+98 across the nine suites). All suites run against temp dirs only (vaults nested one level so the sibling `bikeshed.lock` never collides between concurrent suites).

**Deliberate deviations** — 12, recorded in the implementer's report (`notes/ticket-06-report.md` in the bikeshed orchestration dir): SHA-256 hashing; `.md` suffix required on write paths; `write` auto-creates parents; empty fragment/alias → `nil` (`[[A#]]` ≡ `[[A]]`); etc.

**Post-merge** — the two-axis code review (base `main`) found no core changes: the core is the single-resolver reference and stayed untouched; the JS bundle (ticket 07) was reconciled to it instead (grammar parity fix `03afee4`, see ticket 07). Review findings were in the GUI layer (ticket 08) and the bundle's packaging (ticket 07) — see those Resolutions.
