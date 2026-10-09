# Store protocol: sidecar lock, atomic rename, versioned JSON

The store is a single JSON file (`~/.tandem/todos.json`) read and written by several processes at once — the GUI app plus one short-lived MCP server process per agent session. We decided: writers take an exclusive `flock` on a **sidecar lockfile** (`~/.tandem/tandem.lock`) held across the entire read → modify → write-temp → `rename` cycle; readers take no lock at all; writes are atomic via a same-directory temp file + `rename`; the file is a versioned envelope (`{"version": 1, "todos": [...]}` with UUIDv4 ids, ISO-8601 UTC timestamps, creation order); done todos accumulate until explicitly cleared; a corrupt or schema-invalid file is backed up as `todos.json.corrupt-<ts>` (recovery done under the lock) and the store restarts empty, with the GUI notifying the human.

## Why a sidecar lockfile, not the data file

`flock` locks an inode, and every atomic write replaces the data file's inode via `rename` — locking the data file itself would let two writers each hold "the lock" on different inodes. The sidecar file is never renamed, so its lock is stable. `flock` also releases automatically on process death, so no stale-lock recovery exists to build.

## Consequences

- Readers always see a consistent snapshot (complete old or complete new file) without locking — the MCP `list` path and GUI refresh are lock-free.
- Contention is handled by bounded retry (~2 s, 10–50 ms backoff) then a clear error; writes last milliseconds, so real contention essentially never surfaces.
- The `version` envelope is the migration seam for future schema changes.
- Unknown fields are ignored on read (forward-compat); missing or mistyped required fields invalidate the whole file (backup + restart) — never silently discarded.
