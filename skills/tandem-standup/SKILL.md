---
name: tandem-standup
description: "Post the board's status round to the tickets' comments — one dated comment per `in-progress` ticket in a lane, with what landed, what is next and what blocks, every line naming its evidence. Reach for it when the human asks for a status round, when a sprint's close calls one, or when the user says to run the standup."
---

# Tandem standup

The standup is the board's status round: one dated comment on every `in-progress` ticket in a lane, saying what landed, what is next and what blocks, and naming the evidence for each line. The ticket's comment log is the team's minutes — the app's ticket sheet draws it and `tandem-cli ticket comment list` returns it — and the standup is the log's one regular producer.

The pass **reads the board and writes comments**. It claims nothing, judges nothing, and never invents progress.

## The trigger

The standup runs when the human asks for a status round, or when a sprint's close calls one.

Not at session start: a session that stands up on its own posts every active ticket's status in every session, and the append-only log fills with the same round N times.

## The input

**The lane.** One lane, because a session works one project. The caller names it; the default is the lane the session is working in. A lane that does not exist is an empty set, not an error: report it and stop.

**The two sets** — the lane's `in-progress` tickets, and its `ready` set read for the report:

```sh
tandem-cli ticket list --status in-progress --lane <lane>
tandem-cli ticket list --status ready --lane <lane>
```

Both come back in stored order. The `in-progress` set is who gets a comment; the `ready` set is read and reported, never commented on.

**Per ticket, these reads:**

| what | where |
| --- | --- |
| the description's appended sections — `## Progress`, `## Outcome` | the `list` output carries the description in full |
| the comment log, and the `at` of this ticket's last standup | `tandem-cli ticket comment list --id <id>` |
| the assignee | the `list` output |
| the branch, and the commits on it | `git worktree list`, `git log origin/dev..board/<slug>` |
| the PR, its merge commit and its checks | `gh pr list --head board/<slug>`, `gh pr view <n> --json state,mergedAt,mergeCommit`, `gh pr checks <n>` |

**Findings are read, never inferred from a ticket's title.** A title says what a ticket set out to do; it is never evidence of what it did. Every line of the round names the thing it was read from — a PR number, a merge commit, or the ticket section it came from. A line with no evidence to name says `no evidence on the board`.

**Matching a ticket to its branch.** The branch is `board/<slug>`, and the slug is not the ticket's id. Take the branches from `git worktree list` — `git worktree list --porcelain` prints them as `refs/heads/<name>`, and `gh --head` and `git log` want `<name>` — read each one's PR, and match on the PR naming the ticket's id in its title or body. With no PR naming it, match the slug against the ticket title. With neither, that ticket has no branch — say so; do not guess.

**A PR number is read, not repeated.** A number named in a section (`## Progress`, a comment) is read from `gh pr view <n> --json state,mergedAt,mergeCommit` before the round reports it as landed. "Landed" means the merge was seen, not that a section claimed it.

**The cutoff.** "Since the last standup" means since the `at` of this ticket's last comment authored `standup`. A ticket with no earlier standup has no cutoff: everything its branch has landed counts.

## The output

One comment per `in-progress` ticket, in this shape:

```
Standup — <YYYY-MM-DD>

Landed: <what landed, or "nothing since the last standup">
Next: <the next concrete step, or "nothing — waiting on <who>">
Blocked: <what blocks and on whom, or "nothing">
```

`<YYYY-MM-DD>` is the local date of the round. Each line is `<finding> (<evidence>)`, where the evidence names a PR number, a merge commit, or the ticket section it was read from. A line with nothing to name reads exactly `no evidence on the board`.

