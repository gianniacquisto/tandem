# 03 - Read and orientation protocol: picking up, and finding related information

Type: grilling
Status: resolved (2026-10-17)
Blocked by: 01

## Question

Given the layout and entry point settled in 01, settle the read protocol the skill encodes:

1. **Orientation**: what a new session reads, in what order, before doing work — the entry point from 01, then which notes — and how much to load: enough to pick up where the work left off, without burning the context window.
2. **Retrieval**: how a session finds information related to the topic it is working on — wikilinks, backlinks, plain search of the vault (the agent can grep `~/.tandem/bikeshed/`; the app's full-text search is the same operation with a UI) — and the order of preference among them.
3. **Staleness**: what a session does when what it reads contradicts the current state of the repo — re-verify, correct the note, or work around it; and what it does when a wikilink is broken (the note was renamed or deleted).
4. **Freshness during a session**: how often a long session re-reads its notes, or when it may trust its earlier reads.
5. **Human notes as memory sources**: which human notes count as project context when orienting (the intermixed case settled in 01: a human note in `projects/` with no agent memory yet), and how much weight they carry next to agent notes.

## Answer

Settled 2026-10-17 (grilling with the human). The skill encodes the read protocol below.

**1. Orientation — a small fixed mandatory read, then work.** The cold-start walk is the prototype's five steps: open `Index.md` (the one fixed constant every session knows); match the task to a project line; read that project anchor — `Where left off` in full, `Decisions`/`Gotchas`/`Facts` as reference scanned by heading and pulled only when the task touches them; follow the anchor's wikilinks outward only as needed; check `MCP list` for open threads. **Stop** as soon as the session can state the project's current state, its next move, and which threads are open. Topic notes and human root notes are **not** read at orientation — they are reached on demand (2). No matching project → read only the Index's `topics/` list, then grep the vault; the anchor is bootstrap-created at session end (write protocol, 02).

**2. Retrieval — curated links, then backlinks, then grep.** Order of preference: (i) **wikilinks the anchor already names** — curated for this session, a match beats any search; (ii) **backlinks** when the question is what *depends on* a note or what its neighbourhood is — the agent's stand-in for the app's Backlinks dock is `grep -r '[[Name]]'`; (iii) **keyword grep** of `~/.tandem/bikeshed/` when neither surfaces it. Resolving `[[Name]]` is a filename match (case-insensitive, unique basename), not a search — the target *is* a file. The same three needs the app's Backlinks dock and full-text search serve for the human.

**3. Staleness — the repo is ground truth; repair in place, opportunistically.** When a note contradicts the current repo, work from the repo and **correct the note in place** (02's rule); if a *decision* flipped, leave the one dated reversal line, not a silent edit. Re-verify cheaply first — moved code may not mean changed intent. Never work *around* a stale note silently; that leaves the trap for the next session. A broken `[[wikilink]]` (the note was renamed outside the app, or deleted — agent links are born only alongside their target, and app renames self-heal per ADR 0005) is fixed **when the note is touched**, not by a vault-wide sweep: search for the near-identical target name and rewrite the link; if the target is gone, demote the reference to plain text — never re-point it at a guess.

**4. Freshness — trust the session's reads; re-read only on a trigger.** Reads are trusted for the life of the session; unprompted re-reading is wasted context. Two triggers to re-read: **immediately before amending** a note (the write protocol's re-read; the collision rule is 05), and on an **observed external change** (the app reports a change, another session's todo appears, the human edits a note). No polling. A session's own writes are authoritative for its own state; the only risk is others' concurrent writes, which is 05's territory.

**5. Human notes — same read path, lower authority.** Anything not written by a session reads as human; there is no author field and the read path does not depend on telling them apart. Any note under the project counts as project context and is read; a bare human stub is extended (01), never shadowed. But human prose is **context, not state of record**: it may be aspirational or stale, so it is not treated as a settled decision and current state is re-derived from the repo — only an agent memory note's `Where left off`/`Decisions` carries that authority. Human notes outside the project (`Welcome`, `Ideas`, root notes) are wiki, not orientation: reached by link or grep.

**6. Completion criterion.** Orientation is done when the session can resume the project from the vault alone — current state, next move, open threads in hand.

Parked for later, not this ticket: retrieval quality as the vault accumulates — **link density** (when graph navigation degrades) and **growth/decay** — stay in the map's *Not yet specified*; agent-vs-agent freshness collisions are 05.
