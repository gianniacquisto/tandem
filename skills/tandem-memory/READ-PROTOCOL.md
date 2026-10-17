# Read protocol — orienting and finding related notes

How a session picks up where the work left off, and finds related information cheaply. The layout it walks is in [`CONVENTIONS.md`](CONVENTIONS.md); the writes it triggers are in [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md); concurrent reads are in [`CONCURRENCY.md`](CONCURRENCY.md).

## Orient — a small fixed read, then work

At the start of a session, before reading or editing project files:

1. **Open `Index.md`** — the one fixed constant. In one small read it names every project the vault has memory for, and where the topics live.
2. **Match the task to a project line** — that line's wikilink is the project's **anchor note**.
3. **Read the anchor** — `Where left off` in full; `Decisions`, `Gotchas`, `Facts` scanned by heading and pulled only when the task touches them.
4. **Follow the anchor's wikilinks outward** only as the task needs them.
5. **Check the Tandem todos** (MCP `list`) for the open threads.

**Stop** as soon as you can state the project's **current state**, its **next move**, and which **threads are open**. Topic notes and human root notes are read on demand — see Retrieval — never at orientation.

No project matches the task → read only the Index's `Topics` list, then grep the vault for reusable knowledge; the anchor is bootstrap-created at session end ([`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md)).

**Done when** a cold session could resume the project from the vault alone — current state, next move, open threads in hand.

## Retrieval — curated links, then backlinks, then grep

Preference order:

1. **Wikilinks the anchor already names** — curated for this session; a match beats any search.
2. **Backlinks**, when the question is what *depends on* a note or what its neighbourhood is. The agent's stand-in for the app's Backlinks panel is `grep -rF '[[Name]]' ~/.tandem/bikeshed/` — `-F` keeps the brackets literal.
3. **Keyword grep** of `~/.tandem/bikeshed/` when neither surfaces it.

Resolving a `[[Name]]` is a **filename match** — case-insensitive, unique basename. The target *is* a file, not a search result. The app's Backlinks panel and full-text search are the human's equivalent of the backlink and grep steps.

## Staleness — the repo is ground truth

When a note contradicts the current repo, work from the **repo** and correct the note in place: a fact is corrected silently; a **decision that flipped** leaves the one dated reversal line ([`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md)). Re-verify cheaply first — moved code may not mean changed intent. Never silently work around a stale note; that leaves the trap for the next session.

A broken `[[wikilink]]` — its note was renamed **outside the app**, or deleted; the app rewrites links on its own renames, so those self-heal (ADR 0005) — is fixed **when you touch the note**, never by a vault-wide sweep: search for the near-identical target name and rewrite the link; if the target is gone, demote the reference to plain text. Never re-point a link at a guess.

## Freshness — trust this session's reads

A read is trusted for the life of the session; unprompted re-reading is wasted context. Two triggers re-read a note:

- **Immediately before amending** it — the write boundary; the collision rule is in [`CONCURRENCY.md`](CONCURRENCY.md).
- On an **observed external change** — the app reports a change, another session's todo appears, the human edits a note.

No polling. Your own writes are authoritative for your own state.

## Human notes — same read path, lower authority

Anything a session did not write reads as human; there is no author field, and the read path does not depend on telling the two apart. Any note under the project counts as project context and is read; a bare human stub is extended ([`CONVENTIONS.md`](CONVENTIONS.md)), never shadowed.

Human prose is **context, not state of record** — it may be aspirational or stale. Only a memory note's `Where left off`/`Decisions` carries current-state authority; re-derive current state from the repo. Human notes outside the project (`Welcome`, `Ideas`, root notes) are wiki, not orientation: reached by link or grep.
