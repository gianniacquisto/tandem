# AGENTS.md

## Memory

This project's memory is the Tandem **bikeshed** (`~/.tandem/bikeshed/`), reached through the `tandem-memory` skill. Before you read or edit project files:

- **Orient first**: load the `tandem-memory` skill, then read `Index.md`, the project's **Overview** (`projects/<project>/<project>.md`, the anchor of its fixed scaffold), and `ticket_list`. Do this before touching project files.
- **Record**: the summary at `session_end` (ADR 0027) is the work record. File a **capture** (`note_capture` with its `kind`, `project` and, when the site names one, `session`) for one thing only: a decision, gotcha or fact whose **cost of re-derivation is real** and which is not already in a session record, a ticket, an ADR or the code. The body is the `body` param: the `# <short title>` block and its one to three lines ride the string, never a pipe. The curated bikeshed notes have one writer, the consolidation pass, so never edit them.
- **Consolidate on request** — the captures keep at session close; when the human asks, `skills/tandem-consolidate/` dispatches the pass.
- **Applies to sub-agents too** — a child session receives this section like the root does, so a brief need not restate it.

The full protocol is in the skill: `SKILL.md`, `CONVENTIONS.md` (the page roles and the freshness rule), `WRITE-PROTOCOL.md`, `CONSOLIDATION.md`.

## Prose

No em dash (U+2014) in anything an agent writes: use a comma, a colon, a full stop, or a hyphen instead. The ban covers PR bodies, review replies and issue comments; ticket titles, descriptions and comments; bikeshed notes and captures; commit messages, code comments and docs. Before filing a comment or opening a PR, run `grep -n $'\u2014' <file>` on what you wrote and expect no hits. The dashes already in the tree stay: a prose change does not sweep them.

## Sessions

A session is a record, not a ticket (ADR 0027): a row of the store's `sessions` table in `tandem.db` at Tandem's home, never a ticket on the board. One record per pass, keyed by the handle Tandem mints: `session` names it, and an absent one mints it. The harness session the pass ran in is a pointer beside it, never the key, so a pass re-dispatched into a recorded harness session opens a record of its own under the same pointer (the bullet below). The five tools are `session_start`, `session_link`, `session_end`, `session_list` and `session_show`; the two states are `running` and `ended`.

**Agents call Tandem through the MCP tools**, never through the shell: `tandem-cli` is the human's and the scripts' door, documented in `skills/tandem/SKILL.md`.

- **Start**: `session_start` with `agent` set to the pass and the brief in `brief`: what this session is for and what done looks like. `project` defaults to the working directory's last path component; `session` names the handle and an absent one mints it, because the record's identity is Tandem's; `agent` is never omitted, because every session knows its pass and a record without it cannot attribute the pass and signs the usage comment its end posts `usage`.
- **Carry tickets**: `session_link` with `ticket` for each ticket the session carries. A child session is a session: it starts its own record and links what it carries.
- **End**: before your final reply: `session_end` with `outcome` and the summary in `summary`, one line on what landed. The tool reads the harness usage by default: it reads the harness session `session_start` recorded from `$OPENCODE_SESSION_ID` (opencode's own database, read-only) and reports its tokens, cost, model, skill counts, and the turn budget it counted (`turns`, `calls`, both absent when the read counted neither) on the record; it posts one usage comment, signed by the session's agent (`usage` when the record carries no agent), on every ticket the session linked, carrying `calls`, `turns` and the derived `per turn` ratio when the pair is present. An explicit `session` names another record. When the harness has no row, the tool says so and the pass ends with a plain `session_end`.
- **A re-dispatched pass gets its own record.** The harness may resume an opencode session whose Tandem record the pass before it already ended; a record stamps once, so the resumed pass starts its own (`session_start` with a fresh handle, carrying the same harness pointer) and writes its usage there: the usage lands on a record the ticket names, and its `usage` comment on the tickets it carried. A record named with an explicit `session` is a deliberate target and is never continued.
- **A session that dies leaves a `running` record and nothing to reclaim.** The board holds only work, so no pass reclaims the rows of a dead session — what the session carried, its record's `tickets` names.
- **Stale work is still work.** A ticket claimed by a session that has ended is picked up by the next sprint's triage; the live-vs-ended rule and the dispatch live in `skills/tandem-sprint/SKILL.md`.
- **Identity comes from the spawn site.** The actor a write records resolves from `TANDEM_AGENT` in the process that spawned the call (`rust/core/src/store/actor.rs`), and from `session_start`'s `agent` param when the call carries one. TAN-336's runner (`opencode run`) sets it; the host harness's own subagent dispatch does not today, so agent writes there record `human`. That is a known gap, owned by TAN-336 and named once, not a defect a pass can close from prose.

Tickets keep their own conventions: a ticket's description is written once, and a later pass appends to it rather than rewriting it. That is the convention: append by reading the ticket with `ticket_show` and passing the combined body in `ticket_set`'s `description`, after a blank line, because `description` **replaces** the stored body and a delta passed alone wipes everything else. The CLI's append flag does the same in one transaction at the scripts' door. An empty addition is a domain failure, and an empty `description` clears the body. A session that resumes reopens its carried tickets: `ticket_set` with `status` in-progress.

## Handoff

The actor that finishes a ticket's work sets the next status and, where there is a next, the next assignee — instead of leaving the next move to a later pass. The names are the roster's: `skills/tandem-team/SKILL.md`. The builder hands to `in-review`; the reviewer completes on merge or escalates to `human`; the sprint session dispatches and records.

**Status and assignee move together, and `human` is an assignee only while a question is open.** The actor that closes a hold records the decision in a comment and moves the pair with `ticket_set`: the status the decision earns and the next owner move in the same call, and the ticket leaves the inbox with them. The inbox is the assignee alone, with no Answer-block check.

## Git

Prefer merges over rebases, and avoid force pushes and other destructive git actions unless absolutely necessary — when a force push is unavoidable, `--force-with-lease`, never bare `--force`. This applies to sub-agents too.

## Agents

The prompts for the passes live in `agents/` — `builder`, `consolidator`, `refiner`, `reviewer`, one markdown file each, named as nouns — and the procedures they run in `skills/`. A prompt that names a contract states the current one; see [ADR 0026](docs/adr/0026-agent-prompts-and-portable-setup.md). `skills/tandem-setup/` is the pass a human runs for it. The face ships inside the app bundle, `Tandem.app/Contents/Resources/agent/`; `tandem-cli setup install` materialises that copy as `<TandemHome>/agent/` and links each host into it, so the cask plus the install command is the whole setup, and no path here names one machine. On a machine working on Tandem itself the checkout is the override: the host's links resolve into the working tree, and `tandem-cli setup install --force` replaces them with the installed copy. See [ADR 0034](docs/adr/0034-agent-face-in-the-artifact.md).
