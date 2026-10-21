# Consolidation — from captures to curated notes

The one writer of the curated Bikeshed: the **consolidation agent** — a distinct, fresh agent the root session **hands the saddlebag to on request** (or a human runs it on demand) — never automatically at session close — and **never** a recording sub-agent. It reads the captures every working agent filed, distills them, and archives them. How a given host names and launches it (its agent name, its command) is that host's own binding → [`HARNESS.md`](HARNESS.md). The capture cadence is [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md); the layout and naming are [`CONVENTIONS.md`](CONVENTIONS.md); the write boundary is [`CONCURRENCY.md`](CONCURRENCY.md). Apply the installed `writing-for-agents` skill to every curated note.

It is the **only** writer of `Index.md` and the notes under `projects/**` — no working agent may touch them (decision 8). It never writes a capture.

## Read the queue — hidden by default

Pending = every file under `projects/*/.raw/**` that is **not** under `.done/`. The `.raw/` subtree is hidden, so a plain glob or grep skips it:

- glob with `hidden: true`
- `rg --hidden …`
- `ls -a`

Read every pending file — the **whole** saddlebag, not a session's slice. Group by **project** (the `.raw/` folder's parent) and by **kind** (the filename, and the section it lands in). Drop what the repo or environment reproduces cheaply — a lookup, a file's contents, a command's output. Dedupe repeats within and across sessions.

## Write the curated notes

For each project, **route each capture to the narrowest note that owns it** — never pour it into the project note. The kind picks the section: `decision` → `Decisions`, `gotcha` → `Gotchas`, `fact` → `Facts`, `work` → `Where left off`, `thread` → `Threads`. Use the skeleton, one-line entries, and length targets in [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md).

Route each moment in this order:

1. **Narrowest owner first.** Write it into the narrowest existing note whose topic it belongs to — a subsystem note before the project note.
2. **Coherent subsystem or topic, no note?** A moment that has found its own vocabulary, decisions, or gotchas ([`CONVENTIONS.md`](CONVENTIONS.md)) **earns that note**: create it with the full skeleton, at the placement CONVENTIONS fixes — flat in the project folder, or in one of its groups when the project is grouped — and wikilink it from the note that covers it (its project note, or the Index `Topics`).
3. **Overgrown note?** A note whose section has outgrown its target is **split** into the subsystem note it now describes — never appended to past the target.
4. **A project note stays an overview and index** — current state plus links to its notes; it never becomes a changelog, and subsystem detail lives in the note that owns it.

- **Refresh `Where left off`** from the session's `work` notes: current state and next move.
- **Add `Threads` bullets** for open `thread` notes — Todo title verbatim plus a one-clause hook — and mark resolved ones.
- **Create the project note** (full skeleton, `Where left off` first) when the project has none. A concurrent create is the atomic case in [`CONCURRENCY.md`](CONCURRENCY.md).
- **Place the note** where [`CONVENTIONS.md`](CONVENTIONS.md) puts it: `projects/<project>/<group>/<Note>.md` in a grouped project, `projects/<project>/<Note>.md` in a flat one, `topics/<Topic>.md` for a cross-project concept. **Grouping** a project — one level of sub-folders, once it holds more than 7 curated notes — is this pass's call; the notes move with it and their links do not change (`[[Name]]` resolves by basename either way, ADR 0005).
- **`Index.md`** — re-derive each note's one-line status and rewrite it **only if the status changed**; adding to `Decisions`/`Gotchas` never churns the Index. A grouped note still gets exactly one line, naming the note and not its group.
- **Link** — every note this pass writes wikilinks out to each existing note it names. A capture carries none; adding links is this pass's job.

Write behind the boundary ([`CONCURRENCY.md`](CONCURRENCY.md)): re-read → minimal patch → write, prune on write. Never invent state the captures do not support.

## Dedupe threads against todos

A `thread` note usually already has a Todo — the working agent files it the moment the thread opens. **Do not create a second one**: run `tandem-cli todo list` first, match by title, and write the Threads bullet against the existing Todo. Only a `thread` with no matching Todo earns a new one (`tandem-cli todo add`).

## Consume — archive, never delete

Move each capture it consumed to `projects/<project>/.raw/.done/<session>/<file>`, mirroring the session folder (`mv`, still hidden). Nothing is lost, and the saddlebag stays stateless — pending is simply "any file under `.raw/` not under `.done/`". The human reads the archive with "Show hidden files" in Tandem's Settings — the app's tree then draws the hidden layer, still outside search, backlinks, and wikilinks.

## Done when

Every pending capture is folded into a curated note or dropped as reproducible, every consumed file is archived under `.done/`, and the touched curated notes are current: a cold session could resume from them alone.
