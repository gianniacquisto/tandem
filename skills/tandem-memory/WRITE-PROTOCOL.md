# Write protocol — when and what to record

When and what a session records, so the next session picks up without re-deriving what this one learned. The record is the **session summary**; a **capture** files only what a summary cannot carry, and a **consolidation pass** promotes captures into the **curated** notes. This file is the working agent's capture rule and the curated shape it writes toward; the pass's own protocol is [`CONSOLIDATION.md`](CONSOLIDATION.md); the layout is [`CONVENTIONS.md`](CONVENTIONS.md); the read walk is [`READ-PROTOCOL.md`](READ-PROTOCOL.md); the write boundary is [`CONCURRENCY.md`](CONCURRENCY.md). Apply the installed `writing-for-agents` skill while writing every note — it is the writing discipline, not restated here.

## The record — the session summary; a capture for one real re-derivation cost

The work record is the **session summary**: at session end (or when context nears exhaustion), `tandem-cli session end --summary -` records what was done and where the session left off (ADR 0027) — a sub-agent's minimum deliverable before it returns, too. The consolidation pass reads the ended session records' summaries ([`CONSOLIDATION.md`](CONSOLIDATION.md)).

A **capture** is for one thing only: a decision, gotcha or fact whose **cost of re-derivation is real**, and which is not already in a session record, a ticket, an ADR or the code. A session that ends with its summary and no capture is compliant.

```
printf '# <short title>\n\n<1-3 lines>\n' | tandem-cli note capture --kind <kind> --project <project>
```

What earns one:

- A **decision** the code cannot show — `decision`, the choice plus a one-line rationale.
- A **gotcha or hard-won fact** — `gotcha` / `fact`: an hour of re-derivation behind one line.
- A **thread** — `thread`, the context a title cannot carry; the ticket list stays the live list of threads.

The verb owns the kinds, the body shape (`# <short title>` then 1–3 lines), and the naming — its contract is `skills/tandem/SKILL.md`. Captures are **create-only** and carry **no `[[wikilinks]]`**; the consolidation pass adds those when it promotes a note.

**Threads stay immediate**: `tandem-cli ticket add --title "…"` runs the moment a thread opens — the ticket list is the authoritative live list. Closing one: `tandem-cli ticket complete --id <id>`, plus a `thread` capture only for the context a title cannot carry.

## What fails the cost test

The re-derivation test is what decides, and it rides on the capture, not on promotion. What the repo, the environment, or the session transcript reproduces on the next look — a command's output, a file's contents, a `--help`, a narration of what you are about to do — fails it; leave it out. The ≲50/≲40 length targets and the pruning discipline govern what consolidation **promotes** into the curated notes, not what you capture.

## Working agents never edit curated notes

`Index.md` and the curated notes under `projects/**` have exactly one writer: the consolidation pass (decision 8). Staleness a working agent notices — a note that contradicts the repo, a flipped decision, an outdated status — is captured as a `gotcha`/`fact` capture for the consolidation pass, never patched in place ([`CONCURRENCY.md`](CONCURRENCY.md)).

## The closing pass

At session end (or when context nears exhaustion), end the session record — `session end --summary -`; the summary is the work record. The captures simply keep — consolidation is deliberate, never automatic. When the human asks (or the root was told to close out), **hand the saddlebag to the consolidation agent** ([`CONSOLIDATION.md`](CONSOLIDATION.md)) — a fresh agent, never a recording sub-agent, that folds this session's captures into the curated notes and refreshes the Index. The human can run the same pass on demand. The working session does not consolidate itself.

## The curated shape — what the consolidation pass writes

The **scaffold pages** have a fixed shape of their own ([`CONVENTIONS.md`](CONVENTIONS.md)).

**Overview** (`projects/<project>/<project>.md`) — the anchor:

```
# <Project>

<one line: what this project is>

## Where left off
## Threads
## Notes
```

- `Where left off` — state and next move; the one section read in full at orientation, and the summary the front page shows.
- `Threads` — the project's open threads: the ticket's title verbatim plus a one-clause hook.
- `Notes` — the project's other notes: its scaffold pages and its linked sub-pages, one line each.

The page as a whole is the project's documentation — what this is, how to run it, where things live, and what is open; its role and the freshness rule are [`CONVENTIONS.md`](CONVENTIONS.md)'s.

**Decisions**, **Risks**, and **Processes** — one line per entry, no sections:

```
# <Project> Decisions

<one line: what this page is>

- **the choice** — its rationale. Link to its ADR or ticket.
```

One line per entry, pointers over restatement: a Decisions entry links to its ADR (which stays in the repo) or its ticket; a **decision that flips** leaves one dated line — `**X** — reversed <date>: <reason>` — never two live contradicting lines.

Everything else — a **sub-page** or a **topic note** — uses the skeleton, in this order, with these exact `##` headings:

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

Targets: ≲50 lines the Overview, ≲40 a sub-page or a topic — a *trigger*, not a rule. The **Overview is an overview**, so its ≲50 lines are an index budget, kept small by **linking out**, not by compressing detail. The scaffold's entry pages hold one line per entry; an entry that grows into a topic earns the sub-page that owns it, and keeps its one line plus link. When a section outgrows its target **or** a coherent subsystem appears ([`CONVENTIONS.md`](CONVENTIONS.md)), the pass **creates the sub-page and links it from the Overview** — it never deepens the Overview past its overview role. Pointers over restatement, one-line entries, no code, no step-by-step procedures.

**Prune on write**: adding a line invites retiring one, so the target stays live because every write makes room. Delete whole any line the repo or environment reproduces, a stale line, or a no-op. (The Bikeshed has no history; the dated reversal line is the stopgap.)

## Completion criterion

A write is done when a cold session could resume from the Bikeshed alone — current state, next move, open Threads in hand — or would be strictly slower without it. For the working agent that means the captures are filed and the consolidation pass is dispatched.
