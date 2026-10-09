# 02 - Store protocol: locking, schema v1, retention, editability

Type: grilling
Status: open

## Question

Pin down the store's exact design so the spec (and the implementation) has one protocol to follow. The high level is settled: a JSON file at `~/.tandem/todos.json`, flock + atomic rename, GUI watches the file for live updates, read by the app and by agent-spawned MCP server processes simultaneously.

Nail down:

1. **Locking**: flock on the data file itself, or on a sidecar lockfile? Exactly what is held during a read-modify-write? What happens when the lock is held (retry with backoff? fail fast with a clear error?)? Stale-lock recovery after a crash (flock releases on process death — confirm nothing more is needed, or define it).
2. **Atomicity**: temp-file + `rename()` dance — temp file location (same directory, required for atomic rename), naming convention, cleanup of orphaned temp files (on app startup? on every write?).
3. **Schema v1**: the exact JSON shape — a top-level envelope with a version field? todo fields (id type/format, title, done, created/completed timestamp format), array ordering rules.
4. **Retention**: do done todos accumulate forever? What does "clear done" in the GUI do to the store?
5. **Human editability**: must the app tolerate a human (or an agent) hand-editing the file? If yes: validation on load, recovery behaviour for a corrupt file (back it up? fail? reset?). If no: say so explicitly in the spec.
