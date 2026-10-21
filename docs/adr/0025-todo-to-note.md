# A ticket can become a note — the other half of the note↔ticket pair

[ADR 0010](0010-todo-note-links.md) made a todo point at a bikeshed note. This adds the other half of that pair: **a ticket can be promoted to a note** — the note is created from the ticket's title and description, and the ticket's `link` points at the note it became.

## Why

The ticket is where work is captured; the note is where prose lives. A ticket that has grown a body — a decision, a plan, a spec sketch — is prose wearing a ticket's shape: it has a status, a lane, an assignee, and a place on the board, all of which say *work item*, while its content says *document*. The only way to give it a note today is to write the note by hand and then link the two — copying the body across by hand, which is the one step that loses the thing you do not want to lose.

The pairing is not symmetrical, and that is fine: a ticket can become a note, and a piece of a note can become a ticket. Both directions end with the same shape — a `link` pointing at a bikeshed-root-relative path (ADR 0010) — so the paperclip opens the note either way.

## Decision

- **One core operation, `TodoToNote.promote(_:)`** (GUI-only, like `setLink` and `update`; no CLI verb). It writes the bikeshed and the store, and it owns the rules: the ticket stays exactly as it is, the note is new, and the link points at it.
- **The note is the ticket's title and description**: the title becomes the heading (`# <title>`), the description the body. The ticket's *state* — status, lane, assignee, done-ness — is not written into the note: a note carries prose, not todo state, the same line the note→ticket direction draws.
- **The note lands at `projects/<lane>/<slug>.md`** — the lane is the project's folder (a ticket in the unnamed default area lands in `projects/uncategorized/`), the title is the name. The slug is lowercase, every run of non-alphanumerics collapsed to a single `-`, trimmed, and capped at 60 characters.
- **The note is created, never overwritten.** A name already taken — another ticket's note, or a note the human wrote by hand — gets a `-2`, `-3`, … suffix, and the note that was there stays byte-for-byte as it was. The pick and the write happen under the bikeshed lock, so two writers cannot pick the same name.
- **A ticket that already links to a note is refused**, not re-promoted: the second note would point the ticket away from the first, orphaning prose that may have been written by hand. The refusal names the note the ticket already points at.
- **The ticket's status, lane, and place in the list do not change.** Promotion is not a move: the board keeps the work item, the note holds the prose.
- **The two writes are ordered, and the promotion rolls back.** The note is written first, the link second: a note without a link is a note the human can still find and use, while a link without a note is a link to nothing. A failed link write takes the note back off the disk, so a promotion lands whole or not at all.
- **The link opens the note from the sheet**, the same reader ADR 0010's paperclip uses.

## Decided against

- **A CLI verb** — the agent surface is `list`/`add`/`complete` ([ADR 0009](0009-cli-agent-face.md)); promotion is the human's gesture in the ticket sheet, and an agent that wants a note writes the note.
- **Overwriting a same-named note** — those bytes may be the human's own prose; a promotion that destroys them is worse than a suffixed filename.
- **Moving the ticket to done, or to a "promoted" status** — promotion is not a stage. A ticket that became a note is still the work item; its status says where the work is.
- **Writing the ticket's state into the note** (a `> status: ready` line, a `> lane: Cycle` line) — the same cross-reference graph ADR 0010 cut: the note's text would then mean different things depending on what the list holds.
- **Putting the note's path in the ticket's description** — the link is where the relationship belongs; a path in prose is a second copy to keep honest.
- **Folding the refusal into `DomainError`** — that enum is the *store's* vocabulary, and this rule is about the note as much as the ticket. `PromotionError` is its own type, conforming to `TandemError`, so the message is written once and both domains can speak it.

## Consequences

- The store's shape is unchanged: `link` is an existing additive field (ADR 0010), so the envelope stays `version: 1` ([ADR 0006](0006-stored-order-and-sections.md)) and no migration is needed.
- `TodoToNote` is a new core operation, tested headlessly (creation, the `-2` collision, the refusal, the link round-trip); the GUI has no test target, so the sheet's button is proven by hand.
- The promotion spans two domains — the store and the bikeshed — and one operation spans two locks. The order (note first, link second) and the rollback are the whole of the concurrency story; the refusal is checked twice, before the write and again inside the store's mutation, so a writer that lands a link in between loses the race rather than the note.
- The note the promotion writes is a **new** file, so the bikeshed's own watcher's tree picks it up as any other note; the sheet keeps its own copy of the new link, because the ticket it was opened with is a snapshot that does not know about it yet.
