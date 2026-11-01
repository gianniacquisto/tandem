# AGENTS.md

## Memory

This project's memory is the Tandem **bikeshed** (`~/.tandem/bikeshed/`), reached through the `tandem-memory` skill. Before you read or edit project files:

- **Orient first** — load the `tandem-memory` skill, then read `Index.md`, the project's **Overview** (`projects/<project>/<project>.md`, the anchor of its fixed scaffold), and `tandem-cli ticket list`. Do this before touching project files.
- **Record** — the summary at `session end` (ADR 0027) is the work record. File a **capture** (`tandem-cli note capture --kind <decision|gotcha|fact|thread> --project tandem`) for one thing only: a decision, gotcha or fact whose **cost of re-derivation is real** and which is not already in a session record, a ticket, an ADR or the code. The capture's body rides **stdin**: `capture` takes no body flag, only `--kind`, `--project` and `--session`, so pipe it in, `printf '%s\n' "$body" | tandem-cli note capture --kind gotcha --project tandem`. The curated bikeshed notes have one writer — the consolidation pass — so never edit them.
- **Consolidate on request** — the captures keep at session close; when the human asks, `skills/tandem-consolidate/` dispatches the pass.
- **Applies to sub-agents too** — a child session receives this section like the root does, so a brief need not restate it.

The full protocol is in the skill: `SKILL.md`, `CONVENTIONS.md` (the page roles and the freshness rule), `WRITE-PROTOCOL.md`, `CONSOLIDATION.md`.

## Prose

No em dash (U+2014) in anything an agent writes: use a comma, a colon, a full stop, or a hyphen instead. The ban covers PR bodies, review replies and issue comments; ticket titles, descriptions and comments; bikeshed notes and captures; commit messages, code comments and docs. Before filing a comment or opening a PR, run `grep -n $'\u2014' <file>` on what you wrote and expect no hits. The dashes already in the tree stay: a prose change does not sweep them.

## Sessions

A session is a record, not a ticket (ADR 0027): a row of the store's `sessions` table in `tandem.db` at Tandem's home, never a ticket on the board. One record per pass, keyed by the handle Tandem mints: `--session` names it, and an absent flag mints one. The harness session the pass ran in is a pointer beside it, never the key, so a pass re-dispatched into a recorded harness session opens a record of its own under the same pointer (the bullet below). The CLI's five verbs are `session start`, `session link`, `session end`, `session list` and `session show`; the two states are `running` and `ended`.

- **Start** — `tandem-cli session start --agent <pass> --brief -`, the brief on stdin: what this session is for and what done looks like. `<project>` defaults to the working directory's last path component; `--session` names the handle and an absent flag mints one, because the record's identity is Tandem's; `--agent` is never omitted — every session knows its pass, and a record without it cannot attribute the pass and signs the usage comment its end posts `usage`.
- **Carry tickets** — `tandem-cli session link --ticket <id>` for each ticket the session carries. A child session is a session: it starts its own record and links what it carries.
- **End** — before your final reply: `tandem-cli session end --usage-from-harness --outcome <succeeded|failed|interrupted|unknown> --summary -`, one line on what landed. `--usage-from-harness` reads the harness session `session start` recorded from `$OPENCODE_SESSION_ID` (opencode's own database, read-only) and reports its tokens, cost, model and skill counts on the record; the CLI posts one usage comment — signed by the session's agent, `usage` when the record carries no agent — on every ticket the session linked. An explicit `--session <id>` names another record. When the harness has no row, the verb says so and the pass ends with the plain `session end`.
- **A re-dispatched pass gets its own record.** The harness may resume an opencode session whose Tandem record the pass before it already ended; a record stamps once, so the resumed pass starts its own (`session start` with a fresh handle, carrying the same harness pointer) and writes its usage there: the usage lands on a record the ticket names, and its `usage` comment on the tickets it carried. A record named with an explicit `--session <id>` is a deliberate target and is never continued.
- **A session that dies leaves a `running` record and nothing to reclaim.** The board holds only work, so no pass reclaims the rows of a dead session — what the session carried, its record's `tickets` names.
- **Stale work is still work.** A ticket claimed by a session that has ended is picked up by the next sprint's triage; the live-vs-ended rule and the dispatch live in `skills/tandem-sprint/SKILL.md`.

Tickets keep their own conventions: a ticket's description is written once, and a later pass appends to it rather than rewriting it. That is the convention, not the flag: `--description -` takes the whole body on stdin and **replaces** the stored description, so a pipe carrying only the delta wipes everything else, and appending means re-sending the whole text, the existing body and the addition in one write. An empty or whitespace-only body is a domain failure: exit 1, the description left as it was. Pass `--description ""` when you mean to clear it. A session that resumes reopens its carried tickets: `ticket set --id <id> --status in-progress`.

## Handoff

The actor that finishes a ticket's work sets the next status and, where there is a next, the next assignee — instead of leaving the next move to a later pass. The names are the roster's: `skills/tandem-team/SKILL.md`. The builder hands to `in-review`; the reviewer completes on merge or escalates to `human`; the sprint session dispatches and records.

**Status and assignee move together, and `human` is an assignee only while a question is open.** `ticket answer` is the one write that records the answer and closes the hold: the answering actor names the status the answer opens and the next owner, and the ticket leaves the inbox in the same call. A question answered without that write is not answered — the inbox stops asking as soon as the `## Answer` block is in the body, and the ticket's `human` tag and column are then debt the next pass owes an `answer`.

## Git

Prefer merges over rebases, and avoid force pushes and other destructive git actions unless absolutely necessary — when a force push is unavoidable, `--force-with-lease`, never bare `--force`. This applies to sub-agents too.

## Agents

The prompts for the passes live in `agents/` — `builder`, `consolidator`, `refiner`, `reviewer`, one markdown file each, named as nouns — and the procedures they run in `skills/`. A prompt that names a contract states the current one; see [ADR 0026](docs/adr/0026-agent-prompts-and-portable-setup.md). `skills/tandem-setup/` is the pass a human runs for it. The face ships inside the app bundle, `Tandem.app/Contents/Resources/agent/`; `tandem-cli setup install` materialises that copy as `<TandemHome>/agent/` and links each host into it, so the cask plus the install command is the whole setup, and no path here names one machine. On a machine working on Tandem itself the checkout is the override: the host's links resolve into the working tree, and `tandem-cli setup install --force` replaces them with the installed copy. See [ADR 0034](docs/adr/0034-agent-face-in-the-artifact.md).
