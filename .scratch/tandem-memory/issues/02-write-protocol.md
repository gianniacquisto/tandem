# 02 - Write protocol: when and how sessions record highlights

Type: grilling
Status: resolved (2026-10-17)
Blocked by: 01

## Question

Given the layout and entry point settled in 01, settle the write protocol the skill encodes:

1. **When**: which moments in a session trigger a write — decisions as they are made, state changes, a session-end pass — and how those compose over a long session.
2. **What qualifies**: applying the highlight taxonomy (where-left-off, decisions + one-line rationale, learnings/gotchas, expensive-to-derive facts) with the re-derivation test; judgment calls for borderline content.
3. **Update vs create**: when a session amends an existing note vs creates a new one; keeping a single source of truth (one meaning, one place); how stale content is corrected or superseded rather than left to sediment.
4. **Open threads**: how a session records follow-ups as tandem todos via MCP `add` — what a todo title carries — and how notes wikilink to that context.
5. **Style**: how notes are written for the next session as reader (per `writing-for-agents`): pointers over restatement, hierarchy, leading words, pruning; a ceiling on note length that keeps reads cheap.

## Answer

Settled 2026-10-17 (grilling with the human). The skill encodes the write protocol below.

**1. Cadence — eager, event-driven.** One small edit at each qualifying moment, never a batch. The moments (the ones the skill's always-loaded description names): a non-obvious decision (+ one-line rationale); a gotcha or hard-won fact; a significant unit of work finished; an open thread opened or closed. Plus a **closing pass** at session end *or when context nears exhaustion*: refresh `Where left off`; re-derive the project's `Index` line **only if the status changed** — adding to Decisions/Gotchas never churns the Index.

*Load-bearing rationale:* the notes **are** the compression. A session that would need compacting instead ends, and the next session cold-starts from the vault. Eager, dense writes are what make compaction unnecessary, so avoiding compaction is the point of the protocol, not a side effect.

**2. Qualification gate — cost of derivation.** Write when the information took real effort to derive and re-deriving it would cost that effort again (tokens, exploration breadth, dead ends). Omit what is cheaply reproducible from the repo/environment: a lookup, a file's contents, a command's output, a tool's `--help`. Judged by *effort to create*, not wall-clock — the same fact costs a cloud model and a local model differently. Borderline → one line, never a paragraph.

**3. Correct in place; supersede decisions.** A contradicting truth is edited into the covering note (single source of truth). Facts are corrected silently; a **decision that flips** leaves one dated line — `**X** — reversed <date>: <reason>` — so the next session does not re-litigate it. Never two live contradicting lines; never append-only.

**4. Create vs amend.** Amend the covering note whenever one exists; create a new note only when ticket 01's rule fires (2+ notes must link to it, or it spans projects). Bootstrap: a project's first qualifying highlight creates `projects/<Project>.md` (full skeleton, `Where left off` first) **and** its `Index` line in the same write; the anchor becomes a folder only when a second note is earned.

**5. Threads.** An open thread = a Tandem todo (MCP `add`) + a bullet in the owning note's `## Threads`, leading with the todo title **verbatim** plus a one-clause hook. A closed thread = MCP `complete` + the bullet becomes a dated resolved one-liner. The todos stay the single authoritative list; the note carries only the context a title cannot. No wikilink — todos are not notes.

**6. Style and ceiling.** Fixed skeleton (`Where left off` / `Decisions` / `Gotchas` / `Facts` / `Threads`) across project anchors and topic notes. Target ≲50 lines an anchor, ≲40 a topic — a *trigger*, not a rule: prune on write (adding a line invites retiring one), and a section outgrowing the target is the signal the concept earned its own note. Pointers over restatement, one-line entries, no code, no steps.

**7. Completion criterion.** A write is done when a cold session could resume from the vault alone — or would be strictly slower without it.

Parked for later, not this ticket: vault version control (no history today; the dated reversal line in (3) is the stopgap) and a consolidation pass that densifies notes over time — see the map's Out of scope and Not yet specified.
