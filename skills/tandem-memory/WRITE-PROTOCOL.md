# Write protocol — when and what to record

When and what a session records, so the next session picks up without re-deriving what this one learned. Two layers: the working agent files **captures**; a **consolidation pass** promotes them into the **curated** notes. This file is the working agent's cadence and the curated shape it writes toward; the pass's own protocol is [`CONSOLIDATION.md`](CONSOLIDATION.md); the layout is [`CONVENTIONS.md`](CONVENTIONS.md); the read walk is [`READ-PROTOCOL.md`](READ-PROTOCOL.md); the write boundary is [`CONCURRENCY.md`](CONCURRENCY.md). Apply the installed `writing-for-agents` skill while writing every note — it is the writing discipline, not restated here.

## Capture cadence — file a note at each moment

File one **capture** at each qualifying moment, as it happens; never batch to session end. **Every agent does this, sub-agents included.**

```
printf '# <short title>\n\n<1-3 lines>\n' | tandem-cli note capture --kind <kind> --project <project>
```

The moments and their kinds:

- A **non-obvious decision** — `decision`, the choice plus a one-line rationale.
- A **gotcha or hard-won fact** — `gotcha` / `fact`.
- A **significant unit of work finished** — `work`.
- A **Thread opened or closed** — `thread`, the context a title cannot carry.

The verb owns the kinds, the body shape (`# <short title>` then 1–3 lines), and the naming — its contract is `skills/tandem/SKILL.md`. Captures are **create-only** and carry **no `[[wikilinks]]`**; the consolidation pass adds those when it promotes a note.

**Floor: one `work` note at session end** (or when context nears exhaustion) — what was done and where it left off. That is a sub-agent's minimum deliverable before it returns.

**Threads stay immediate**: `tandem-cli todo add --title "…"` runs the moment a thread opens — the Todos are the authoritative live list — with a `thread` capture for the context a title cannot carry. Closing one: `tandem-cli todo complete --id <id>`, plus a `thread` note.

## Qualification gate — what you'd lose when the session ends

File when the information would be **lost when the session ends** and re-deriving it would cost real effort. A capture is cheap, so borderline notes are welcome — omit only what the repo or environment reproduces on the next look (a command's output, a file's contents, a `--help`). The stricter cost-of-derivation gate, the ≲50/≲40 length targets, and the pruning discipline govern only what consolidation **promotes** into the curated notes; they ride on that pass, not on the moment.

## Working agents never edit curated notes

`Index.md` and the curated notes under `projects/**` have exactly one writer: the consolidation pass (decision 8). Staleness a working agent notices — a note that contradicts the repo, a flipped decision, an outdated status — is captured as a `gotcha`/`fact` capture for the consolidation pass, never patched in place ([`CONCURRENCY.md`](CONCURRENCY.md)).

## The closing pass

At session end (or when context nears exhaustion), file the floor `work` note, then **hand the saddlebag to the consolidation agent** ([`CONSOLIDATION.md`](CONSOLIDATION.md)) — a fresh agent, never a recording sub-agent, that folds this session's captures into the curated notes and refreshes the Index. The human can run the same pass on demand. The working session does not consolidate itself.

## The curated shape — what the consolidation pass writes

Every curated note — project note or topic — uses this skeleton, in this order, with these exact `##` headings:

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
- `Threads` — as below.

Correct in place: the covering note is the single source of truth — edit a contradicting truth into it. Facts are corrected silently; a **decision that flips** leaves one dated line so the next session does not re-litigate it — `**X** — reversed <date>: <reason>`. Never two live contradicting lines; never append-only. Amend the covering note; create a new one only when [`CONVENTIONS.md`](CONVENTIONS.md)'s coherence test fires.

Targets: ≲50 lines a project note, ≲40 a topic — a *trigger*, not a rule. A **project note is an overview**, so its ≲50 lines are an index budget, kept small by **linking out**, not by compressing subsystem detail. When a section outgrows its target **or** a coherent subsystem appears ([`CONVENTIONS.md`](CONVENTIONS.md)), the pass **creates the subsystem note and links it from the project note** — it never deepens the project note past its overview role. Pointers over restatement, one-line entries, no code, no step-by-step procedures.

**Prune on write**: adding a line invites retiring one, so the target stays live because every write makes room. Delete whole any line the repo or environment reproduces, a stale line, or a no-op. (The Bikeshed has no history; the dated reversal line is the stopgap.)

## Completion criterion

A write is done when a cold session could resume from the Bikeshed alone — current state, next move, open Threads in hand — or would be strictly slower without it. For the working agent that means the captures are filed and the consolidation pass is dispatched.
