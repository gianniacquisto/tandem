# Map: Tandem memory — agent memory and wiki on the bikeshed

Type: wayfinder:map

## Destination

A working, model-invoked skill stored in this repo at `skills/tandem-memory/`: `SKILL.md` plus companion convention docs, and a global install/link step so sessions in any repo on this Mac see it (OpenCode and Claude Code). The skill makes the bikeshed (`~/.tandem/bikeshed/`) the backbone of agent memory: sessions record the important highlights of their work as they go, so a new session can pick up where things left off or find information related to what it is doing; the notes double as a wiki of all the work, navigable by wikilinks and backlinks in the app. Writing the skill is the handoff act, not a ticket: the map settles the design decisions the skill's text depends on.

## Notes

> **Amended (2026-10-17, [ADR 0009](../../docs/adr/0009-cli-agent-face.md)):** the todo surface is now `tandem-cli` (`todo list` / `todo add` / `todo complete`), not MCP — read every MCP todo reference below as that binary. The notes surface is unaffected: the bikeshed is still on-disk files.

- Domain: an agent-facing skill plus the conventions it encodes. The memory surface is the existing bikeshed: a plain `.md` folder at `~/.tandem/bikeshed/` (ADR 0004: filename = note identity, one-sided sidecar lock — agents write plain files with no lock, last write wins; the app watches live). Wikilinks resolve by case-insensitive name (ADR 0005). MCP exposes todos only (`list`/`add`/`complete`).
- Tracker: local markdown — map at `.scratch/tandem-memory/map.md`, tickets at `.scratch/tandem-memory/issues/NN-<slug>.md`. Claim = set `Status: claimed`. Blocking = `Blocked by:` line in the ticket header.
- Spec (the destination): `.scratch/tandem-memory/spec.md` — written 2026-10-17 from the resolved decisions below; labelled `ready-for-agent`. Deliverable: two skills under `skills/` — `tandem-memory` (model-invoked) and `setup-tandem-memory` (user-invoked, run once) — the second being the global install/link step embodied as a skill.
- Settled during charting (conversationally, not as tickets):
  - Plan, don't do: the map produces decisions; writing `skills/tandem-memory/` (including the install step) is the handoff act.
  - Invocation: model-invoked, not human-invoked — the description is the always-loaded context pointer that fires the skill on its own (no `disable-model-invocation`); the description wording is settled from ticket 04's findings at handoff.
  - Storage/discovery: single source of truth in this repo at `skills/tandem-memory/`, linked/installed globally for OpenCode and Claude Code; no per-repo copies.
  - Placement: intermixed — the skill creates notes anywhere in the vault where the topic naturally lives; no owned area. The whole vault is the wiki; the orientation/entry mechanism and naming discipline are ticket 01.
  - Highlight taxonomy: memory records where the work left off, decisions + one-line rationale, learnings/gotchas, and expensive-to-derive facts; the test is whether the next session can cheaply re-derive the fact from the repo or environment — if it can, the note omits it. Excluded: verbose narration, code, step-by-step procedures, raw transcripts.
  - Todos: notes carry durable memory + wiki; tandem todos (via MCP `add`) carry the active work — the skill records open threads as todos and wikilinks them from notes; the two stores stay separate.
  - Note style: notes are written for the next agent session as reader, optimised per `writing-for-agents` (context pointers, information hierarchy, single source of truth, leading words, pruning).
- Skills to consult: `grilling` + `domain-modeling` for grilling/prototype tickets; `research` for research tickets; `writing-for-agents` (+ `SKILL-MECHANICS.md`) when the handoff writes the skill. `GLOSSARY.md` at repo root is the vocabulary authority; add entries as terms resolve (candidates: **memory note**, **orientation note**).

## Decisions so far

<!-- one line per closed ticket: name (wrapping its link) + one-line gist -->

