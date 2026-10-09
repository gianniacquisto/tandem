# 02 - Store protocol: locking, schema v1, retention, editability

Type: grilling
Status: resolved

## Question

Pin down the store's exact design so the spec (and the implementation) has one protocol to follow. The high level is settled: a JSON file at `~/.tandem/todos.json`, flock + atomic rename, GUI watches the file for live updates, read by the app and by agent-spawned MCP server processes simultaneously.

Nail down:

1. **Locking**: flock on the data file itself, or on a sidecar lockfile? Exactly what is held during a read-modify-write? What happens when the lock is held (retry with backoff? fail fast with a clear error?)? Stale-lock recovery after a crash (flock releases on process death — confirm nothing more is needed, or define it).
2. **Atomicity**: temp-file + `rename()` dance — temp file location (same directory, required for atomic rename), naming convention, cleanup of orphaned temp files (on app startup? on every write?).
3. **Schema v1**: the exact JSON shape — a top-level envelope with a version field? todo fields (id type/format, title, done, created/completed timestamp format), array ordering rules.
4. **Retention**: do done todos accumulate forever? What does "clear done" in the GUI do to the store?
5. **Human editability**: must the app tolerate a human (or an agent) hand-editing the file? If yes: validation on load, recovery behaviour for a corrupt file (back it up? fail? reset?). If no: say so explicitly in the spec.

## Answer

Resolved by grilling (Q14–Q19, all accepted). Normative protocol:

**Locking** — Writers take an exclusive `flock` on the sidecar lockfile `~/.tandem/tandem.lock` (created if missing, never renamed), held across the entire read → modify → write-temp → `rename` cycle. Readers take no lock: atomic rename guarantees any reader sees a complete old or new file. `flock` releases on process death — no stale-lock recovery to build.

**Contention** — Retry with 10–50 ms backoff for up to ~2 s, then fail with a clear "store locked by another process — retry" error.

**Atomic writes** — Temp file in `~/.tandem/` (same directory, required for atomic rename), named `.todos-<random>.tmp`, fsync before `rename`. After acquiring the lock, each writer deletes orphaned `.todos-*.tmp` files older than 60 s (crash leftovers; the age threshold never touches in-flight writes).

**Schema v1** — Versioned envelope; todos in creation order (display order is a UI concern, not the store's); `~/.tandem/` is created on first write:

```json
{
  "version": 1,
  "todos": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "title": "Water the plants",
      "done": false,
      "createdAt": "2026-10-09T14:03:22Z",
      "completedAt": null
    },
    {
      "id": "6ba7b810-9dad-41d4-82b5-54d938c42a22",
      "title": "File taxes",
      "done": true,
      "createdAt": "2026-10-01T09:15:00Z",
      "completedAt": "2026-10-09T11:00:00Z"
    }
  ]
}
```

**Retention** — Done todos accumulate until explicitly removed: GUI "Clear done" removes all done in one write; per-todo delete is GUI-only; no automatic pruning; agents have no delete in v1.

**Editability & corruption** — Lenient read, strict write: unknown extra fields are ignored (forward-compat); missing or mistyped required fields invalidate the file. On an invalid file, under the lock (so the app and an MCP server can't double-recover the same file): rename it to `todos.json.corrupt-<unix-timestamp>`, start from an empty store, and the GUI notifies the human with the backup path. Nothing is ever silently discarded.

Assets: [ADR 0001 — Store protocol: sidecar lock, atomic rename, versioned JSON](../../../docs/adr/0001-store-protocol.md)
