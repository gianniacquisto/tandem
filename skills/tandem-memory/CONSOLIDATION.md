# Consolidation — from captures to curated notes

The one writer of the curated Bikeshed: the **consolidation agent** — a distinct, fresh agent the root session **hands the saddlebag to on request** (or a human runs it on demand) — never automatically at session close — and **never** a recording sub-agent. It reads the agents' captures, the human's daily notes, and the ended session records' summaries as its inputs — distills them, and archives what it consumes (the daily notes excepted — they are read-only and stay where they are). This repo names it (`agents/consolidator.md`) and ships the skill that dispatches it (`skills/tandem-consolidate/`); how a host binds those is that host's own wiring → [`HARNESS.md`](HARNESS.md). The capture cadence is [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md); the layout and naming are [`CONVENTIONS.md`](CONVENTIONS.md); the write boundary is [`CONCURRENCY.md`](CONCURRENCY.md). Apply the installed `writing-for-agents` skill to every curated note.

It is the **only** writer of `Index.md` and the notes under `projects/**` — no working agent may touch them (decision 8). It never writes a capture, and it **never writes, moves or archives a daily note**.

## Read the inputs — the saddlebag, the daily notes, the session records

The pass reads the agents' saddlebag captures and the human's daily notes ([`CONVENTIONS.md`](CONVENTIONS.md)) — and, alongside them, the ended session records' summaries (ADR 0027).

**The saddlebag** — pending = every file under `projects/*/.raw/**` that is **not** under `.done/`. The `.raw/` subtree is hidden, so a plain glob or grep skips it:

- glob with `hidden: true`
- `rg --hidden …`
- `ls -a`

Read every pending file — the **whole** saddlebag, not a session's slice. Group by **project** (the `.raw/` folder's parent) and by **kind** (the filename, and where it lands). Drop what the repo or environment reproduces cheaply — a lookup, a file's contents, a command's output. Dedupe repeats within and across sessions.

