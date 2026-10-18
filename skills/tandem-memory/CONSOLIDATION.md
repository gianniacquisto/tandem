# Consolidation — from raw capture to curated notes

The one writer of the curated Bikeshed. A distinct agent the root session **dispatches at close** (or on demand), **never** a recording sub-agent: it reads the raw capture notes every working agent filed, distills them, and archives them. The raw cadence is [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md); the layout and naming are [`CONVENTIONS.md`](CONVENTIONS.md); the write boundary is [`CONCURRENCY.md`](CONCURRENCY.md). Apply the installed `writing-for-agents` skill to every curated note.

It is the **only** writer of `Index.md` and the notes under `projects/**` — no working agent may touch them (decision 8). It never writes a raw note.

## Read the queue — hidden by default

Pending = every file under `projects/*/.raw/**` that is **not** under `.done/`. The `.raw/` subtree is hidden, so a plain glob or grep skips it:

- glob with `hidden: true`
- `rg --hidden …`
- `ls -a`

Read every pending file — the **whole** queue, not a session's slice. Group by **project** (the `.raw/` folder's parent) and by **kind** (the filename, and the section it lands in). Drop what the repo or environment reproduces cheaply — a lookup, a file's contents, a command's output. Dedupe repeats within and across sessions.

## Write the curated notes

For each project, fold its notes into the covering curated note — the project folder's note that owns the moment — with `decision` → `Decisions`, `gotcha` → `Gotchas`, `fact`/`work` → `Facts` / `Where left off`, `thread` → `Threads`. Use the skeleton, one-line entries, and length targets in [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md).

- **Refresh `Where left off`** from the session's `work` notes: current state and next move.
- **Add `Threads` bullets** for open `thread` notes — Todo title verbatim plus a one-clause hook — and mark resolved ones.
- **Create the project note** (full skeleton, `Where left off` first) when the project has none. A concurrent create is the atomic case in [`CONCURRENCY.md`](CONCURRENCY.md).
- **Create a topic note** only when [`CONVENTIONS.md`](CONVENTIONS.md)'s granularity gate fires (2+ notes must link it, or it spans projects); otherwise it stays a section.
- **`Index.md`** — re-derive the project's one-line status and rewrite it **only if the status changed**; adding to `Decisions`/`Gotchas` never churns the Index.
- **Link** — a curated note wikilinks out to every existing note it mentions by name. A raw note carries none; adding links is this pass's job.

Write behind the boundary ([`CONCURRENCY.md`](CONCURRENCY.md)): re-read → minimal patch → write, prune on write. Never invent state the raw notes do not support.

## Dedupe threads against todos

A `thread` note usually already has a Todo — the working agent files it the moment the thread opens. **Do not create a second one**: run `tandem-cli todo list` first, match by title, and write the Threads bullet against the existing Todo. Only a `thread` with no matching Todo earns a new one (`tandem-cli todo add`).

## Consume — archive, never delete

Move each raw file it consumed to `projects/<project>/.raw/.done/<session>/<file>`, mirroring the session folder (`mv`, still hidden). Nothing is lost, and the queue stays stateless — pending is simply "any file under `.raw/` not under `.done/`". The human reads the archive with "show hidden files".

## Done when

Every pending raw note is folded into a curated note or dropped as reproducible, every consumed file is archived under `.done/`, and the touched curated notes are current: a cold session could resume from them alone.
