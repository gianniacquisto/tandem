# Concurrency — two harnesses, one Bikeshed

Two agent sessions can work the same project — or the same topic Note — at once. Agent writes take no lock (ADR 0004: the sidecar `flock` is app-only, last write wins), so coordination here is a rule, not mutual exclusion. The write protocol is [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md); the re-read trigger is [`READ-PROTOCOL.md`](READ-PROTOCOL.md); the sole writer of the shared notes is [`CONSOLIDATION.md`](CONSOLIDATION.md).

## Ownership — raw private, curated single-writer

- **Raw capture notes are create-only and per-session.** Every file is named for its session and moment (`projects/<project>/.raw/<session>/<utc>-<kind>-<slug>.md`), so two sessions never target the same file and a raw note is never amended. There is no shared raw file and no write boundary: the filesystem's collision suffix (`-2`, `-3`, …) is the whole race.
- **Curated notes are shared, with one writer.** `Index.md` and every note under `projects/**` are written only by the **consolidation pass** ([`CONSOLIDATION.md`](CONSOLIDATION.md)) — working agents never edit them (decision 8). A session's working memory is its context window and its raw notes, never the curated notes.

So there is no merge-back lifecycle against shared notes: a working agent only ever creates its own files.

## The write boundary

The boundary guards the **curated** notes, the shared files. Keep the write window tiny; every write is **re-read → patch → write**, never a blind file replace:

- **Immediately before amending**, re-read the note ([`READ-PROTOCOL.md`](READ-PROTOCOL.md)'s pre-write trigger) and compare it to your in-memory copy.
- If it changed, re-apply your edit as a **minimal patch onto the current content** — keep the other session's text, layer yours on top, so both survive.
- Never overwrite a region you did not re-read in the same breath.

A clash on the *same* line — two contradicting decisions — falls to [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md): newest wins, the loser leaves the one dated reversal line.

## Additive by default

A new highlight is a new bullet — naturally additive. An in-place edit touches only the one already-present line being superseded, behind the write boundary. Pruning respects the same boundary: retire a line only if it is still the line you read; if it changed, leave it (and do not force the ≲50/≲40 target).

## Bootstrap — atomic exclusive-create

A project's curated note is created with `O_CREAT|O_EXCL`. At most one writer wins the create; a loser gets `EEXIST`, re-reads, and **amends** the note (appends its section) instead of erroring. This is the one write a rule cannot lean on a re-read for, so the OS closes the race — mirroring ADR 0004's atomic-`rename`, no-journal ethos.

## Archiving a consumed raw file

Archiving is a **move** of a raw file to `projects/<project>/.raw/.done/<session>/<file>`. No boundary is needed: only the session that created it wrote it (create-only), and only the consolidation pass consumes it. Because pending is defined by **location** — any file under `.raw/` not under `.done/` — the move is the only state change, and no index can drift.

## Derived cases

All fall out of the rules above:

- **`Index` hot spot** — re-derive the project's one-line status from the note's `Where left off` and rewrite only that line, leaving the others byte-identical. A contended line resolves by re-deriving from its source, never by picking a winner.
- **Overlapping rewrite** — whoever reaches the write boundary second sees the first's output and re-applies onto current content. No winner; write order decides.
- **Stale trusted read** — no watcher, so the write boundary's re-read *is* the freshness guarantee. A session that only reads loses nothing; there is no polling ([`READ-PROTOCOL.md`](READ-PROTOCOL.md)).
- **Delete vs write** — an amend that finds its target gone does not resurrect it (the deletion was deliberate; agent delete is a permanent `rm`, ADR 0004). It first checks for a near-identical rename target and re-points the link if found; otherwise the broken links demote to plain text when touched ([`READ-PROTOCOL.md`](READ-PROTOCOL.md)).

## Completion criterion

Two concurrent sessions can work the same project — or the same topic note — on one Bikeshed, with no lock, and neither loses a highlight: the raw layer is private per session, and the curated notes have the single writer and the boundary above. The one accepted loss is a genuinely concurrent delete.