**The daily notes** — the harvest list is `["daily"]` today: the **human-owned** folders at the vault root, and **read-only to the consolidator**. A daily note is `daily/YYYY-MM-DD.md`, one file per day; the human writes and edits it in the app (⌘K or **File → Today's Note**). Read every file dated after the fold marker and fold what it holds into the scaffold page or sub-page of whichever project it concerns. The pass **never edits, moves or archives** a daily note: the human keeps them, and the pass leaves every one byte-identical. A second folder joins by being declared in the same list, with the same contract — and gets its own marker line.

**The fold marker** — the pass's position in the daily notes, one dated line the consolidator owns in `Index.md`: `Human notes folded through 2026-10-26`. Advance it to the day of the last daily note whose content is fully folded, and only after a pass has read everything up to it — once per pass, never ahead of the fold. Nothing is written into the daily files themselves. **Discovery is the list plus the marker date, not mtime**: a touch must not re-fold a note, and an edit to a note dated before the marker is the human's to re-date.

**The session records** — every ended session's summary: `tandem-cli session list --project <project> --status ended`, then `tandem-cli session show --session <id>` for the `summary` field (ADR 0027). The summary is the session's work record — what it did and where it left off; there is no floor `work` capture to read. Records are rows of the store's `sessions` table in `tandem.db` (ADR 0031), never the saddlebag: they are read, not archived.

## Write the curated notes

For each project, route each moment to where the scaffold owns it ([`CONVENTIONS.md`](CONVENTIONS.md)):

| Kind / moment | Lands in |
| --- | --- |
| an ended session's **summary** | the **Overview**'s `Where left off` — current state and next move |
| `thread` | the **Overview**'s `Threads` — the ticket's title verbatim plus a one-clause hook |
| `decision` | the **Decisions** page — one line: the choice, its rationale, a link to its ADR or ticket |
| an open **risk** | the **Risks** page — one line: the risk, its consequence, the ticket that owns it when one exists |
| an established **process** | the **Processes** page — one line: what it is, a link to the note or ticket that documents it |
| `gotcha`, `fact`, subsystem detail | the narrowest **sub-page** that owns the topic — never a scaffold page |

Use the one-line entries and length targets in [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md). What the scaffold does not own, route in this order:

1. **Narrowest owner first.** Write it into the narrowest existing sub-page whose topic it belongs to — a subsystem sub-page before any broader one.
2. **Coherent subsystem or topic, no note?** A moment that has found its own vocabulary, decisions, or gotchas ([`CONVENTIONS.md`](CONVENTIONS.md)) **earns that sub-page**: create it with the full skeleton, at the placement CONVENTIONS fixes — flat in the project folder, or in one of its groups when the project is grouped — and link it from the Overview's notes.
3. **Overgrown note?** A note whose section has outgrown its target is **split** into the sub-page it now describes — never appended to past the target.
4. **The Overview stays an overview and index** — `Where left off`, `Threads`, and links to the project's notes; it never becomes a changelog, and no detail lives in it.

- **Refresh `Where left off`** from the ended sessions' summaries plus the remaining captures: current state and next move.
- **Add `Threads` bullets** for open `thread` notes — Ticket title verbatim plus a one-clause hook — and mark resolved ones.
- **Create the scaffold on demand** — when a project has none, this pass creates all four pages: the **Overview** (the anchor note, `Where left off` first) and the **Decisions**, **Risks**, and **Processes** pages. The anchor's concurrent create is the atomic case in [`CONCURRENCY.md`](CONCURRENCY.md); the other three pages follow it.
- **Give the Decisions page its index duty** — every ADR in the repo and every decision recorded on the board (`tandem-cli ticket list`) that the page can reach earns a line with its link; the ADR stays in the repo, never copied into the page. A decision that flips leaves the one dated reversal line ([`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md)).
- **Place the note** where [`CONVENTIONS.md`](CONVENTIONS.md) puts it: the scaffold pages at the project root, `projects/<project>/<group>/<Note>.md` in a grouped project's sub-pages, `projects/<project>/<Note>.md` in a flat one, `topics/<Topic>.md` for a cross-project concept. **Grouping** a project — one level of sub-folders for its sub-pages, once the project folder holds more than 7 curated notes — is this pass's call; the notes move with it and their links do not change (`[[Name]]` resolves by basename either way, ADR 0005).
- **`Index.md`** — re-derive each note's one-line status and rewrite it **only if the status changed**; adding to `Decisions`/`Gotchas` never churns the Index. A grouped note still gets exactly one line, naming the note and not its group.
- **Link** — every note this pass writes wikilinks out to each existing note it names. A capture carries none; adding links is this pass's job.

Write behind the boundary ([`CONCURRENCY.md`](CONCURRENCY.md)): re-read → minimal patch → write, prune on write. Never invent state the inputs do not support.

## Dedupe threads against tickets

A `thread` note usually already has a Ticket — the working agent files it the moment the thread opens. **Do not create a second one**: run `tandem-cli ticket list` first, match by title, and write the Threads bullet against the existing Ticket. Only a `thread` with no matching Ticket earns a new one (`tandem-cli ticket add`).

## Consume — archive, never delete

Move each capture it consumed to `projects/<project>/.raw/.done/<session>/<file>`, mirroring the session folder (`mv`, still hidden). Nothing is lost, and the saddlebag stays stateless — pending is simply "any file under `.raw/` not under `.done/`". The human reads the archive with "Show hidden files" in Tandem's Settings — the app's tree then draws the hidden layer, still outside search, backlinks, and wikilinks. The **daily notes are the exception**: they are folded, never moved — the pass leaves every one byte-identical, and the marker just says how far it read.

## Done when

Every pending capture is folded into the scaffold or dropped as reproducible, every daily note after the marker is read and left byte-identical, the marker is advanced exactly once, every consumed capture is archived under `.done/`, and every project with input carries all four scaffold pages with the touched notes current: a cold session could resume from them alone.
