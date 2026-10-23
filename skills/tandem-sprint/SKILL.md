---
name: tandem-sprint
description: "The SDLC kickoff — one skill that runs the whole cycle: orient, triage the board to fill `ready`, open a sprint ticket for the batch, and drive each ticket through build and review to merge. Reach for it when a coding session starts, when the human says run a sprint, or asks what the workflow is."
---

# Tandem sprint

A sprint is the unit of work: a batch of `ready` tickets driven through build → review → merge — one at a time unless the human in the session chooses parallel — recorded on one sprint ticket. Every coding session is a sprint — a batch of one is valid.

The roles are the roster's (`skills/tandem-team/SKILL.md`): the session runs the sprint, the `refiner` shapes tickets, the `builder` builds, the `reviewer` gates the merge, the human answers what only a human can.

## 1. Orient

Start the session record (`AGENTS.md` — `session start`, `session link --ticket` for what this sprint carries), then walk the memory protocol (`skills/tandem-memory/SKILL.md`): `Index.md`, the project's curated note, the open threads.

## 2. Triage

Fill `ready` before you draw from it — `tandem-cli ticket list --filter open --lane <lane> --status backlog,refined,ready`:

- **Backlog ticket too big or too undecided for one session** → dispatch the `refiner` (`agents/refiner.md`) with the ticket id. Otherwise shape it yourself into the brief — Why / What / Done when / Decided against / Open questions — and mark it `refined`.
- **Ready** = refined, no open questions, no human decision pending, and it fits one agent session. Bigger → split, file the parts, link them in the description.
- **A human-only decision** → `--assignee human`, status `refined`, one question per ticket, described below.
- **Escalation scope**: a preference, a spend, or an irreversible or outward-facing act. Everything else you decide yourself and record under **Decided against** — a question the environment can answer is not a question.

## 3. Open the sprint

The batch is the first five ready tickets in stored order — **five is a cap, not a quota**. Fewer is the batch; the session's spare capacity goes to triage. **Zero ready** → no sprint is opened: this is a triage session, and it says so in its report.

The number `<n>` is derived, never guessed: one more than the highest `n` among the lane's tickets titled `Sprint <n> — <lane>`, `1` when there are none (`--filter all`, because a closed sprint still holds its number).

```sh
sprint=$(tandem-cli ticket add --title "Sprint $n — $lane" \
  --status in-progress --assignee self --lane "$lane" --origin self | jq -r '.id')
batch=$(tandem-cli ticket list --lane "$lane" --status ready | jq -c '.tickets[:5]')
```

Take the batch before anything moves it. One comment per batch ticket naming the sprint: `ticket comment add --id <id> --author sprint --body "Sprint $n — $lane: in the batch, on sprint ticket $sprint."`

The sprint ticket's description is **written once and appended to**: `## The batch` first (ids and titles in stored order, the session that opened it), then a `## Run — <date>` per landing, then `## Close`. An append is read → add → write back:

```sh
current=$(tandem-cli ticket show --id "$sprint" | jq -r '.description // ""')
printf '%s\n\n%s\n' "$current" "$section" | tandem-cli ticket set --id "$sprint" --description -
```

## 4. Run

The human in the session picks the mode before the first dispatch — **one at a time** (the default) or **in parallel** — and the answer goes in the sprint description, one line under `## The batch`: `Mode: parallel (human choice)`.

Per ticket:

1. **Claim** — `tandem-cli ticket set --id <id> --status in-progress --assignee builder`.
2. **Build** — dispatch the `builder` subagent (`agents/builder.md`) with the ticket id and its description — `tandem-cli ticket show --id <id>`, since `list` rows are summaries without the body. The build runs in the builder's own git worktree, branched fresh from `origin/dev` — serial or parallel alike, so no two builders ever share a working tree and the host's checkout stays on `dev` until the merges land. The builder verifies, opens the PR with its `## Merge Danger` claim, and files its captures.
3. **Review** — first check the branch against its base: `git rev-list --left-right --count origin/dev...origin/<branch>`. A branch that went stale while the gate queued would cost the pass its whole read before it learns it cannot merge; behind, bring the branch up to date (`gh pr update-branch <n>`) and name the new base head in the brief. Then dispatch the `reviewer` subagent (`agents/reviewer.md`) with the ticket id and PR. It merges a two-way/low door and completes the ticket; a one-way door is escalated to the human and **leaves the batch**. An `escalated` report is accepted only after you read the ticket yourself (`tandem-cli ticket show --id <id>`): status `refined` and assignee `human` mean handed over; anything else is a failed gate — re-dispatch the reviewer, or do the handover in this session and say so in the `## Run` note.
4. **Record** — append `## Run — <date>`: the ticket, what landed, the PR.

