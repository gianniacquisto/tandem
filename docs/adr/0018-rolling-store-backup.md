# A rolling store backup: one previous good copy, taken under the write's own lock

> **Amended 2026-10-26 by [ADR 0031](0031-one-database-the-store.md):** the backup is `tandem.db.bak`, one whole-database generation taken through SQLite's backup API inside the write's transaction. This record's `todos.json.bak` name, its sidecar `flock` and its `mv` recipe belong to the JSON store it was written about; the one-generation rule, the take-it-before-the-write seam, "a copy we cannot take fails the write" and "no restore verb, `mv` is the restore" stand. The amendment's text is ADR 0031's.

The store already kept two copies aside, and both answer a *shape* failure: a file that will not decode is quarantined as `todos.json.corrupt-<ts>` ([ADR 0001](0001-store-protocol.md)), and a file still carrying the legacy `done` mirror is copied aside as `todos.json.pre-migrator-<ts>` ([ADR 0017](0017-the-ticket-model.md)). Neither answers an **ordinary bad write** — a `kill` mid-rename, an agent with a bad regex, a half-finished hand edit — because nothing holds the state that write replaced. On 2026-10-17 that cost a real list, and the recovery was manual.

We decided: **before each successful write, the file about to be replaced is copied to `todos.json.bak`**, one generation, by the same temp + fsync + `rename` every other store write uses, under the same sidecar `flock`. Reads write nothing; the backup moves only when the store does.

## Why the copy is taken between the fsync and the `rename`

"Before the write" is ambiguous, and the two readings differ in exactly the case the backup exists for:

- **Copy first, then write.** A failure in the write leaves a `.bak` that was already overwritten — so a failed write has silently consumed the previous generation, which is the ticket's own "a failed write leaves the previous `.bak` untouched".
- **Copy after the new bytes are durable but before the commit.** Nothing can go wrong quietly: if writing or fsyncing the new file fails, the backup was never touched; if the copy fails, the write is abandoned with the old file still in place; if only the final `rename(2)` fails, the store file is unchanged and the newly written `.bak` holds those very same bytes — still the previous good copy, by identity.

So the seam is inside the shared primitive: `LockedDirectory.atomicWrite(_:to:beforeCommit:)`, where `beforeCommit` runs after the temp's fsync and before the `rename(2)`. One new parameter with a `{}` default, so the bikeshed's write path is untouched, and the store's rolling copy is itself an `atomicWrite` — therefore never observed half-written, and never a generation behind.

## Why one generation

Recorded in the ticket as decided against: rotation is a backup *system*, not a store invariant. One previous copy answers every failure we have — a bad write, a bad regex, a kill — because each write only ever has to undo itself. Timestamps, a directory of generations, retention, and a restore verb are the shape of a feature nobody has yet needed; a `.bak` is the shape of a file being recoverable with `mv todos.json.bak todos.json`.

## Why a backup we cannot take fails the write

If `todos.json` cannot be read, or `todos.json.bak` cannot be written, the write **fails loudly** rather than proceeding without a copy — the same rule as `backUpBeforeMigrating` ("never silently discard"). The store is hand-editable and multi-writer; a store quietly writing without its only previous copy is worse than a write the caller can see failed and retry.

## Why `doctor` reports it inside the `store` check

[ADR 0014](0014-cli-doctor.md) pins the six check names and their order as a documented contract (`skills/tandem/SKILL.md`), so a seventh check would be a breaking change to a published shape — for a line that can never be a failure. The backup is a property *of the store*, and the store row is where a human reads the recovery story, most of all when that row already says "present but unreadable or corrupt" — the one case the `.bak` exists for. The row now ends `; backup 5m old` or `; no backup yet`, and the new read-only core surface is `Store.backupHealth()` (`absent`, or the age of the copy), mirroring `Store.health()`: lock-free, non-mutating, and gathered into `Facts` so `SetupDiagnostics.report(_:)` stays a pure function of injected facts.

## Rejected

- **A hard link (`link(2)`) instead of a copy.** It would be O(1) and atomic, but the `.bak` would share the data file's inode — and the file is *hand-editable by design*, so an editor or script that truncates and rewrites in place would silently rewrite the backup too. A copy is the only form that survives the case the file's own legibility invites.
- **Writing the backup after the store write.** A crash between the two leaves a `.bak` equal to the *new* state: no previous generation at all, and the failure mode is silent.
- **Rotating generations, or a timestamped series.** Answered above: a backup system, not an invariant.
- **A new `backup` check in `doctor`.** Breaks the pinned check list for an informational line.
- **Putting the backup in the store envelope, or in `~/.tandem/backups/`.** The file's neighbours are part of its contract — the same directory as the lockfile, the quarantine, and the pre-migrator copy, so a human looking at the store sees everything that belongs to it. No schema change, no envelope version change ([ADR 0006](0006-stored-order-and-sections.md)): nothing about the file's shape moves.
- **A restore verb.** `mv` is the restore; the backup is a file, not an API.

## Consequences

- The store's directory normally holds `todos.json`, `todos.json.bak`, and `tandem.lock`. Every write now fires two more directory events (the backup's temp and its rename), which the GUI's `StoreWatcher` sees — a reload it already does for the store's own two, over identical contents.
- Tests (`StoreBackupTests`): no `.bak` on the first write, the pre-write state on the second, one generation after the third, no backup for a write that changed nothing, and two failure shapes — a mutation that throws, and an IO failure in the commit — both leaving the store, the previous `.bak`, and the temp directory untouched.
- The quarantine and pre-migrator paths are unchanged: a corrupt file is still quarantined rather than backed up (there is nothing valid to roll), so a recovery leaves the previous `.bak` as it was.
- Recovery stays manual and one command deep: `mv ~/.tandem/todos.json.bak ~/.tandem/todos.json`, with the app's watcher reloading it.
