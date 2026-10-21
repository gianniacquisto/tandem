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

## Amendment, 2026-10-20: a sixth Status, `in-review`

Status gains a sixth value, **`in-review`** (displayed `In Review`), between `in-progress` and `done`. It names the wait the earlier five could not express: the work and its PR exist, a reviewer has not yet read them, and only then does the ticket become `done`. The value sits on the same axis, with the same free-form transitions and the same guarantee that completing a ticket erases the stage it came from.

The compatibility rule is the mirror's rule, one step earlier: an unknown `status` still decodes as `backlog`, so an older build shows an `in-review` ticket in the wrong column rather than calling the file corrupt. Order is therefore release the build carrying the value → install → then let any writer store `in-review`; never write it from a build older than the installed one. The envelope stays `version: 1` — the change is additive, and a bump would label it without protecting anyone.

`reviewed` was the ask's word; `in-review` was chosen because it names the state a ticket is in, the way `in-progress` does. The word is one enum case and one display string if the human prefers theirs.

## Amendment, 2026-10-20: comments are an append-only sidecar

The recorded no on *comments* is reversed — the human has asked for them by name, and the no was deliberate, so this is an amendment, not a correction. A ticket's comments live in a **sidecar, not a field**: `<home>/comments/<ticket-id>.jsonl`, one JSON object per line (`author`, `at` in ISO-8601 UTC, `body` in markdown), appended under the store's existing `tandem.lock` flock. The directory is created on the first append; the log is append-only — there is no edit and no delete, and a deleted ticket's log stays on disk. An unknown id on `append` is a domain failure carrying the open list, like `complete`; on `list` it is an empty list, never an error.

Why a sidecar: the installed app rewrites `todos.json` whole and drops keys it does not know — the same tax `origin` pays — so an in-ticket `comments` array would be erased by the next human edit until a build carrying the field is installed and the reader-first dance run, for a feature whose whole point is not losing history. A sidecar cannot be erased by an older writer, and append-only suits two agents taking turns on one ticket. The description stays refinement-only — it is refinement's output, not a conversation — and a linked bikeshed note remains the long-form escape hatch. `tandem-cli todo comment add|list` reaches the log; a GUI panel is a separate ticket.

## Amendment, 2026-10-21: the CLI's filters never carry the archive

The archive is one axis, but the two faces read it differently. The app's `All` still shows the archive — the app has an Archived segment of its own (⌘4), and the archive is where a human brings a ticket back. The **CLI's** filters never do: `tandem-cli todo list --filter all` is every ticket *not* archived, because an agent's read otherwise pays for the whole history — most of the store, on this Mac. The archive is reached by asking for it: `tandem-cli todo list --archived` is the archive on its own — the archived tickets, statuses ignored, the app's Archived segment — and named with a filter it joins that filter, so `--filter all --archived` is every ticket.

Why the CLI alone: the archive is the human's to keep tidy, while an agent's read is a question about live work. The faces ask different questions, so they answer them differently, and the archive stays one axis in the query rather than becoming two filters. The axis itself is unchanged — `TicketQuery.Archived` is still three-valued, and `.only` is what the CLI's bare `--archived` asks for.

The flag is a switch, not a value: `--archived` takes none, and the CLI's flag grammar grows its first one.
