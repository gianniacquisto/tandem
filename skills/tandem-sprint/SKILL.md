---
name: tandem-sprint
description: "The sprint cycle — pick a batch of ready tickets, run them one at a time through the board, close the sprint with a retro and a named release, and hand the next sprint to a new session. Reach for it when the human asks for a sprint, or when a session opens with a sprint as its brief."
---

# Tandem sprint

A sprint is a **batch**: up to five `ready` tickets the team agrees is the work now, run one at a time through the board's own flow, closed with a retro and a release the human performs, and handed on as the next sprint in a new session.

The cycle adds no pass. The per-ticket flow is the board's dispatch → build → review → merge; the close is the existing standup and retro rules applied to the sprint's own sessions. What is new is the batch, the ticket that carries it, and the order the four moves run in.

**Two pages this skill points at rather than restates.** The roles — `sprint`, `build`, `reviewer`, `retro`, `standup` — are the roster's: `skills/tandem-team/SKILL.md`. The gates — what `ready` means, what a claim is, what an escalation looks like — are the board skill's: `skills/tandem-board/SKILL.md`.

The commands below read the JSON the CLI returns with `jq`; any JSON reader does.

## The trigger

A sprint **is requested** — by the human ("run a sprint", "start the sprint"), or by a session whose brief is a sprint.

It is never automatic. A session working a single ticket is not a sprint, and a session with nothing ready opens no sprint.

## The input

One lane's `ready` set, in **stored order** — the order the human arranged and the board draws (ADR 0006).

```sh
tandem-cli ticket list --lane <lane> --status ready
```

## The output

A **sprint ticket** in the lane, `Sprint <n> — <lane>`, carrying the batch in its description — plus one comment on each selected ticket.

## Where the output lands

The board, all of it:

- the sprint ticket, in the lane — `## The batch`, a `## Run — <date>` per landing, `## Close`;
- one comment per selected ticket, naming the sprint and the batch ticket;
- the statuses the per-ticket flow already moves.

The bikeshed gets nothing of its own — the session's one `work` capture is the standing rule (`AGENTS.md`, `skills/tandem-memory/`). The release lands wherever the human puts it.

## 1. Open — selection

### The batch

The first five of the ready set, in stored order, are the batch. **Five is a cap, not a quota.**

- **More than five ready** — the first five are the batch; the rest stay `ready` for the next sprint.
- **Fewer than five ready** — the batch is what is ready, and the session's spare capacity goes to triage (`backlog` → `refined` → `ready`) so the next sprint has a fuller set. A sprint is never padded with unrefined tickets.
- **Zero ready** — no sprint ticket is filed and no sprint is opened. The session is a triage session, and it says so in its report.

### The number

`<n>` is derived, never guessed: one more than the highest `n` among the lane's tickets whose title matches `^Sprint [0-9]+ — `, and `1` when the lane has none.

```sh
lane="Tandem"    # the lane the sprint runs in
highest=$(tandem-cli ticket list --filter all --lane "$lane" \
  | jq -r '.tickets[].title' \
  | grep -E '^Sprint [0-9]+ — ' \
  | sed -E 's/^Sprint ([0-9]+) — .*/\1/' \
  | sort -n | tail -1)
n=$(( highest + 1 ))
```

`--filter all`, because a closed sprint still holds its number.

### The sprint ticket

```sh
sprint=$(tandem-cli ticket add --title "Sprint $n — $lane" \
  --status in-progress --assignee sprint --lane "$lane" --origin sprint \
  | jq -r '.id')
```

`add` returns the ticket as JSON; `sprint` is its id. `add` puts the ticket on top of its lane, above the batch — the ready set keeps its stored order below it.

### The batch, captured once

Take the batch before anything moves it, and keep it for the whole sprint:

```sh
batch=$(tandem-cli ticket list --lane "$lane" --status ready | jq -c '.tickets[:5]')
```

### The description

The sprint ticket's description is **written once and appended to**: `## The batch` first, then a `## Run — <date>` per landing, then `## Close`.

`ticket set --description -` replaces the whole body, so an append is read → add → write back:

```sh
current=$(tandem-cli ticket list --filter all --lane "$lane" \
  | jq -r --arg id "$sprint" '.tickets[] | select(.id == $id) | .description // ""')
printf '%s\n\n%s\n' "$current" "$section" \
  | tandem-cli ticket set --id "$sprint" --description -
```

`$section` is the new block — the same command writes the first one, with `$current` empty. The first block is the batch: the ids and titles in stored order, and the session that opened the sprint.