- **Landed** — what landed since the cutoff: a merged PR number and its merge commit, or the commits on the branch. With a cutoff and nothing new: `nothing since the last standup`. With no cutoff and nothing landed: `nothing landed yet`. With nothing at all on the board for this ticket: `no evidence on the board`.
- **Next** — the next concrete step, and where it was read: the ticket's own `## Progress` section, the PR's state (open and green → the review), or a blocker naming who it waits on. When the ticket is waiting on someone: `nothing — waiting on <who>`, where `<who>` is the assignee or the person the blocker names. With nothing to name: `no evidence on the board`.
- **Blocked** — what blocks, and on whom: the blocker, read from the description's `## Progress`, the comment log, or the PR's failing checks. With no blocker found: `nothing`. With nothing readable to say: `no evidence on the board`.

The pass never invents progress and never judges a ticket's quality. It reports what it read.

## Where the output lands

**The board** — the ticket's comment log: the sidecar `<home>/comments/<ticket-id>.jsonl`, where `<home>` is `~/.tandem` for the shipped app, `~/.tandem-dev` for a from-source build, or `$TANDEM_HOME` when that is set — `tandem-cli doctor`'s `home` check names the one in force. The app's ticket sheet draws it and `tandem-cli ticket comment list --id <id>` returns it. One comment per `in-progress` ticket; nothing else in the ticket is touched.

**During a sprint** — one more comment, on the sprint ticket (`Sprint <n> — <lane>`, per `skills/tandem-sprint/`), reporting the batch: how many of the batch landed, which ticket is at the head, and the ready set — how many are `ready` and which one comes first in stored order.

**With no sprint open** — the ready set is reported to the caller in the pass's own report, and is not posted. There is no ticket a board-wide ready report belongs to, and the board's `ready` column already shows it.

## The author rule

Every comment the pass writes is signed `--author standup` — the role name, written out, every time:

```sh
tandem-cli ticket comment add --id <id> --author standup --body -
```

- **Write the role out.** `--author standup`, never `--author self`: the CLI does not expand `self` for `--author` (it does for `--assignee` and `--origin`), so `self` lands in the log as the four letters and names nobody.
- **Never omit `--author`.** An omitted author is the empty string `""` — the unnamed author — and a minute with no speaker is a gap in the record.
- **The role's name lives in the roster** — `skills/tandem-team/SKILL.md`. That page owns the vocabulary for `--assignee` and `--author`; this skill does not restate its table. When the name changes there, it changes here.

## The seam

The standup **reports**; the dispatcher **claims**.

It reads the `ready` set, names the first ticket in stored order, and changes nothing: no status, no assignee, no description, no bikeshed note. The board skill's dispatch step (`skills/tandem-board/SKILL.md`) is the only claimant, so a ticket is never picked up twice — and the standup is safe to run beside a dispatch, because it writes comments and nothing else.

A `ready` ticket gets no comment. It has not started, and a comment on every ready ticket each round would bury the log that matters.

## The report

One line to the caller, then the ready-set line:

```
STANDUP: <lane> — <n> tickets reported, <m> blocked — <the one thing to look at>
READY: <lane> — <k> ready, first: <ticket id> <title>
```

`<the one thing to look at>` is the blocked ticket that matters most, or the first ticket reported when nothing is blocked. With no ready ticket: `READY: <lane> — 0 ready, first: none`.

No questions to the caller, and no offer to fix.

## The steps

1. Name the lane; read the `in-progress` set and the `ready` set.
2. For each `in-progress` ticket: read the description's appended sections, the comment log, the assignee, the branch, the PR and its checks; write the three lines.
3. Post one comment per ticket, `--author standup`, with the body on stdin (`--body -`).
4. With a sprint open, post the batch comment on the sprint ticket.
5. Report the two lines.

The pass ends there: it sets no status, assigns no ticket, edits no description, and files no capture.

## Done when

- Every `in-progress` ticket in the lane carries one dated standup comment with the three labelled lines, each naming its evidence, signed `standup`.
- The `ready` set is read and reported, and nothing on it is claimed or commented on.
- The comment is in the ticket's log: `tandem-cli ticket comment list --id <id>` returns it with `"author":"standup"`.
