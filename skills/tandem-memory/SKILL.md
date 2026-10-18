---
name: tandem-memory
description: "Memory and the wiki of work, backed by the Tandem bikeshed (`~/.tandem/bikeshed/`) — a plain folder of markdown notes. Reach this at the **start of a session in any project on this Mac, before reading or editing project files**, to orient: read `Index.md`, the matching project note, and Tandem's open threads (`tandem-cli todo list`). Reach it again **as you work** to record — file a raw note with `tandem-cli note capture` when you make a non-obvious decision, hit a gotcha, finish a significant unit of work, or open/close a thread; don't batch to session end."
---

# Tandem memory

The **bikeshed** (`~/.tandem/bikeshed/`) is the wiki of all work on this Mac and its memory: a plain folder of `[[wikilinked]]` markdown notes, shared by humans and agents and written for the next agent session as reader. It has two layers: **raw** capture notes — small, per-moment, filed by every agent as work happens — and the **curated** notes, which only a consolidation pass writes. This skill orients any session from the bikeshed and records its work back into it.

## Orient — pick up where the work left off

Before reading or editing project files, in order:

1. Read **`Index.md`** — the Orientation note at the bikeshed root: pointers and one-line status.
2. Match the task to a project line, then read that project's **curated note** — `projects/<project>/<anchor>.md`. A **project is always a folder** holding its curated notes and a hidden `.raw/` capture layer. Read `Where left off` in full; `Decisions` / `Gotchas` / `Facts` scanned by heading and pulled only when the task touches them.
3. Follow only the note's **wikilinks** the task needs.
4. Check `tandem-cli todo list` for the open **threads**.

No project line matches → read the Index `Topics` list, then grep the bikeshed; the project note is bootstrap-created at session end (Record). **Read protocol** — retrieval order, staleness repair, freshness, human-note authority → `READ-PROTOCOL.md`.

**Done when** the session can resume from the bikeshed alone: the project's current state, its next move, and its open threads in hand — or, with no matching project, has read the `Topics` list, grepped, and knows there is no prior memory.

## Record — every agent files raw; one pass writes curated

**File a raw note at each moment.** At each qualifying moment — a non-obvious decision, a gotcha or hard-won fact, a finished unit of work, a thread opened or closed — file one raw note with `tandem-cli note capture`, right then, not batched to session end. This includes sub-agents: a child finishes with one `work` note before it returns. The kinds, body shape, and naming are the verb's contract, not yours → `skills/tandem/SKILL.md`. **Floor: one `work` note at session end.** **Write protocol** — cadence, the loosened qualification gate, threads as todos, the curated shape → `WRITE-PROTOCOL.md`. **Concurrency** — raw create-only, the curated write boundary → `CONCURRENCY.md`.

**Never edit a curated note.** `Index.md` and the notes under `projects/**` have exactly one writer: the **`consolidator`** agent — the consolidation pass dispatched at session close (or on demand), or run by the human as `/consolidate`. Staleness you notice becomes a raw `gotcha`/`fact` note for that pass — never a patch in place. **Consolidation protocol** — the hidden queue, the writes, the archive → `CONSOLIDATION.md`.

### Dispatch discipline

A **working** sub-agent starts fresh and sees no skill, so the root agent carries this instruction into **every working sub-agent brief**: orient first (Index → the project's curated note → `tandem-cli todo list`), file raw notes as work happens, and finish with one `work` note. This lives here because the child cannot discover the skill on its own.

Consolidation is no longer hand-written into a brief: at session close the root **dispatches the `consolidator`** agent — its own OpenCode agent definition with its own permissions (`/consolidate` runs it too). The pass reads its own protocol; the root briefs it with nothing.

Write the note itself for the next session as reader: apply the installed **`writing-for-agents`** skill and its `SKILL-MECHANICS.md`.

**Done when** a cold session could resume from the bikeshed alone — or would be strictly slower without it.
