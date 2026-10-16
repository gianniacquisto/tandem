# Map: Tandem memory — agent memory and wiki on the bikeshed

Type: wayfinder:map

## Destination

A working, model-invoked skill stored in this repo at `skills/tandem-memory/`: `SKILL.md` plus companion convention docs, and a global install/link step so sessions in any repo on this Mac see it (OpenCode and Claude Code). The skill makes the bikeshed (`~/.tandem/bikeshed/`) the backbone of agent memory: sessions record the important highlights of their work as they go, so a new session can pick up where things left off or find information related to what it is doing; the notes double as a wiki of all the work, navigable by wikilinks and backlinks in the app. Writing the skill is the handoff act, not a ticket: the map settles the design decisions the skill's text depends on.

## Notes

- Domain: an agent-facing skill plus the conventions it encodes. The memory surface is the existing bikeshed: a plain `.md` folder at `~/.tandem/bikeshed/` (ADR 0004: filename = note identity, one-sided sidecar lock — agents write plain files with no lock, last write wins; the app watches live). Wikilinks resolve by case-insensitive name (ADR 0005). MCP exposes todos only (`list`/`add`/`complete`).
- Tracker: local markdown — map at `.scratch/tandem-memory/map.md`, tickets at `.scratch/tandem-memory/issues/NN-<slug>.md`. Claim = set `Status: claimed`. Blocking = `Blocked by:` line in the ticket header.
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

— none yet —

## Not yet specified

- Memory growth and decay: compaction, archiving, and pruning as the vault accumulates over months; specifiable once placement (01) and the write protocol (02) are settled.
- The human-note boundary: when a memory fact belongs in a human note, and when a human note is a natural memory source; needs the intermixed-placement model from 01.
- Note granularity and link density: when a concept earns its own note vs a section in an existing note, and how dense the wikilink graph should stay navigable; surfaces through 01 and 02.
- `writing-for-agents` coupling: whether the skill references the installed `writing-for-agents` skill or inlines a distilled rule set; a handoff decision.

## Out of scope

- Backfilling past work into the wiki: the vault accumulates forward from adoption; pre-existing notes (human or agent) are not reorganised.
- MCP note tools: the agent surface stays on-disk files (the bikeshed map's scope cut stands; a separate effort if ever wanted).
- Cross-machine sync of the bikeshed: one Mac, one vault.
- App changes: the skill works with the app's existing surfaces (vault, wikilinks, backlinks, search, todo MCP tools); no app feature requests ride on this map.
