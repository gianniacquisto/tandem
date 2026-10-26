---
name: tandem-memory
description: "Memory and the wiki of work, backed by the Tandem bikeshed (`~/.tandem/bikeshed/`) — a plain folder of markdown notes. Reach this at the **start of a session in any project on this Mac, before reading or editing project files**, to orient: read `Index.md`, the matching project's Overview, and Tandem's open threads (`tandem-cli ticket list`). Reach it again **as you work** to record — file a capture with `tandem-cli note capture` when you make a non-obvious decision, hit a gotcha, or open/close a thread; don't batch to session end."
---

# Tandem memory

The **bikeshed** (`~/.tandem/bikeshed/`) is the wiki of all work on this Mac and its memory: a plain folder of `[[wikilinked]]` markdown notes, shared by humans and agents and written for the next agent session as reader. It has two layers: **captures** — small, per-moment notes filed into a project's saddlebag as work happens — and the **curated** notes, which only a consolidation pass writes. This skill orients any session from the bikeshed and records its work back into it.

## Orient — pick up where the work left off

Before reading or editing project files, in order:

1. Read **`Index.md`** — the Hub at the bikeshed root: pointers and one-line status.
2. Match the task to a project line, then read that project's **Overview** — `projects/<project>/<project>.md`, the anchor of its fixed scaffold (Overview, Decisions, Risks, Processes). A **project is always a folder** holding its scaffold, its sub-pages, and a hidden saddlebag (`.raw/`). Read the Overview's `Where left off` in full; pull the project's Decisions, Risks, and Processes pages — or a linked sub-page — only when the task touches them.
3. Follow only the note's **wikilinks** the task needs.
4. Check `tandem-cli ticket list` for the open **threads**.

No project line matches → read the Index `Topics` list, then grep the bikeshed; the project's scaffold is created on demand by the consolidation pass (Record). **Read protocol** — retrieval order, staleness repair, freshness, human-note authority → `READ-PROTOCOL.md`.

**Done when** the session can resume from the bikeshed alone: the project's current state, its next move, and its open threads in hand — or, with no matching project, has read the `Topics` list, grepped, and knows there is no prior memory.

## Record — every agent files captures; one pass writes curated

**File a capture at each moment.** At each qualifying moment — a non-obvious decision, a gotcha or hard-won fact, a thread opened or closed — file one capture with `tandem-cli note capture`, right then, not batched to session end. This includes sub-agents: a child ends its session with a summary before it returns. The kinds, body shape, and naming are the verb's contract, not yours → `skills/tandem/SKILL.md`. **There is no floor `work` note** — the summary at `session end` (ADR 0027) is the work record. **Write protocol** — cadence, the qualification gate, threads as tickets, the curated shape → `WRITE-PROTOCOL.md`. **Concurrency** — captures create-only, the curated write boundary → `CONCURRENCY.md`.

**Never edit a curated note.** `Index.md` and the notes under `projects/**` have exactly one writer: the **consolidation agent** — a distinct agent the root hands the saddlebag to on request — never automatically at session close — or a human runs on demand. The pass's inputs are the agents' captures, the human's daily notes, and the ended session records' summaries; the daily notes are read-only to it — it folds, never edits, moves or archives ([`CONVENTIONS.md`](CONVENTIONS.md)). Staleness you notice becomes a `gotcha`/`fact` capture for that pass — never a patch in place. **Consolidation protocol** — the inputs, the writes, the archive → `CONSOLIDATION.md`. **Host bindings** — how a given host wires the pass, and reaches the hidden layer → `HARNESS.md`.

### Dispatch discipline

A **working** sub-agent carries this discipline already: the root's instruction file reaches a child session too, so its brief holds the task and nothing else. The child reads the skill off that instruction — orient (Index → the project's Overview → `tandem-cli ticket list`), file captures as work happens. Where a host does not deliver ambient instructions to children, that gap belongs to [`HARNESS.md`](HARNESS.md).

Consolidation is a distinct agent, never one of those briefed children: when consolidation is requested the root **hands the saddlebag to the consolidation agent** — a fresh agent that reads its own protocol, so the root briefs it with nothing beyond the queue. It does not run at session close unless asked.

Write the note itself for the next session as reader: apply the installed **`writing-for-agents`** skill and its `SKILL-MECHANICS.md`.

**Done when** a cold session could resume from the bikeshed alone — or would be strictly slower without it.
