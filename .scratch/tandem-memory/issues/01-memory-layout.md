# 01 - Memory layout and entry point: intermixed notes, naming, orientation

Type: prototype
Status: claimed (2026-10-17)

## Question

Placement is intermixed: the skill creates notes anywhere in the vault where the topic naturally lives, with no owned area. Settle the layout discipline, and the orientation surface a new session uses to pick up where the work left off.

1. **Naming conventions** for agent-created notes: per-project notes, topic/knowledge notes, anything session-scoped — how to name them so they are findable among human notes and wikilinkable (the app resolves wikilinks by case-insensitive basename; a bare target needs a unique basename match, duplicates become visible broken links — ADR 0005).
2. **Placement**: where a note lives by topic — flat, nested folders, per-project folders mixed with topic folders; how an agent decides where a new note goes.
3. **Entry point**: how a new session orients — a vault-level index note, a per-project anchor note, a lookup convention, or a combination; what it contains and how it stays current without becoming a second source of truth.
4. **Granularity and links**: when a concept earns its own note vs a section in an existing note; the wikilink discipline that makes a topic's neighbourhood (its related notes) reachable in one hop via backlinks.

Deliverable: a concrete sketch for the human to react to: the note tree for two worked projects plus cross-project topics, 2–3 sample notes written in agent-optimised style (per `writing-for-agents`) with the highlight taxonomy applied (where-left-off, decisions + rationale, learnings/gotchas, expensive-to-derive facts), and the orientation walk a new session would take from a cold start. Build it as a prototype (Skill tool: `prototype`) on a throwaway `prototype/tandem-memory-layout` branch, report files under `.scratch/tandem-memory/prototype/`; link the prototype from this ticket. The resolution records the settled conventions.
