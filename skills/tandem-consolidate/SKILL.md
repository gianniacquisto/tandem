---
name: tandem-consolidate
description: "Run one consolidation pass over the pending Tandem bikeshed captures — the captures every agent filed are folded into the curated notes (`Index.md` and the notes under `projects/**`), and the consumed ones are archived. Reach for it when the human asks to consolidate, close out the memory, fold the captures, or refresh the curated notes. The pass has exactly one writer, the `consolidator` agent, and it never runs by itself at session close."
---

# Tandem consolidate

One consolidation pass over the whole pending raw queue. The captures stay where they are until the pass consumes them; the curated notes are rewritten from what it finds.

## The one writer

`Index.md` and every note under `projects/**` are written by exactly one thing: the **consolidator** agent (`agents/consolidator.md` in this repo). No working agent edits them, and neither do you.

So this skill does not consolidate anything itself. It **dispatches** the `consolidator` subagent, and reports what that agent did.

## The human's daily notes

The human keeps daily notes at the vault root: `daily/YYYY-MM-DD.md`, one file per day, written and edited in the app (⌘K or **File → Today's Note**). The harvest list is `["daily"]` today — the human-owned folders, a second one joining by being declared in the same list with the same contract. They are **human-owned and read-only to every agent**: the pass reads each file dated after the fold marker and **never edits, moves or archives** a source file.

The fold position is a dated line the consolidator owns in `Index.md` — `Human notes folded through 2026-10-26` — advanced only after a pass has read everything up to it. Discovery is the list plus the marker date, not mtime. The full protocol is `tandem-memory/CONSOLIDATION.md`.

Agent captures are unchanged: every agent still files with `tandem-cli note capture`, and the daily notes add no agent-facing write path — the absence is the enforcement.

## What to do

**Pick the mode before any dispatch** — if the human asks for more than one consolidation pass, the human in the session chooses: **serial** (the default, one pass at a time) or **parallel**. Caution: the curated notes have exactly one writer at a time and passes consume the same raw queue, so parallel passes risk write races.

1. **Dispatch the `consolidator` subagent** — one task, one brief, nothing else:

   > Run one consolidation pass over the whole pending raw queue, then report what you folded, where, and what you archived.

   The agent's own prompt and the protocol it reads are the contract: it reads `tandem-memory`'s `CONSOLIDATION.md` and follows it exactly. Do not restate that protocol in the brief, and do not pass it captures to promote — it reads the queue itself.
2. **Report the agent's report** — what it folded, where, what it archived, and anything it left pending.

## Never

- **Do the pass yourself.** A consolidation run by the wrong writer breaks the one-writer rule the curated layer depends on.
- **Run the pass unasked.** Consolidation is deliberate: at session close the captures simply keep. The human asks for it; the root hands the saddlebag over.
- **Edit a curated note to "fix" one.** A contradiction between a note and the repo is a `gotcha` or `fact` capture, and the pass corrects the note in its own run.

## The protocol

The pass's own rules live with the memory skill: `CONSOLIDATION.md` (the pass), `CONVENTIONS.md` (where notes go), `WRITE-PROTOCOL.md` (what a curated note says), `READ-PROTOCOL.md` (the read walk), `CONCURRENCY.md` (the write boundary). The agent reads them; you do not need to.
