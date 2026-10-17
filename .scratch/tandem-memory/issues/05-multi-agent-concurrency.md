# 05 - Multi-agent concurrency: two harnesses, one vault

Type: grilling
Status: resolved (2026-10-17)
Blocked by: 01

## Question

OpenCode and Claude Code can work the same project — or the same shared topic notes — at the same time, and agent writes take no lock (ADR 0004: the sidecar lock is one-sided, for the app; agents write plain files, last write wins; the app's mine/theirs banner covers app buffers only, not agent-vs-agent). Settle the conventions the skill encodes so concurrent sessions do not lose each other's highlights:

1. **Ownership**: which notes are shared (project state, topic knowledge) vs per-session scratch; whether per-agent or per-session notes exist at all, and how their content merges back into shared notes.
2. **Write discipline**: re-read immediately before write; minimal-diff amendments; append-only sections. What a session does when it detects another session's changes landing mid-amendment (agents have no UI to resolve in — the convention must be a rule).
3. **Collision inventory**: the realistic collision scenarios (two sessions on one repo; two sessions on different repos touching one shared topic note; a session amending a note the other is about to rewrite) and the convention each needs.

## Answer

Settled 2026-10-17 (grilling with the human). The skill encodes the concurrency protocol below.

**1. Ownership — no scratch tier.** Every note is shared; there are no per-session or per-agent notes. A session's working memory is its context window, never the vault. Anything worth persisting is written straight into the covering shared note (01's one-concept-one-note), so there is no merge-back lifecycle to run and no scratch to garbage-collect.

**2. Coordination — lock-free.** Agents take no lock and no claim file; ADR 0004's sidecar `flock` stays app-only. Coordination is a rule, not mutual exclusion: keep the write window tiny, and every write is a re-read → patch → write, never a blind file replace. Per-file locks / claim registries stay rejected as ADR 0004 rejected them — ceremony for a lock client neither harness has.

**3. The mid-amendment rule (the ticket's centre).** Immediately before amending, re-read the file (03 §4) and compare to your in-memory copy. If it changed, re-apply your edit as a **minimal patch onto the current content**: keep the other session's text and layer yours on top, so both survive. A clash on the *same* line with two contradicting decisions falls back to 02 — newest wins, the loser leaves one dated reversal line (`**X** — reversed <date>: <reason>`). Never overwrite a region you did not re-read in the same breath.

**4. Additive by default; in place only the target line.** 02's "correct in place; never append-only" stands, clarified for concurrency: a new highlight is a new bullet (naturally additive); an in-place edit touches only the one already-present line being superseded, behind (3)'s re-read. Pruning respects the same boundary — retire a line only if it is still the line you read; if it changed, leave it (and do not force the ≲50/≲40 ceiling).

**5. Bootstrap — atomic exclusive-create.** A project anchor is created with `O_CREAT|O_EXCL`. At most one session wins the create; a loser gets `EEXIST`, re-reads, and **amends** the anchor instead (appends its section) rather than erroring. This is the one write where a rule cannot lean on a re-read, so the OS closes the race — mirroring ADR 0004's atomic-`rename`, no-journal ethos.

**6. Derived scenarios (all fall out of 1–5).**
- **`Index` hot spot** — re-derive your project's one-line status from the anchor's `Where left off` and rewrite only that line, others byte-identical; a changed line is resolved by re-deriving from its source, never by picking a winner.
- **Overlapping rewrite** — whoever reaches the write boundary second sees the first's output and adapts, re-applying onto current content. No winner; write order decides.
- **Stale trusted read** — no watcher exists, so (3)'s write-boundary re-read *is* the freshness guarantee; a session that only reads loses nothing, and there is no polling (03 §4).
- **Delete vs write** — an amend that finds its target gone does not resurrect it (the deletion was deliberate, agent delete = permanent `rm`, ADR 0004); it first checks for a near-identical rename target and re-points if found, else the broken links are demoted to plain text when touched (03).

**7. Completion criterion.** Two concurrent sessions can work the same project — or the same topic note — on one vault, with no lock, and neither loses a highlight except a genuinely concurrent delete, the one loss the protocol accepts.

Nothing new parked: the map's existing *Not yet specified* (growth/decay, link density, `writing-for-agents` coupling) and *Out of scope* already cover the rest.