- [Memory layout and entry point: intermixed notes, naming, orientation](issues/01-memory-layout.md): two folder kinds — `projects/` (one note per project; the note becomes a folder when a second note is earned) and `topics/` (flat, cross-project); the agent extends a human note that covers a topic instead of creating a parallel note; Title Case, basename unique vault-wide, no agent prefix, no `#`/`|`; the entry point is two tiers — `Index.md` at the vault root (pointers + one-line status only, lines re-derived from the project anchor and updated in the same session) → project anchor (Where left off at top) → topics via wikilinks, the Backlinks dock as the one-hop neighbourhood; a concept earns its own note when 2+ notes must link to it or it spans projects, else a section, and only outbound links are maintained.
- [Model-invoked skill mechanics: discovery, description wording, global install](issues/04-model-invocation.md): model-invocation is the default in both harnesses (frontmatter = `name` + `description`, avoid the `disable-model-invocation`/`opencode-autoinvoke` opt-outs); the description is the only always-loaded trigger, so it scopes first then names both trigger branches (session-start orientation before touching project files; record a note on decisions, gotchas, finished units, open threads), with the record-as-you-go discipline living in the body that stays in context after the first fire; install = symlink `skills/tandem-memory/` into `~/.agents/skills/` (OpenCode) and `~/.claude/skills/` (Claude Code), both of which follow symlinks and hot-reload on repo updates.
- [Write protocol: when and how sessions record highlights](issues/02-write-protocol.md): eager event-driven writes at four moments + a closing pass (`Where left off`, and the `Index` line only on a status change) — the notes *are* the compression, so a session that would compact instead ends and the next cold-starts from the vault; qualification is **cost of derivation** (effort to create, not wall-clock), omitting what the repo/environment reproduces; correct in place, a flipped decision leaving one dated reversal line; amend the covering note, lazy anchor bootstrap; threads = the todo title verbatim in the note's `Threads` + MCP `add`/`complete`; fixed skeleton with ≲50/≲40-line targets, prune on write; done when a cold session could resume from the vault alone.
- [Read and orientation protocol: picking up, and finding related information](issues/03-read-protocol.md): orientation is a bounded mandatory read — `Index.md` → one project anchor (`Where left off` full, other sections scanned by heading), stop at state + next move + open threads, topics/human notes on demand; retrieval order is curated wikilinks → backlink grep (`[[Name]]`) → keyword grep, and `[[Name]]` resolves as a filename, not a search; the repo is ground truth — correct notes in place (dated line if a decision flipped) and repair broken links opportunistically, never by sweep or guess; reads are trusted all session, re-read only before writing or on an observed external change; human notes share the read path but are context, not record — only agent `Where left off`/`Decisions` carry state authority.
- [Multi-agent concurrency: two harnesses, one vault](issues/05-multi-agent-concurrency.md): no scratch tier — every note is shared, a session's working memory is its context window; coordination is **lock-free** (no claim files, ADR 0004's lock stays app-only) and the lock's replacement is a rule, the **write boundary** — re-read immediately before a minimal-patch write, never blind-overwrite, so both sessions' text survives and a same-line contradiction falls back to 02's dated reversal; new highlights are additive with only the one superseded line edited in place; a project anchor is bootstrapped by an atomic `O_EXCL` create whose loser amends; `Index` status lines are re-derived from the anchor, overlapping rewrites are ordered by who hits the boundary second, a stale read self-corrects at the next write, and a deliberate delete is respected (a rename is re-pointed, else its links demote to plain text).

## Not yet specified

- Memory growth and decay: compaction, archiving, pruning, and a consolidation pass (an agent that rewrites notes denser or merges them) as the vault accumulates over months — this is the maintenance pass above the write protocol (02, now settled), not a blocker on it.
- Link density: how dense the wikilink graph can get before navigation degrades, as the vault accumulates; specifiable alongside growth and decay.
- `writing-for-agents` coupling: whether the skill references the installed `writing-for-agents` skill or inlines a distilled rule set; a handoff decision.

## Out of scope

- Backfilling past work into the wiki: the vault accumulates forward from adoption; pre-existing notes (human or agent) are not reorganised.
- MCP note tools: the agent surface stays on-disk files (the bikeshed map's scope cut stands; a separate effort if ever wanted).
- Cross-machine sync of the bikeshed: one Mac, one vault.
- App changes: the skill works with the app's existing surfaces (vault, wikilinks, backlinks, search, todo MCP tools); no app feature requests ride on this map.
- Vault history / version control: the bikeshed is a plain, unversioned folder, so a corrected note is unrecoverable — giving it durable history is a platform concern, not the skill's (the write protocol's dated reversal line is the stopgap).
