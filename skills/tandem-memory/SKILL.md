---
name: tandem-memory
description: "Memory and the wiki of work, backed by the Tandem bikeshed (`~/.tandem/bikeshed/`) — a plain folder of markdown notes. Reach this at the **start of a session in any project on this Mac, before reading or editing project files**, to orient: read `Index.md`, the matching project anchor, and Tandem's open threads (`tandem-cli todo list`). Reach it again **as you work** to record — write a memory note when you make a non-obvious decision, hit a gotcha, finish a significant unit of work, or open/close a thread; don't batch to session end."
---

# Tandem memory

The **bikeshed** (`~/.tandem/bikeshed/`) is the wiki of all work on this Mac and its memory: a plain folder of `[[wikilinked]]` markdown notes, shared by humans and agents and written for the next agent session as reader. This skill orients any session from the vault and records its work back into it, in two steps — **Orient** at session start, **Record** as work happens. The reference detail lives in four companions, each reached by its pointer below when that branch fires.

## Orient — pick up where the work left off

Before reading or editing project files, in order:

1. Read **`Index.md`** — the Orientation note at the vault root: pointers and one-line status.
2. Match the task to a project line, then read that project's **anchor note** — `Where left off` in full; `Decisions` / `Gotchas` / `Facts` scanned by heading and pulled only when the task touches them.
3. Follow only the anchor's **wikilinks** the task needs.
4. Check `tandem-cli todo list` for the open **threads**.

No project line matches → read the Index `Topics` list, then grep the vault; create the anchor at session end (Record). **Read protocol** — retrieval order, staleness repair, freshness, human-note authority → `READ-PROTOCOL.md`.

**Done when** the session can resume from the vault alone: the project's current state, its next move, and its open threads in hand — or, with no matching project, has read the `Topics` list, grepped, and knows there is no prior memory.

## Record — keep the vault current as you work

At each qualifying moment, write one small edit — right then, not batched to session end. The moments:

- a non-obvious **decision**, with a one-line rationale;
- a **gotcha** or a hard-won **fact**;
- a finished unit of work;
- an opened or closed **thread**.

Then the **closing pass**, at session end or when context nears exhaustion: refresh `Where left off`, and re-derive the `Index` line only if the status changed.

Qualify by **cost of derivation** — write what took real effort to derive and would cost it again; leave out what the repo or environment reproduces. Correct a fact in place; a flipped decision leaves one dated reversal line. Amend the covering note; create one only when the **conventions** rule fires → `CONVENTIONS.md`. **Write protocol** — cadence, the fixed skeleton and length targets, pruning, threads as todos → `WRITE-PROTOCOL.md`. **Concurrency** — shared ownership, the lock-free write boundary, atomic anchor bootstrap → `CONCURRENCY.md`.

Write the note itself for the next session as reader: apply the installed **`writing-for-agents`** skill and its `SKILL-MECHANICS.md`.

**Done when** a cold session could resume from the vault alone — or would be strictly slower without it.
