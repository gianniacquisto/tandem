# Concurrency — two harnesses, one Bikeshed

OpenCode and Claude Code can work the same project — or the same topic Note — at once. Agent writes take no lock (ADR 0004: the sidecar `flock` is app-only, last write wins), so coordination here is a rule, not mutual exclusion. The write protocol is [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md); the re-read trigger is [`READ-PROTOCOL.md`](READ-PROTOCOL.md).

## Ownership — every Note shared

Every Note is shared; there are no per-session or per-agent Notes. A session's working memory is its context window, never the Bikeshed. Anything worth persisting goes straight into the covering shared Note (one concept, one Note — [`CONVENTIONS.md`](CONVENTIONS.md)), so there is no merge-back lifecycle and no private tier to collect.

## The write boundary

Keep the write window tiny; every write is **re-read → patch → write**, never a blind file replace:

- **Immediately before amending**, re-read the Note ([`READ-PROTOCOL.md`](READ-PROTOCOL.md)'s pre-write trigger) and compare it to your in-memory copy.
- If it changed, re-apply your edit as a **minimal patch onto the current content** — keep the other session's text, layer yours on top, so both survive.
- Never overwrite a region you did not re-read in the same breath.

A clash on the *same* line — two contradicting decisions — falls to [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md): newest wins, the loser leaves the one dated reversal line.

## Additive by default

A new highlight is a new bullet — naturally additive. An in-place edit touches only the one already-present line being superseded, behind the write boundary. Pruning respects the same boundary: retire a line only if it is still the line you read; if it changed, leave it (and do not force the ≲50/≲40 target).

## Bootstrap — atomic exclusive-create

A project Anchor is created with `O_CREAT|O_EXCL`. At most one session wins the create; a loser gets `EEXIST`, re-reads, and **amends** the anchor (appends its section) instead of erroring. This is the one write a rule cannot lean on a re-read for, so the OS closes the race — mirroring ADR 0004's atomic-`rename`, no-journal ethos.

## Derived cases

All fall out of the rules above:

- **`Index` hot spot** — re-derive your project's one-line status from the anchor's `Where left off` and rewrite only that line, leaving the others byte-identical. A contended line resolves by re-deriving from its source, never by picking a winner.
- **Overlapping rewrite** — whoever reaches the write boundary second sees the first's output and re-applies onto current content. No winner; write order decides.
- **Stale trusted read** — no watcher, so the write boundary's re-read *is* the freshness guarantee. A session that only reads loses nothing; there is no polling ([`READ-PROTOCOL.md`](READ-PROTOCOL.md)).
- **Delete vs write** — an amend that finds its target gone does not resurrect it (the deletion was deliberate; agent delete is a permanent `rm`, ADR 0004). It first checks for a near-identical rename target and re-points the link if found; otherwise the broken links demote to plain text when touched ([`READ-PROTOCOL.md`](READ-PROTOCOL.md)).

## Completion criterion

Two concurrent sessions can work the same project — or the same topic Note — on one Bikeshed, with no lock, and neither loses a highlight except a genuinely concurrent delete, the one loss the protocol accepts.
