# Spec: the ticket model — Status, Archived, Cycle lane, and the agent's ticket surface

Type: spec
Status: implemented
Source: `.scratch/board/map.md` (wayfinder) + the grilling of 2026-10-19, ADR [0017](../../docs/adr/0017-the-ticket-model.md), `GLOSSARY.md`

Covers map tickets **1** (the store carries the three axes), **2** (the CLI ticket surface), **3** (the query in the core) and **7** (archive replaces "Clear done"). The board view, the human's inbox, GUI description editing, the migrator, lane selection, and the persona registry are named in *Out of Scope* — each needs decisions this spec does not make.

Built in PR [#38](https://github.com/gianniacquisto/tandem/pull/38), merged 2026-10-19. **The merge is not the install**: the shipping app and the `tandem-cli` on `PATH` still predate it, so until a build carrying it is installed, run the built binary (`.build/debug/tandem-cli`, `TANDEM_HOME` to choose the store) — an older writer drops the fields it does not know, which is why the migrator is gated on the install.

## Problem Statement

Tandem's todo is a title and a done state. That is enough to *capture* work and not enough to *run* it. Two things changed under that model and it did not keep up: the list is now shared between a human and several agents, and the agents — the actors doing most of the work — can only add a title and mark it finished.

So the list can't answer the questions a shared list has to answer. Where is this piece of work: not yet thought through, thought through and ready to build, being built? Who is on it? What is it, beyond one line? An agent cannot write down what it learned, cannot say "I've picked this up", and cannot file a ticket for the human to decide.

The workarounds are already visible. Titles run to a paragraph because there is nowhere else to put the detail. A design session's outcome lives in `.scratch` markdown that no agent can move, so carrying it into the next session is manual relaying. And "finished" is the only state the list can hold, so an agent that has *started* and a ticket nobody has looked at look exactly alike.

## Solution

A todo becomes a **ticket**: three independent axes — **Status** (`backlog` → `refined` → `ready` → `in-progress` → `done`), **Archived**, and **Cycle lane** — plus a **description** and an **assignee**, every one of them reachable from `tandem-cli`. Filtering becomes one query over those fields, parsed in the core, so the CLI and the app cannot disagree about what "the ready tickets" means.

That unlocks the flow the human described. A design session writes its outcome into a ticket's description and moves the ticket to `refined`, then `ready`. Another agent lists the tickets assigned to it that are `ready`, moves one to `in-progress`, builds it, and moves it to `done`. If it needs a decision it assigns the ticket to the human — which is the human's inbox. The human's own work and the agents' work stay one list, as they are today.

The compatibility constraint shapes the whole design: an older Tandem binary must keep reading the file correctly, so every field is additive, the envelope **version stays `1`**, and the legacy `done` boolean survives as a mirror of Status until a migrator can retire it.

## User Stories

**The human, at the board and the list**

1. As a Human, I want to see each todo's Status at a glance, so that I know what is waiting on me and what is already moving.
2. As a Human, I want the list grouped by Cycle lane, so that I can see the work in the area I care about without reading every title.
3. As a Human, I want to change a ticket's Status in one gesture, so that keeping the board honest costs me nothing.
4. As a Human, I want five fixed Statuses rather than columns I have to define and maintain, so that the workflow is one thing I do not have to decide about.
5. As a Human, I want to move a ticket to any Status directly — including backwards — so that the board reflects reality rather than a process I have to argue with.
6. As a Human, I want to archive a done ticket, so that finished work leaves the view without leaving the file.
7. As a Human, I want "Clear done" to stop deleting and archive instead, so that the tidy-up button can never destroy work.
8. As a Human, I want to unarchive a ticket, so that archiving is reversible like everything else.
9. As a Human, I want to filter by free text over both title and description, so that I can find a ticket by something I wrote inside it.
10. As a Human, I want to filter by Status, assignee, and Cycle lane, combined, so that "what is in progress in this area" is one query rather than a scroll.
11. As a Human, I want archived tickets excluded from the default view, so that my everyday list stays about live work.
12. As a Human, I want to see a ticket's description without leaving the list, so that opening a note is not the price of reading it.
13. As a Human, I want everything assigned to me in one view, so that the agents' requests for a decision have somewhere to land.
14. As a Human, I want my todo list to keep working through the upgrade without me migrating anything by hand, so that a schema change is never my problem.
15. As a Human, I want a ticket I completed in the old app to still read as done afterwards, so that the upgrade cannot silently reopen finished work.
16. As a Human, I want a ticket I reopened in the old app to still read as open afterwards, so that the old app's decisions are honoured while it is still around.
17. As a Human, I want the file to stay legible and hand-editable, so that I can still fix a list by editing JSON.
18. As a Human, I want an archived ticket to stay in the file rather than vanish, so that archiving is not a delete wearing a friendlier name.
19. As a Human, I want the list's stored order preserved, so that the order I arranged is the order I see.
20. As a Human, I want one definition of "the ready tickets" shared by the app and the CLI, so that my board and my agent's query never disagree.

**The agent, through the CLI**

21. As an Agent, I want to list tickets filtered by Status, so that I can pick up work that is ready to build.
22. As an Agent, I want to list tickets filtered by assignee, so that I can find what is mine and what is waiting on the human.
23. As an Agent, I want to list tickets filtered by Cycle lane, so that I can stay inside the area I was asked to work on.
24. As an Agent, I want to list tickets by free text across title and description, so that I can find a ticket from something I remember writing in it.
25. As an Agent, I want `list` to carry every field of a ticket verbatim, so that one shape describes a ticket everywhere and I need no second call to see its status or description.
26. As an Agent, I want `--filter open|done|all` to keep working exactly as documented, so that no existing agent instruction breaks.
27. As an Agent, I want to add a ticket with a title alone, so that capturing a follow-up stays one call.
28. As an Agent, I want to add a ticket already in a Status and a Cycle lane, so that filing it correctly does not take three calls.
29. As an Agent, I want to assign a new ticket to myself, so that taking work is one word rather than an id I have to look up.
30. As an Agent, I want to set a ticket's Status, so that I can say I have picked it up and that I have finished it.
31. As an Agent, I want to complete a ticket, so that the short path to done stays short.
32. As an Agent, I want to reopen a ticket, so that a wrong completion is correctable without the app.
33. As an Agent, I want to write a ticket's description from stdin, so that a multi-line markdown summary from a session lands intact rather than through shell quoting.
34. As an Agent, I want to rename a ticket by setting its title, so that a title that has drifted can be fixed.
35. As an Agent, I want to assign a ticket to the human, so that asking for a decision is an assignable action rather than a message into the void.
36. As an Agent, I want to clear an assignee or a description, so that a ticket can go back to unowned and unexplained.
37. As an Agent, I want to move a ticket between Cycle lanes and reorder it within one, so that the list's shape stays manageable by agents and not only by mouse.
38. As an Agent, I want to archive finished work in bulk, so that a long list of done tickets leaves the view in one call.
39. As an Agent, I want an invalid Status value rejected by name with the values that would have worked, so that I can retry in one turn.
40. As an Agent, I want a bad id to come back with the current open list, so that a stale id costs me one turn, not a conversation.
41. As an Agent, I want an empty result to be a success, so that "nothing to do" never reads as a failure.
42. As an Agent, I want to change one field without touching the others, so that setting a Status cannot silently rewrite a description.
43. As an Agent, I want a Status change to be a single write, so that two agents working at once cannot interleave into a half-edited ticket.

**Both, and the system**

44. As an Agent, I want concurrent access — several agents plus the app — to never corrupt the file, so that the shared list stays trustworthy.
45. As a Human, I want the app to refresh when an agent changes a ticket, so that the board reflects work as it happens.
46. As a Human, I want the store to keep its `version: 1` envelope, so that an older binary reading the file cannot mistake it for corrupt and move my real list aside.
47. As a Human, I want a Cycle lane rename to not break an older reader, so that the concept can change its name without the file changing its shape.
48. As a Human, I want no due dates, priorities, labels or comments to appear as a side effect of this change, so that the ticket stays as small as the workflow needs.

## Implementation Decisions

### The store

- `Todo` gains four stored fields, all additive, all written explicitly (the existing encoder already writes optional fields as JSON `null`): `status`, `archived`, `description`, `assignee`. The lane is the renamed `section`.
- **The stored key for a Cycle lane stays `section`.** The code property becomes `lane` with the custom `CodingKeys` mapping `lane` → `section`, so the domain speaks the new word while the file keeps the old shape. A key rename is the migrator's job, not this change's.
- `status` is a closed enum of five values — `backlog`, `refined`, `ready`, `in-progress`, `done` — decoded `decodeIfPresent ?? .backlog`, so a file written before this change reads as `backlog`, and every closure that still writes `status` explicitly is legible by hand.
- `archived` is a boolean, absent meaning `false`.
- `description` is a markdown string; leading and trailing whitespace is trimmed and an empty result means nil, matching the existing rule for a note link.
- `assignee` is a plain string: absent means unassigned, the literal `human` is the one human, and anything else is an agent name. Nothing validates the name — the persona registry that would is out of scope — and a name is stored as given, lowercased only for the reserved word.
- The envelope **version does not move**: every field is additive, so an older reader ignores what it does not know (ADR [0006](../../docs/adr/0006-stored-order-and-sections.md)).

### The legacy `done` mirror

- `done` stays on disk as a mirror of `status == done`, because an older reader treats a missing required key as corruption — the same outcome as an unknown version, and the 2026-10-17 way a real list was moved aside (ADR 0006).
- **`done` wins on read; a write reconciles.** Only an older writer can make the two disagree (a newer writer sets both), and an older writer's completion or reopen is a real instruction from a real click. So: reading a ticket whose `done` and `status` disagree resolves to `done`, and the next write persists `status = done ? .done : (status ?? .backlog)`.
- The rule lives in the store's existing single normalization point, applied on both paths — in memory on read, persisted on write — so both faces see the same ticket and the file self-heals.
- **Completing erases the stage it came from.** A reopen therefore lands in `backlog`; a caller that means a particular stage sets it, because status is just another field. (The alternative — storing the previous stage — buys a nicer reopen at the price of a second field with no other use.)

### Operations

- One operation sets fields, and it is one write: a caller names the fields it changes and the rest are untouched. The "unchanged" and "cleared" cases must be distinguishable in the type — a nil-means-clear signature cannot express "leave the description alone" — and the spelling is the implementer's choice.
- `complete` sets Status to `done` (so the mirror is written too); `reopen` sets it to `backlog`. Both stay idempotent, and both keep today's behaviour of not reordering the list.
- The existing clear-done operation becomes **archive-done**: it sets `archived` on every done ticket and returns how many it archived, replacing the destructive behaviour. Single-item delete stays destructive, and stays GUI-only (the agent surface has no delete).
- The existing lane move — today `move(id, toSection:at:)`, reorder-in-place and move-between in one call — keeps its shape and takes the renamed lane. Its ordering arithmetic stays in the core, not the view.
- Errors keep today's convention: name the failed value, say what would have worked, and carry the current open list when the failure is an unknown id.

### The query

- One value type in the core describes a view over the tickets, and both faces use it: the CLI's flags and the app's filter box are two syntaxes over one parser, so they cannot drift (the rule that put drag arithmetic in the core).
- Its axes: free text over **title and description** (case-insensitive substring), a set of Statuses, an assignee (a name, or unassigned), a Cycle lane, and archived. The archive axis is three-valued — excluded (the default), included, only — because "hide finished work" and "show me only the archive" are both real.
- The legacy `Filter` of three values survives as sugar over the axes: `open` is not done and not archived, `done` is done and not archived, `all` is everything. `--filter open|done|all` is a shipped, documented contract and keeps its default of `open`.
- A Status value that is not one of the five is a domain error naming the value and listing the five — not a usage error, matching today's rule that the domain rejects a missing flag value rather than the parser.
- An empty result is a successful empty list.

### The CLI

- `todo list` gains the query's filters, and keeps `--filter`. A repeated axis is expressed comma-separated (`--status ready,in-progress`) rather than by repeating the flag, because the existing flag grammar takes the last value of a repeated flag and would silently drop the earlier ones.
- `todo set --id` sets `--title`, `--status`, `--assignee`, and `--description`, any subset, in one write. `--description -` reads the body from stdin, the precedent `note capture` set, because a description is multi-line markdown and a shell argument is a quoting trap. An empty value clears the field.
- `todo add` gains optional `--status`, `--assignee` and `--lane`; a title alone stays the default. It does **not** gain a description: a ticket is born from a capture, and its description arrives from the session that refined it.
- `todo move --id --lane [--index]` carries the lane and position move to the agent, mirroring the core's split between setting fields and moving a ticket. This absorbs the standing todo *move and rename todos from cli*.
- `--assignee self` resolves to the agent's own name from the environment, so claiming work needs no lookup. There is no implicit self-assignment anywhere: `add` never assigns, because agents routinely file tickets *for* the human.
- Results stay schema-verbatim — the whole ticket, every field — so the new fields reach agents with no encoding work. If `list` payload ever becomes a problem, the fix is a `--brief` flag, not a second default shape.
- The CLI performs no validation of its own: it hands raw values to the core, which parses and rejects them. This keeps the adapter decision-free and the errors in one place.

## Testing Decisions

- **A good test here exercises external behaviour only**: what an operation returns, what error text comes back, and what the file contains afterwards — never internal locking mechanics or parse structure. This is the core's existing standard (ADR [0009](../../docs/adr/0009-cli-agent-face.md)).
- **One seam, unchanged: `TandemCore`.** It is the only module with real logic; the app and the CLI are thin adapters with no test target, and this change adds nothing that belongs outside the core. Everything above — the mirror, the reconciliation, the query, the errors — is core logic and lands at that seam. The CLI's flag lexer stays decision-free by design, so leaving it untested costs nothing; the GUI stays manual.
- **The store and its operations**, against a real temp-directory store, extending the existing suites: a ticket round-trips through `status`, `archived`, `description`, `assignee` and `lane`; a file written before this change reads with `backlog`; the envelope is still `version: 1`; the stored key on disk is still `section` while the code says lane; a lane rename by a stale writer cannot lose a ticket.
- **The mirror and the reconciliation**, with hand-written files — the prior art is the recovery suite, which already proves behaviour against malformed and older files: `done` true with `status` absent reads as done; `done` true with `status: backlog` reads as done; the next write persists `status: done`; `done` false with `status: done` reads as open and reconciles to `backlog`.
- **The query**, as a pure function of a ticket list — the cheapest and most complete seam in the change: each axis alone, the axes combined, text matching inside a description, the archive tri-state, the legacy `Filter` values meaning what they mean today, an invalid status rejected by name, and an empty result succeeding.
- **The agent surface's behaviour** is proved at the core operations, not through the binary: setting one field leaves the others, a status change is one write, `complete` and `reopen` are idempotent, archive-done archives rather than deletes and reports the count.
- **Concurrency** already has prior art (racing writers, a bounded-retry lock) and needs one addition: two agents setting different fields of one ticket never produce a half-edited ticket.
- **Manual checks**: one smoke per CLI verb, since the adapter is untested by design (the precedent is the `doctor` and MCP handshake smokes); the GUI by hand, judged by the human.

## Out of Scope

- **The board view** — columns by Status, rows by Cycle lane, tickets dragged between columns. The model had to land first, and the view owed a decision: whether the board replaces the list's filter control or sits beside it. **Built on 2026-10-20 as a third tab beside the list** — `.scratch/board/board-spec.md`.
- **The human's inbox** — the view of everything assigned to `human`. It is a view over the query this spec defines, and it wants its own decisions about how a request for a decision is surfaced.
- **Description editing in the GUI** — where the body is shown, how it is edited, whether it is searched live.
- **The migrator** — retiring the `done` mirror and renaming the stored `section` key. Its decisions are not made here, and its trigger is the *install* of the shipping build rather than the merge, because an older binary reading the file is the whole hazard (ADR 0017).
- **Cycle lane selection** — a cached default lane for new tickets, so a session of filing does not repeat the lane.
- **The persona registry** — agent identities defined by an `agent.md`, specialists with their own memory, the human as the escalation target. It is pinned as its own wayfinder; until it exists, `assignee` is a plain string with no validation.
- **Due dates, priority, labels/tags, checklists, comments, attachments** — a recorded no. The original wayfinder out-scoped due dates and priorities, so reversing that is its own decision.
- **Ticket hierarchy** — an "epic" as a parent ticket is a tree, not a board row.
- **User-defined workflow columns** — the five Statuses are fixed, and the Cycle lane is already the user's own naming axis.
- **Permissions and auth** — single user, no auth stays an invariant, so any writer may set any assignee, including assigning work to the human.

## Further Notes

- **Vocabulary.** `GLOSSARY.md` is the authority: the entity is a **Todo**, its axes are **Status**, **Archived** and **Cycle lane**, and the store's word for a lane is still `section`. "Ticket" — and "board", "card", "column" — are the informal register this epic is discussed in; the spec uses the glossary's words so the code and the record agree. If "Ticket" should become a glossary term in its own right, that is a small follow-up to ADR 0017.
- **A correction made while writing this spec.** An earlier draft of ADR 0017 implied a reopen returns a ticket to the stage it was in. With `done` as a Status value that stage is erased by completion, so both the ADR and the map now say plainly: a reopen lands in `backlog`.
- **Why this spec stops where it does.** The map charted ten tickets and this spec charters four of them. The rest are views and follow-ups over the model, and the repo's convention is to write a spec from resolved decisions — so they get their own once the store's shape is real and the open calls (the filter control's fate, the migrator's mechanism) have been made.
- **The tracker.** This repo's tracker is local markdown (map, spec, tickets), and since the ticket epic's first decision, the work items themselves live on the Tandem board; the spec is the record. No hosted issue tracker is in use here, so no `ready-for-agent` label exists to apply beyond the status line above.
