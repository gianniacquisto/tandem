# Spec: Tandem memory — agent memory and a wiki of all work on the bikeshed

Type: spec
Status: ready-for-agent
Source: `.scratch/tandem-memory/map.md` + tickets 01–05 (each ticket's Answer is normative for its area), ADR 0004 (vault protocol) and ADR 0005 (wikilink rename rewriting), branch `prototype/tandem-memory-layout` (layout sketch; verdict accepted 2026-10-17), branch `research/tandem-memory-model-invocation` (harness mechanics), `GLOSSARY.md` (Bikeshed / Memory note / Human note / Anchor note / Orientation note / Wikilink / Backlink / Threads), and the `writing-for-agents` skill (+ `SKILL-MECHANICS.md`) for the writing discipline the skill is built on.

## Problem Statement

Every agent session starts cold. A local coding agent (OpenCode or Claude Code) opens a repo with no memory of the last session: the decision that was made and why, the gotcha that cost an hour, the fact that took a dozen dead ends to derive — all gone, to be rediscovered or repeated. A long session that would keep some of it instead thrashes against its context limit. Tandem already has a shared notes surface — the bikeshed, a plain folder of `[[wikilinked]]` markdown the app renders live — but nothing tells an agent what to write there, where it goes, or how a new session should pick it up. So the folder is a place to keep notes, not the backbone of memory, and each session pays again for what the last one already learned.

## Solution

One **model-invoked** skill, `tandem-memory`, stored in this repo at `skills/tandem-memory/` and linked globally by a companion run-once setup skill, so every session on this Mac — in any repo — finds it on its own, with no human typing its name. The memory skill turns the bikeshed into memory: at session start the agent orients from the vault's `Index.md` and the matching project anchor note and picks up the open threads; as it works it records a **memory note** at each qualifying moment — a non-obvious decision, a gotcha, an expensive-to-derive fact, a finished unit of work, an opened or closed thread — so the next session can pick up where this one left off or find information related to what it is doing. The notes accumulate into a wiki of all the work, navigable by wikilinks and backlinks in the Tandem app, while the active work lives in Tandem's todos. Nothing new to run and no service to hold: the harness fires the skill, the agent writes plain files, and the app watches them live.

## User Stories

### Orientation — picking up where the work left off

1. As an Agent, I want the skill to fire on my own at the start of a session in any repo on this Mac, so that I orient from the vault without the human typing its name.
2. As an Agent, I want to open `Index.md` first, so that in one small read I learn every project the vault has memory for and where the topics live.
3. As an Agent, I want to match my task to a project line and read that project's anchor note, so that I pick up that project's current state and next move.
4. As an Agent, I want to read `Where left off` in full and scan the other sections by heading, pulling detail only when my task touches it, so that orientation stays cheap.
5. As an Agent, I want to stop orienting as soon as I can state the project's current state, its next move, and which threads are open, so that I don't over-read.
6. As an Agent, I want to check Tandem's open todos (MCP `list`) as the last orientation step, so that I know the active threads and where the work actually stands.
7. As an Agent, I want to follow the anchor's wikilinks outward only as the task needs them, so that I reach the topic notes without loading the whole vault.
8. As an Agent, when no project matches my task, I want to read only the Index's topics list and grep the vault, so that I reuse existing knowledge before starting fresh.
9. As an Agent, I want the app's Backlinks dock to be the human's one-hop neighbourhood while my equivalent is `grep -r '[[Name]]'`, so that the same graph is navigable from both sides.
10. As an Agent, I want topic notes and human root notes to be reached on demand, not read at orientation, so that the cold-start read stays bounded.

### Recording — the notes are the compression

11. As an Agent, I want the skill's body to stay in my context after it fires, so that the record-as-you-go discipline is in front of me all session, not only at the start.
12. As an Agent, I want to write one small edit at each qualifying moment rather than batching at session end, so that nothing is lost when a session ends or compacts.
13. As an Agent, I want a memory note to record a non-obvious decision with a one-line rationale, so that the next session does not re-litigate a settled choice.
14. As an Agent, I want a memory note to record a gotcha or an expensive-to-derive fact, so that the next session does not pay to rediscover it.
15. As an Agent, I want to qualify content by **cost of derivation** (effort to create, not wall-clock), omitting anything the repo or environment reproduces, so that notes stay dense and cheap to read.
16. As an Agent, I want to record when a significant unit of work is finished, so that progress is visible mid-session, not only at the end.
17. As an Agent, I want to amend the covering note instead of creating a parallel one, so that there is one source of truth per concept.
18. As an Agent, I want to create a new note only when 2+ notes must link to it or it spans projects, so that the vault does not sprawl.
19. As an Agent, I want to correct a fact in place and leave one dated reversal line when a decision flips, so that stale decisions do not sediment as live ones.
20. As an Agent, I want every note to follow a fixed skeleton (`Where left off` / `Decisions` / `Gotchas` / `Facts` / `Threads`) with length targets, so that every note reads the same way and a section outgrowing its target signals a note earned.
21. As an Agent, I want to prune on write — adding a line invites retiring one — so that notes do not grow without bound.
22. As an Agent, I want a closing pass at session end or when context nears exhaustion that refreshes `Where left off`, so that the vault is resumable by a cold session.
23. As an Agent, I want a write to be done when a cold session could resume from the vault alone, so that I know when to stop writing.

### Threads and todos

24. As an Agent, I want an open thread to be a Tandem todo (MCP `add`) plus a bullet in the owning note's `Threads` section leading with the todo title verbatim, so that the authoritative list stays in the todos and the note carries only the context a title cannot.
25. As an Agent, I want a closed thread to be MCP `complete` plus the note's bullet becoming a dated resolved line, so that progress is recorded in both places.
26. As an Agent, I want threads to be plain text, never wikilinks, so that the link graph stays clean and todo titles do not masquerade as notes.
27. As an Agent, I want the vault's active work to live in Tandem's todos and its durable memory to live in notes, so that the two stores stay separate and each is authoritative for its own thing.

### Layout and naming

28. As an Agent, I want exactly two folder kinds — `projects/` and `topics/` — so that placement is predictable and I never invent a folder.
29. As an Agent, I want a project's memory to start as a single anchor note and become a folder only when a second note is earned, so that the common case is one file.
30. As an Agent, I want Title Case note names, basenames unique vault-wide, and no `#` or `|`, so that notes match human ones and every name is wikilinkable.
31. As an Agent, I want no agent prefix on note names, so that there is no separate agent namespace and no name-based ownership to explain.
32. As an Agent, I want to extend a human note that already covers a topic instead of creating a parallel agent note, so that one concept stays one note.
33. As an Agent, I want `Index.md` to hold pointers and one-line status only, re-derived from the project note, so that it cannot drift into a second source of truth.
34. As an Agent, I want to update a project's `Index` line in the same session I update that note — and only when the status actually changed — so that the index is current without churn.

### Retrieval, staleness, freshness

35. As an Agent, I want a retrieval preference order — curated wikilinks, then backlink grep, then keyword grep — so that I find related information reliably and cheaply.
36. As an Agent, I want `[[Name]]` to resolve as a case-insensitive filename match, not a search, so that resolution is deterministic and duplicates are visible, fixable broken links.
37. As an Agent, I want the repo to be ground truth: when a note contradicts it I correct the note in place (a dated line only if a decision flipped), so that the next session is not misled.
38. As an Agent, I want to repair a broken wikilink opportunistically when I touch the note — never by sweep or by guessing a new target — so that link rot heals without mass edits.
39. As an Agent, I want to trust my session's reads and re-read only immediately before writing or on an observed external change, so that I don't burn context re-reading.
40. As an Agent, I want human notes read as context but not as state of record, so that I do not treat aspirational or stale human prose as a settled decision.

### Concurrency — two harnesses, one vault

41. As an Agent sharing the vault with another session, I want to re-read a note immediately before amending it and apply my edit as a minimal patch onto the current content, so that neither session's highlights are lost.
42. As an Agent, I want new highlights to be additive bullets with in-place edits limited to the single line being superseded, so that concurrent writes rarely overlap at all.
43. As an Agent, I want a project anchor bootstrapped with an atomic exclusive-create whose loser amends instead of erroring, so that two cold starts cannot destroy a full note.
44. As an Agent, I want a genuinely concurrent delete respected — re-pointing only if it was a rename — so that I never resurrect a note the human or another agent deliberately removed.
45. As an Agent, I want `Index` status lines resolved by re-deriving them from the anchor, not by picking a winner, so that the structurally busiest file has no ambiguous state.

### Install, scope, and the harnesses

46. As a Human, I want a run-once setup skill I can invoke by name, so that linking the memory skill globally is one guided step rather than commands I have to get exactly right.
47. As a Human, I want the setup skill to detect what is already installed and confirm before it acts, so that re-running it is safe and I am never surprised by what it changes.
48. As a Human, I want the setup skill to link the repo's skills into both harnesses' global roots and verify they resolve, so that I know the install actually worked.
49. As a Human, I want the skills stored once in this repo and linked, not copied, so that updating the repo updates every session's copy.
50. As an Agent, I want the memory skill to hot-reload when the repo changes, so that a fix lands without restarting my session.
51. As a Human, I want the memory skill global to every repo on this Mac, so that the vault is one wiki of all my work rather than memory siloed per repo.
52. As an Agent, I want the same memory skill to work in OpenCode and Claude Code from one source, so that I do not maintain two memories.

## Implementation Decisions

### The artifact: two skills, disclosed

The deliverable is **two skills** under `skills/` in this repo: the memory skill **`tandem-memory`** (model-invoked, the body of this spec) and a run-once **`setup-tandem-memory`** (user-invoked) that performs the global link step. The memory skill is made of a lean `SKILL.md` plus companion convention docs reached by pointer. Because a fired skill's body stays in context for the rest of the session (both harnesses), `SKILL.md` carries only what every branch needs — the two steps (orient, record), their completion criteria, and pointers — and pushes the reference detail into companions loaded on demand, per `writing-for-agents` progressive disclosure.

- `SKILL.md` — frontmatter (see below) + the scope/trigger framing, the **orientation** step, the **record** step (its four qualifying moments and the closing pass), their completion criteria, and pointers to the companions.
- `CONVENTIONS.md` — the vault layout: the two folder kinds, naming, the `Index.md` + anchor entry point, granularity, and the wikilink discipline (ticket 01).
- `WRITE-PROTOCOL.md` — when/what to write, update-vs-create, the reversal-line rule, threads-as-todos, the fixed skeleton, the length targets, and pruning (ticket 02).
- `READ-PROTOCOL.md` — the orientation walk, the retrieval order, staleness/repair, freshness, and human-note authority (ticket 03).
- `CONCURRENCY.md` — ownership (no scratch tier), lock-free coordination, the write boundary, the atomic anchor bootstrap, and the derived collision scenarios (ticket 05).

The exact file set is the implementation session's to trim; the decisions it must not lose are the bodies of tickets 01–05. The setup skill is a single lean `SKILL.md` with no companions.

### Invocation and frontmatter

Model-invocation is the default in both harnesses; the frontmatter is `name: tandem-memory` plus a `description`, and **neither opt-out flag** is set (`disable-model-invocation` / `metadata.opencode/autoinvoke: "false"` would hide it from the model). The `description` is the only always-loaded surface, so it is the skill's top-level context pointer: scope-first, then the two trigger branches, under ~1,024 characters with the key use case first. The intended wording (from ticket 04's findings; refine on the firing smoke pass):

