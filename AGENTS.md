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

- **Start** — `tandem-cli session start --agent <pass> --brief -`, the brief on stdin: what this session is for and what done looks like. `<project>` defaults to the working directory's last path component; `--session` names the handle and an absent flag mints one, because the record's identity is Tandem's; `--agent` is never omitted — every session knows its pass, and a record without it cannot attribute the pass in the usage report.
- **Carry tickets** — `tandem-cli session link --ticket <id>` for each ticket the session carries. A child session is a session: it starts its own record and links what it carries.
- **End** — before your final reply: `tandem-cli session end --usage-from-harness --outcome <succeeded|failed|interrupted|unknown> --summary -`, one line on what landed. `--usage-from-harness` reads the harness session `session start` recorded from `$OPENCODE_SESSION_ID` (opencode's own database, read-only) and reports its tokens, cost, model, skill counts, and the turn budget it counted (`turns`, `calls`, both absent when the read counted neither) on the record. The usage lands on the record and nowhere else: the end writes no comment, and the record's faces are the Usage tab's ticket row and the ticket sheet's Sessions section; the comment log is where agents talk (Handoff below). An explicit `--session <id>` names another record. When the harness has no row, the verb says so and the pass ends with the plain `session end`.
- **A re-dispatched pass gets its own record.** The harness may resume an opencode session whose Tandem record the pass before it already ended; a record stamps once, so the resumed pass starts its own (`session start` with a fresh handle, carrying the same harness pointer) and writes its usage there: the usage lands on a record the ticket names, and the Usage tab's ticket row is where the ticket reads it. A record named with an explicit `--session <id>` is a deliberate target and is never continued.
- **A session that dies leaves a `running` record and nothing to reclaim.** The board holds only work, so no pass reclaims the rows of a dead session — what the session carried, its record's `tickets` names.
- **Stale work is still work.** A ticket claimed by a session that has ended is picked up by the next sprint's triage; the live-vs-ended rule and the dispatch live in `skills/tandem-sprint/SKILL.md`.

Tickets keep their own conventions: a ticket's description is written once, and a later pass appends to it rather than rewriting it. That is the convention, and the flag is `--append-description`: it adds the new text after a blank line in one transaction, or writes the bare text when the body is empty. `--description -` takes the whole body on stdin and **replaces** the stored description, so a pipe carrying only the delta wipes everything else. An empty or whitespace-only addition is a domain failure, and naming `--description` with `--append-description` is refused too: exit 1, the description left as it was. Pass `--description ""` when you mean to clear it. A session that resumes reopens its carried tickets: `ticket set --id <id> --status in-progress`.

## Handoff

The actor that finishes a ticket's work sets the next status and, where there is a next, the next assignee — instead of leaving the next move to a later pass. The names are the roster's: `skills/tandem-team/SKILL.md`. The builder hands to `in-review`; the reviewer completes on merge or escalates to `human`; the sprint session dispatches and records.

**A pass narrates its handoff in one comment.** Before it moves a ticket's status, the pass appends one comment on every ticket it carried, through `ticket comment add`, signed with its own agent name. Three short fields: **what landed** (one line), **outcome and next status** (including the escalation, when there is one), and **issues** (anything blocking or discovered, or none). The comment narrates the status move; the `ticket set --status` write stays a separate act, and no status, verb or flag backs the contract. Usage is not narrated here: it lands on the session record, and the Usage tab's ticket row and the ticket sheet's Sessions section are its faces.

**Status and assignee move together, and `human` is an assignee only while a question is open.** The actor that closes a hold records the decision in a comment and moves the pair with `ticket set`: the status the decision earns and the next owner move in the same call, and the ticket leaves the inbox with them. The inbox is the assignee alone, with no Answer-block check.

## Git

Prefer merges over rebases, and avoid force pushes and other destructive git actions unless absolutely necessary — when a force push is unavoidable, `--force-with-lease`, never bare `--force`. This applies to sub-agents too.

## Agents

The prompts for the passes live in `agents/` — `builder`, `consolidator`, `refiner`, `reviewer`, one markdown file each, named as nouns — and the procedures they run in `skills/`. A prompt that names a contract states the current one; see [ADR 0026](docs/adr/0026-agent-prompts-and-portable-setup.md). `skills/tandem-setup/` is the pass a human runs for it. The face ships inside the app bundle, `Tandem.app/Contents/Resources/agent/`; `tandem-cli setup install` materialises that copy as `<TandemHome>/agent/` and links each host into it, so the cask plus the install command is the whole setup, and no path here names one machine. On a machine working on Tandem itself the checkout is the override: the host's links resolve into the working tree, and `tandem-cli setup install --force` replaces them with the installed copy. See [ADR 0034](docs/adr/0034-agent-face-in-the-artifact.md).
