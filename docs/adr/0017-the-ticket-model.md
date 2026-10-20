# The ticket model: status carries done, archive sets a ticket aside, and the legacy `done` outlives both

[ADR 0010](0010-todo-note-links.md) called a todo "a title and a done state". It is now a **ticket**, and the shape we settled is three independent axes. **Status** is one axis of five values — `backlog` (the default), `refined`, `ready`, `in-progress`, `done` — so *done is where the workflow ends, not a flag set beside it*; transitions are **free-form** (the store validates the value, never the transition). **Archived** is a second axis: a ticket set aside, out of the default views, still in the store; any ticket can be archived, and archiving done work is how a list is cleared without deleting. **Cycle lane** — the concept the store still calls `section` — is the third: the area of concern, which the list groups by and the board draws as its rows. Two fields ride along: `description` (markdown) and `assignee` (a human, or an agent id — a plain string until a persona registry exists). All of it is additive, so the envelope **version stays `1`** ([ADR 0006](0006-stored-order-and-sections.md)).

The surprising part is the legacy `done` boolean: it neither disappears nor wins. **It stays on disk as a mirror of `status == done`.** On the one disagreement an older writer can produce, `done` wins for done-ness and the next write reconciles.

## Why

- **Done as a status value, not a separate flag** — the board's columns *are* the workflow, so its last column is Done. Two fields meaning "finished" would force every view to choose between them, and the human's workflow reads as one line: backlog → refined → ready → in-progress → done.
- **Free-form transitions** — nothing can know that "refined" happened, and a validated pipeline would make the CLI and a drag-and-drop board disagree about what is legal. Precedent already on this side: `complete` is an idempotent assertion, `move` clamps rather than rejects.
- **`Archived`, not open/closed** — `Open` already means "not yet completed" ([GLOSSARY](../../GLOSSARY.md)). One word with two meanings on one entity is the tax [ADR 0015](0015-bikeshed-not-vault.md) exists to prevent. "Archive" is also the standing ask: *"Archive done todos so the done list stays out of the way without deleting."*
- **Three axes, not one** — status is where the work is, archived is whether it is still in the way, the cycle lane is what area it belongs to. Collapsing any two (a lane per status, say) would break the stored-order invariant that keeps the array grouped and the file legible by hand (ADR 0006).
- **The `done` mirror is the only safe option** — stopping the write has no safe form: omitting the key makes an older reader see a missing required field, and bumping the version makes it see a version it cannot act on. Both are the 2026-10-17 incident: the file is judged corrupt, moved to `todos.json.corrupt-<ts>`, and the list restarts empty. So the mirror outlives the model that made it, until nothing older can read the store.
- **Deferred, each a recorded no**: due dates, priority, labels, checklists, comments. Note too that *origin* (who added a todo) is not *assignee* (who is on it) and is not part of this.

## Consequences

- Every write still emits `done`. The **migrator** is what retires it, and its precondition is not code but reachability: no binary older than this change may touch the store. That is satisfiable because the CLI ships inside the bundle and a from-source build writes `~/.tandem-dev` ([ADR 0001](0001-store-protocol.md)) — so the trigger is the install of the build that carries this, not the merge.
- A stale writer's completion or reopen is honoured: `done` wins on read, and the next write reconciles `status` (`done ? "done" : (status ?? "backlog")`). Because `done` *is* a status value, completing a ticket erases the stage it came from: a reopen — by anyone — lands in `backlog`, and a caller that means a particular stage sets it, since status is just another field.
- Every `!done` predicate — in the store, the filter, and both faces — becomes a status comparison. `Filter`'s three values survive as sugar over the axes (`open` = not done and not archived).
- The GUI's "Clear done" stops being a permanent delete and becomes "Archive done". Single-item delete stays destructive.
- `section` keeps its stored name while the concept is a cycle lane; a key rename rides with the migrator, never before it.
- Tickets themselves move: this epic's work items are Tandem todos, and the repo keeps only the record — the map, the spec, the ADRs, `GLOSSARY.md`.

## Amendment, 2026-10-20: the mirror retires

The precondition above — no binary older than the ticket model may touch the store — was met by installing 0.12.0, the first shipping build carrying Status. The mirror is now **retired from the file**: every write emits `status` alone, and the first write over a file that still carries `done` copies that file aside as `todos.json.pre-migrator-<ts>` before rewriting it. "A list moved aside, never a list lost."

Reading still honours `done` when it is there: the stale-writer rule above is unchanged, the decoder accepts both shapes permanently, and a store only ever read keeps its old file. The CLI's JSON keeps the derived `done` — that wire shape is older than Status, and agents parse it; only the store file drops the key. The envelope stays `version: 1`: a v1-era reader treats a missing required key exactly as it treats an unknown version, so a bump would label the change without protecting anyone. The gate was reachability, and it was met by installing the build, not by labelling the file.
