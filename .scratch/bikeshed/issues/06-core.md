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

(to be filled at merge)