**The gate brief carries only what the gate cannot look up.** The contract's path (`agents/reviewer.md`), the ticket id, the PR number, the base branch, and the pre-flight the gate needs — whether the branch is behind its base. At most one clause may name what is not the gate's to judge; nothing more. A PR that has been gated before is not re-briefed with a history of its passes: what each earlier pass established goes on the PR as a review comment (`gh pr comment`), and the brief points at it in one line — "earlier passes are on the PR; read the head on its own." The verdict comes from the gate's own reading of the head, never from the brief's summary of what earlier passes found — a brief that retells prior passes anchors the fresh reading it asks for, and its cost is paid again on every re-gate.

**One at a time** — the default — runs the per-ticket list above end to end before the next claim.

**In parallel** — the human's call — dispatches every batch ticket's build at once, each in its own worktree on its own branch; as each build lands, its reviewer goes in batch order. Reviews still gate every merge; parallel changes what builds at once, never what merges unreviewed.

**Why serial is the default.** The 2026-10-20 collision — nine branches sharing one checkout of this repo, `BikeFace.swift` alone touched by six — is what builds look like without worktree isolation; it stands as history, the failure isolation ends. With every build in its own worktree, merge safety no longer argues for serial: the default rests on turn budget and simplicity — one dispatch at a time keeps each turn's cost legible. Velocity is tickets per sprint, not concurrency. A ticket that escalates does not stop the sprint; the close names it.

**Budget the turns.** A turn re-reads the whole window, so a session's bill is its turn count times its context — the same rule the terse style states (`terse/TERSE.md`, Budget). One turn per dispatch, with the calls it needs in it: gather every fact the brief needs in one message — a turn that needs three facts fetches three facts in one message, and a loop over a list is one call, not N. Watch it drift: calls per turn is countable from the host's store, `~/.local/share/opencode/opencode.db` — read-only:

```sh
sqlite3 "file:$HOME/.local/share/opencode/opencode.db?mode=ro" '
WITH turns AS (SELECT session_id, COUNT(*) n FROM session_message
               WHERE type="assistant" GROUP BY session_id),
     calls AS (SELECT m.session_id, COUNT(*) n FROM session_message m,
               json_each(json_extract(m.data,"$.content")) je
               WHERE m.type="assistant" AND json_extract(je.value,"$.type")="tool"
               GROUP BY m.session_id)
SELECT ROUND(1.0*COALESCE(c.n,0)/t.n,2) FROM turns t
LEFT JOIN calls c ON c.session_id=t.session_id WHERE t.session_id="<session-id>";'
```

Below three means the next dispatch briefs the batch's shape — the reads named, the repeats cut — before it claims.

## 5. Human answers

A decision ticket's body is the channel — there is no comment field for it. The human answers in the app, from the CLI (`ticket set --description -` with the answer appended), or in chat to an agent, who appends it. The answer is its own block:

```
## Answer
B — <what was chosen, and any condition>
```

Then in one write: keep the answer, reset `assignee`, set the status the answer opens — `ready` if buildable, `backlog` if it needs re-refining, `done` if settled. Every pass starts with `ticket list --assignee human`: a human ticket never goes back to an agent unread.

## 6. Close

Append `## Close` — what landed, what slipped, what waits on the human — then `tandem-cli ticket complete --id "$sprint"`. Write the close before completing: a done sprint with an empty close is a lie the next session inherits. The close is the session's final act: nothing is dispatched after it begins, so no child session can spend the turn budget the close needs. The report names `Sprint <n+1>` as the next step, opened in a **new** session — never this one.

**A dead sprint is stale work.** An `in-progress` ticket titled `Sprint <n> — <lane>` whose opening session — the one named in its `## The batch` — has ended (its session record says `ended` — `tandem-cli session show --session <id>`, the record's two states are `running` and `ended`; when the record still says `running`, the host's session list settles it: an `outcome` **and** a stale `time.updated`, both signals needed and neither exact alone) died before it could write `## Close`, and nothing on the board separates it from a sprint still running. The next sprint's triage closes it **from what the board shows**: append a `## Close` that names the recorded `## Run` entries and any escalations left with the human, says it was written by triage from the board, then `ticket complete`. It writes nothing the dead session did not leave on the board. A sprint whose session is live is left alone — the "a claim is not a stall" rule governs the borderline.

## The board

Statuses `backlog → refined → ready → in-progress → in-review → done`, one **cycle lane** per project; the store validates the value, never the transition — these gates are the convention, not the schema. A lane must exist before `--lane` accepts it; `lane add` is a human call — ask. `--lane ""` is the default area, where untriaged capture lands. The board is not due dates, priority, labels, or a lock: a human may drag anything anywhere, and the next triage reconciles it. Long-form design lives in a linked bikeshed note, never in the ticket.