```
## The batch

- `79E00479-…` — <title>
- `000C4186-…` — <title>
- `3C0CE343-…` — <title>
- `566772AA-…` — <title>
- `2F9D76A9-…` — <title>

Session: <session id>
```

`printf '%s' "$batch" | jq -r '.[] | "- `\(.id)` — \(.title)"'` prints those lines; `$OPENCODE_SESSION_ID` is the session id.

### The comments

One comment per selected ticket, naming the sprint and the batch ticket:

```sh
printf '%s' "$batch" | jq -r '.[].id' | while read -r id; do
  tandem-cli ticket comment add --id "$id" --author sprint \
    --body "Sprint $n — $lane: in the sprint batch, on sprint ticket $sprint."
done
```

`--author sprint` — the role, written out; the name rule is the roster's.

## 2. Run — the per-ticket flow, unchanged

In batch order, **one ticket at a time**. The flow is the board's — `skills/tandem-board/SKILL.md`, **Dispatch** — and the gate is the reviewer's — `agents/reviewer.md`. Nothing about it changes inside a sprint.

1. **Claim** — `tandem-cli ticket set --id <id> --status in-progress --assignee build`.
2. **Build** — dispatch a `build` child with the ticket's id and description and nothing more. It branches `board/<slug>` off `origin/dev`, builds, verifies, opens the PR with its `## Merge Danger` claim, and files its captures.
3. **Review** — the `reviewer` merges a two-way door and completes the ticket; a one-way door is escalated (`tandem-cli ticket set --id <id> --status refined --assignee human`) and **leaves the batch**.
4. **Record** — append a `## Run — <date>` section to the sprint ticket: the ticket, what landed, the PR.

   ```
   ## Run — 2026-10-22

   - `79E00479-…` — <title> — landed: PR #123 (merge `abc1234`).
   ```

**One at a time, never in parallel.** Nine simultaneous branches on this repo collided at merge time on 2026-10-20 — `BikeFace.swift` alone was touched by six of them — and a sprint is a sequence. The human's "increase ticket velocity" is tickets per sprint, not concurrency.

A ticket that escalates to the human does not stop the sprint: it leaves the batch, and the close names it.

## 3. Close — retro, tech debt, release

In this order.

1. **The final standup** — the standup pass posts its batch comment on the sprint ticket. The human's answer set the standup's trigger: on the human's request, with a sprint's close as its one automatic caller — so the close calls it. (`skills/tandem-standup/` — not built yet; ticket `64BD463D` builds it. Until it lands, the sprint session writes the batch comment itself, authored `standup`.)
2. **One retro per session the sprint spawned that carries no `## Retro` marker** — the standing rule applied as a catch-up; the rule and the marker are the board skill's (**Dispatch**). A child whose PR merged was already retro'd by the dispatch step. The sprint session's own retro is its own. To check a session:

   ```sh
   tandem-cli ticket list --filter all --text "<session-id>" \
     | jq -r '[.tickets[] | select(.title | startswith("Session — "))] | first | (.description // "")' \
     | grep -q '^## Retro' && echo "retro'd" || echo "needs a retro"
   ```

   A session that needs one: dispatch the `retro` agent (`agents/retro.md`) with that session id, then append the marker the board skill describes.

3. **The tech-debt tickets are the retro's** — the `backlog` tickets the retro files. Nothing else files them, and zero is a valid outcome.
4. **The release is the human's act** and the cycle does not perform it. Name it on the sprint ticket: what landed and is waiting to be released, and the release ticket the human already has (`tandem-cli ticket list --assignee human --lane "$lane"` — the open one whose title begins `Release`; today `9518E391`). A close does not open a second release ticket while one is open.
5. **Close the sprint ticket** — append `## Close`: what landed, what slipped, what waits on the human. Then `tandem-cli ticket complete --id "$sprint"`.

Write the close before completing: a `done` ticket with an empty close is a lie the next session inherits.

## 4. Next — a new session

The next sprint is `Sprint <n+1>` in a **new** session — the human's own cycle. The closing session names that as the next step in its report and does not open it; the next session derives its own `<n>` from the lane.

## Done when

- The sprint ticket carries `## The batch`, a `## Run — <date>` per landing, and `## Close`.
- Every selected ticket carries one comment naming the sprint, and the statuses the run moved.
- A retro ran for every session the sprint spawned that carried no `## Retro` marker.
- The release is named on the sprint ticket, and the human owns it.
- The sprint ticket is `done`, and the report names `Sprint <n+1>` as the next step.

## Report

The session's report says: the sprint number and its lane, the batch, what landed, what slipped, what waits on the human, and the next sprint as the next step. When nothing was ready, it says the session triaged instead.