> Memory and the wiki of work, backed by the Tandem bikeshed (`~/.tandem/bikeshed/`) — a plain folder of markdown notes. Reach this at the **start of a session in any project on this Mac, before reading or editing project files**, to orient: read `Index.md`, the matching project anchor, and Tandem's open threads (MCP `list`). Reach it again **as you work** to record — write a memory note when you make a non-obvious decision, hit a gotcha, finish a significant unit of work, or open/close a thread; don't batch to session end.

The second skill, `setup-tandem-memory`, is **user-invoked** — `disable-model-invocation: true`, with a human-facing one-line `description` and zero context load — because it runs once by hand and no other skill needs to reach it on its own (the `SKILL-MECHANICS` rule).

### Scope marker — settled

The skill is scoped to **every local session on this Mac**, not to Tandem-tracked repos. That is the map's destination ("sessions in any repo on this Mac") and it is what makes the vault a wiki of *all* the work rather than per-repo memory. This dissolves ticket 04's one open variable — no "Tandem-tracked" marker is needed, because the whole machine is the scope. The cost is the always-loaded description firing on unrelated sessions; the mitigation is that orientation is cheap and bounded (read `Index.md`; stop), and the record branch fires only on its named moments.

### Install / link — a run-once setup skill

The install is a **user-invoked setup skill**, `setup-tandem-memory` (settled 2026-10-17) — not a script and not an `INSTALL.md`. It is a prompt-driven, exploratory skill in the mould of `setup-matt-pocock-skills`: it detects the machine's current state, confirms with the human, then acts. Its job is the global link step — symlink the repo's `skills/tandem-memory/` (and itself) into `~/.agents/skills/` (OpenCode, and this machine's existing convention) and `~/.claude/skills/` (Claude Code) — idempotently (`ln -sfn`), with a verification pass that the skill resolves in each harness and an unlink path for removal. Because both harnesses follow symlinks and hot-reload, a repo update needs no reinstall; re-running the setup is only needed on a re-clone.

**Bootstrap.** For the first run on a machine with no links yet, either the human runs the setup skill from a session working in this repo (the repo must expose its `skills/` directory project-locally to both harnesses — OpenCode via a `skills` entry in the repo's config, Claude Code via a project skills path; the exact mechanism is the implementation session's), or runs the two `ln` commands directly as the documented fallback. Once linked, later runs fire from anywhere.

### Vault, naming, and vocabulary

The memory surface is the existing bikeshed — a plain `.md` folder at `~/.tandem/bikeshed/`, filename = note identity — governed by ADR 0004 (agents write plain files and take no lock; the sidecar `flock` is app-only; agent deletion is a permanent `rm`) and ADR 0005 (wikilinks resolve live by case-insensitive basename; a broken link is a visible broken link, never a silent tie-break). The skill uses the glossary's terms exactly: **Bikeshed**, **Note**, **Memory note**, **Human note**, **Anchor note**, **Orientation note**, **Wikilink**, **Backlink**, **Todo**, **Human**, **Agent**, **MCP server**.

### The two steps the skill encodes

**Orient** (read protocol, ticket 03): read `Index.md` → match the task to a project line → read that anchor (`Where left off` in full; other sections scanned by heading and pulled only on relevance) → follow only needed wikilinks → check MCP `list` for open threads. Stop at state + next move + open threads. No matching project → read the Index's `topics/` list, then grep the vault; the anchor is bootstrap-created at session end.

**Record** (write protocol, ticket 02): an eager, event-driven write at each qualifying moment — a non-obvious decision (+ rationale), a gotcha or hard-won fact, a finished unit of work, an opened/closed thread — plus a closing pass refreshing `Where left off` (and the `Index` line only on a status change). Qualification is **cost of derivation**; correct in place with one dated reversal line for a flipped decision; amend the covering note, create a note only when 01's rule fires; reuse the fixed skeleton with ≲50-line anchor / ≲40-line topic targets and prune on write.

### Concurrency (ticket 05)

No scratch tier — every note is shared and a session's working memory is its context window. Coordination is **lock-free**: no lock, no claim file, ADR 0004's lock stays app-only. The rule that replaces the lock is the **write boundary** — immediately before amending, re-read the note and apply the edit as a **minimal patch** onto the current content, keeping the other session's text; a same-line contradiction falls to the write protocol's dated reversal. New highlights are additive; in-place edits touch only the one superseded line. A project anchor is created with an atomic `O_CREAT|O_EXCL`; a loser gets `EEXIST`, re-reads, and **amends** instead. The derived cases — `Index` lines re-derived from the anchor, overlapping rewrites ordered by who hits the boundary second, a stale read self-correcting at the next write, and a deliberate delete respected — all follow.

### `writing-for-agents` coupling — settled

The skill **points at** the installed `writing-for-agents` skill (and its `SKILL-MECHANICS.md`) for the note-writing discipline, rather than inlining a distilled copy. Rationale: single source of truth for the rules, and the skill is already installed globally on this Mac; an inlined copy would drift. The reference is a context pointer inside the record branch — the discipline is applied while writing notes, not loaded at orientation.

## Testing Decisions

The skill is agent-facing prose plus a run-once setup skill; there is **no automated seam**, by decision (2026-10-17). A good test here would assert external behavior, but the skill's behavior is the model firing it, which no unit test observes. The candidates were considered and set aside: an install helper's symlink side effects are testable, but the human chose a user-invoked setup skill over a script, so an automated install test is deferred until a script is earned. This matches the repo's existing stance that only `TandemCore` is unit-tested while the GUI is verified manually.

- **What is tested:** nothing automatically. **Verification is a manual smoke pass**, documented in the spec rather than encoded as a suite.
- **The manual pass (the release gate):** after creating the skills and linking them, in **both** OpenCode and Claude Code — (a) start a session in a repo unrelated to Tandem and confirm branch A fires and orients (opens `Index.md`, reads a project anchor, runs MCP `list`) without being named; (b) make a non-obvious decision or hit a gotcha and confirm branch B fires and a memory note lands in `~/.tandem/bikeshed/` with the right skeleton and placement; (c) edit `SKILL.md` in the repo and confirm the change hot-reloads into a live session without a restart; (d) run `setup-tandem-memory` on a machine with no links and confirm it links both harnesses, verifies the skills resolve, and is idempotent on re-run.
- **No fixture vault, no network, no `~/.tandem` writes** are added to the test targets; the core suite is untouched.
- **Prior art for the manual pass:** the v2 UI work was verified by a manual pass against a prototype; the core suite's temp-dir tests are the prior art for the deferred install-script test if one is ever added.

## Out of Scope

- **An install script.** The install is a prompt-driven run-once setup skill, not a script; an automated, temp-`HOME`-tested helper is a later addition, not this spec.
- **Memory growth and decay** — compaction, archiving, pruning, and a consolidation pass over a vault that accumulates for months; and **link density** — how dense the wikilink graph can get before navigation degrades. Both are in the map's *Not yet specified*, in scope for the effort but not specifiable yet; they open a future effort, not this handoff.
- **MCP note tools** — the agent surface stays on-disk files (the bikeshed map's scope cut stands).
- **Cross-machine sync** of the bikeshed — one Mac, one vault.
- **App changes** — the skill uses the app's existing surfaces (vault, wikilinks, backlinks, search, todo MCP tools); no app feature rides on this spec.
- **Vault history / version control** — the bikeshed is a plain, unversioned folder; the write protocol's dated reversal line is the stopgap.
- **Backfilling past work into the wiki** — the vault accumulates forward from adoption; pre-existing human and agent notes are not reorganised.
- **A router skill** — `tandem-memory` is model-invoked and reached directly, and `setup-tandem-memory` is run once by hand; no router is needed for two skills.

## Further Notes

- **Provenance.** Every decision above traces to `.scratch/tandem-memory/map.md` and its resolved tickets 01–05 (the ticket Answers are normative for their areas), ADR 0004 and ADR 0005, branch `prototype/tandem-memory-layout` (the accepted layout sketch and orientation walk), and branch `research/tandem-memory-model-invocation` (the harness discovery/invocation/install facts and description-wording guidance).
- **Two skills, one source.** The deliverable ships as a pair — `tandem-memory` (model-invoked) and `setup-tandem-memory` (user-invoked) — settled at spec time from the map's "a global install/link step": the step is embodied as a run-once skill in the mould of `setup-matt-pocock-skills`, so the install is discoverable and guided rather than a command the human must get right by hand.
- **The map is complete.** All five decision tickets are resolved; this spec is the destination, written at handoff. The map's remaining *Not yet specified* items (growth/decay, link density) and *Out of scope* items are carried into the sections above rather than left as fog.
- **This is the handoff act, not a ticket** — writing `skills/tandem-memory/` and `skills/setup-tandem-memory/` is the implementation session this spec seeds; the map produces decisions, this spec, and then the skills.
- **Why "the notes are the compression."** The load-bearing idea behind eager writes is that a session which would need to compact instead ends and the next cold-starts from the vault; dense, eager notes are what make compaction unnecessary, which is the point of the protocol rather than a side effect.
