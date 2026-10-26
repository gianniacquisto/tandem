# Store protocol: sidecar lock, atomic rename, versioned JSON

> **Amended 2026-10-26 by [ADR 0031](0031-one-database-the-store.md):** the store is one SQLite database, `tandem.db`. For the store, the sidecar `flock`, the atomic-rename dance, the versioned JSON envelope and the quarantine-and-restart are retired. What stands from this record: a reader never writes, a reader sees a consistent state, contention is bounded and loud, and the dev-build home rule. The amendment's text is ADR 0031's.

The store is a single JSON file (`~/.tandem/todos.json`) read and written by several processes at once — the GUI app plus one short-lived MCP server process per agent session. We decided: writers take an exclusive `flock` on a **sidecar lockfile** (`~/.tandem/tandem.lock`) held across the entire read → modify → write-temp → `rename` cycle; readers take no lock at all; writes are atomic via a same-directory temp file + `rename`; the file is a versioned envelope (`{"version": 1, "todos": [...]}` with UUIDv4 ids, ISO-8601 UTC timestamps, creation order); done todos accumulate until explicitly cleared; a corrupt or schema-invalid file is backed up as `todos.json.corrupt-<ts>` (recovery done under the lock) and the store restarts empty, with the GUI notifying the human.

## Dev builds get their own home

`~/.tandem` belongs to the *shipped app* — the cask's `Tandem.app` and the `tandem-mcp` inside it. Anything built and run in place (`swift run Tandem`, a bare `.build/…` binary, any debug build) resolves to `~/.tandem-dev` instead, so a from-source run can reorder, rewrite, or wipe its own throwaway list without the real one being reachable at all. The gate is "release build **and** inside a `.app` bundle" — not a flag a half-configured run can get wrong — and `TANDEM_HOME` overrides both for a deliberate one-off.

One rule, one owner: `TandemHome` (TandemCore) is the only place that answers "where is home?", and the store, the vault, the lockfiles, and the pid file all ask it.

This is not hypothetical. On 2026-10-17 a from-source GUI was reading and writing the real store while the schema was being changed under it — one bad build away from wiping the human's list, with the only protection being that the mistake hadn't happened yet.

## Why a sidecar lockfile, not the data file

`flock` locks an inode, and every atomic write replaces the data file's inode via `rename` — locking the data file itself would let two writers each hold "the lock" on different inodes. The sidecar file is never renamed, so its lock is stable. `flock` also releases automatically on process death, so no stale-lock recovery exists to build.

## Consequences

- Readers always see a consistent snapshot (complete old or complete new file) without locking — the MCP `list` path and GUI refresh are lock-free.
- Contention is handled by bounded retry (~2 s, 10–50 ms backoff) then a clear error; writes last milliseconds, so real contention essentially never surfaces.
- The `version` envelope is the migration seam for future schema changes.
- Unknown fields are ignored on read (forward-compat); missing or mistyped required fields invalidate the whole file (backup + restart) — never silently discarded.
- A dev build and the installed app can run at the same time (distinct pid files) and never contend on one lockfile, because they are not looking at the same home.
