# Write protocol — when and what to record

When and what a session records, so the next session picks up without re-deriving what this one learned. The layout it writes into is [`CONVENTIONS.md`](CONVENTIONS.md); the read walk that precedes a write is [`READ-PROTOCOL.md`](READ-PROTOCOL.md); the write boundary is [`CONCURRENCY.md`](CONCURRENCY.md). Apply the installed `writing-for-agents` skill while writing every note — it is the writing discipline, not restated here.

## Cadence — eager, event-driven

Record one small edit at each qualifying moment, as it happens; never batch to session end. The moments:

- A **non-obvious decision** — the choice plus a one-line rationale.
- A **gotcha or hard-won fact**.
- A **significant unit of work finished**.
- A **Thread opened or closed**.

Plus the **closing pass**, at session end or when context nears exhaustion: refresh `Where left off`; re-derive the project's `Index` line and rewrite it **only if the status changed** — adding to `Decisions`/`Gotchas` never churns the `Index`.

*The notes are the compression.* A session that would compact instead ends, and the next cold-starts from the Bikeshed; eager, dense notes are what make compaction unnecessary.

## Qualification gate — cost of derivation

Record when the information took real effort to derive and re-deriving it would cost that effort again (tokens, exploration breadth, dead ends). Omit what the repo or environment reproduces cheaply — a lookup, a file's contents, a command's output, a tool's `--help`. Judge by **effort to create**, not wall-clock: the same fact costs a cloud model and a local model differently. Borderline → one line, never a paragraph.

## Correct in place; supersede a flipped decision

The covering note is the single source of truth: edit a contradicting truth into it. Facts are corrected silently; a **decision that flips** leaves one dated line so the next session does not re-litigate it — `**X** — reversed <date>: <reason>`. Never two live contradicting lines; never append-only. Correcting a note against the repo is [`READ-PROTOCOL.md`](READ-PROTOCOL.md); concurrency of the edit is [`CONCURRENCY.md`](CONCURRENCY.md)'s write boundary.

## Create vs amend

Amend the covering note whenever one exists. Create a new Note only when [`CONVENTIONS.md`](CONVENTIONS.md)'s granularity rule fires (2+ notes must wikilink to it, or it spans projects); placement, naming, and extending a human note are also [`CONVENTIONS.md`](CONVENTIONS.md).

**Anchor bootstrap.** A project's first qualifying highlight creates `projects/<Project>.md` — full skeleton, `Where left off` first — **and** its `Index` line in the same write. The anchor becomes `projects/<P>/` with the anchor inside only when a second note is earned. A concurrent anchor create is [`CONCURRENCY.md`](CONCURRENCY.md)'s atomic-create case.

## Threads

An **open Thread** is a Tandem Todo (`tandem-cli todo add --title "…"`) **plus** a bullet in the owning note's `## Threads`, leading with the Todo title **verbatim** and a one-clause hook for the context a title cannot carry. A **closed Thread** is `tandem-cli todo complete --id <id>` plus the bullet becoming a dated resolved one-liner.

- `- <Todo title verbatim> — <one-clause hook>`
- `- <Todo title> — resolved <date>`

The Todos stay the single authoritative list of active work; the note carries only the context. Threads are plain text, never `[[wikilinks]]` — Todos are not Notes, and the link graph stays clean. Active work lives in the Todos; durable memory lives in the Notes.

## The skeleton

Every Note — Anchor or topic — uses this skeleton, in this order, with these exact `##` headings:

```
# <Note name>

<one line: what this is>

## Where left off
## Decisions
## Gotchas
## Facts
## Threads
```

- `Where left off` — state and next move; the one section read in full at orientation.
- `Decisions` — one line per decision: the choice plus its rationale.
- `Gotchas` — what cost time; the trap the next session would fall into.
- `Facts` — expensive-to-derive facts.
- `Threads` — as above.

Targets: ≲50 lines an Anchor, ≲40 a topic — a *trigger*, not a rule. A section outgrowing its target is the signal the concept earned its own note ([`CONVENTIONS.md`](CONVENTIONS.md)). Pointers over restatement, one-line entries, no code, no step-by-step procedures.

## Prune on write

Adding a line invites retiring one; the target stays live because every write makes room. Delete whole any line the repo or environment reproduces, a stale line, or a no-op. (The Bikeshed has no history; the dated reversal line is the stopgap.)

## Completion criterion

A write is done when a cold session could resume from the Bikeshed alone — current state, next move, open Threads in hand — or would be strictly slower without it.
