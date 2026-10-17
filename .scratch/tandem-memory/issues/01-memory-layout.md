# 01 - Memory layout and entry point: intermixed notes, naming, orientation

Type: prototype
Status: resolved (2026-10-17, human verdict on the sketch)

## Question

Placement is intermixed: the skill creates notes anywhere in the vault where the topic naturally lives, with no owned area. Settle the layout discipline, and the orientation surface a new session uses to pick up where the work left off.

1. **Naming conventions** for agent-created notes: per-project notes, topic/knowledge notes, anything session-scoped — how to name them so they are findable among human notes and wikilinkable (the app resolves wikilinks by case-insensitive basename; a bare target needs a unique basename match, duplicates become visible broken links — ADR 0005).
2. **Placement**: where a note lives by topic — flat, nested folders, per-project folders mixed with topic folders; how an agent decides where a new note goes.
3. **Entry point**: how a new session orients — a vault-level index note, a per-project anchor note, a lookup convention, or a combination; what it contains and how it stays current without becoming a second source of truth.
4. **Granularity and links**: when a concept earns its own note vs a section in an existing note; the wikilink discipline that makes a topic's neighbourhood (its related notes) reachable in one hop via backlinks.

Deliverable: a concrete sketch for the human to react to: the note tree for two worked projects plus cross-project topics, 2–3 sample notes written in agent-optimised style (per `writing-for-agents`) with the highlight taxonomy applied (where-left-off, decisions + rationale, learnings/gotchas, expensive-to-derive facts), and the orientation walk a new session would take from a cold start. Build it as a prototype (Skill tool: `prototype`) on a throwaway `prototype/tandem-memory-layout` branch, report files under `.scratch/tandem-memory/prototype/`; link the prototype from this ticket. The resolution records the settled conventions.

## Verdict

Human verdict on the sketch (2026-10-17): all four positions accepted as sketched. Settled conventions:

- **Naming** — Title Case, consistent with human notes; basename unique vault-wide (a bare wikilink needs a unique case-insensitive basename; a duplicate is a visible broken link, ADR 0005); no `#`/`|` in names (unnameable by wikilink); no agent prefix — a prefix would own a namespace by name and contradict intermixed placement. An agent that collides with an existing name renames itself; it never shares.
- **Placement** — exactly two folder kinds: `projects/` (one note per project; the note becomes a folder — `projects/P/` with the anchor inside — only when a second note is earned) and `topics/` (flat, cross-project concepts); no deeper nesting. When a human note already covers a topic, the agent extends that note instead of creating a parallel agent note: one concept, one note, single source of truth at vault level.
- **Entry point** — two tiers. `Index.md` at the vault root is the one fixed constant every session knows; it holds pointers and one-line status only — no rationale, no detail, those live in the project and topic notes. A project's Index line is re-derived from that note's *Where left off*, and a session that updates a note updates the Index line in the same session — what keeps the Index from becoming a second source of truth. Below it: the project anchor note (Where left off at the top) → topic notes via wikilinks; the app's Backlinks dock is the one-hop neighbourhood, derived and maintained by nobody.
- **Granularity + links** — a concept earns its own note when 2+ notes must link to it (a link target has to be a note; fragments are inert, ADR 0005) or it spans projects; otherwise it is a section in the most relevant note. A new note wikilinks out to every existing note it mentions by name; a broken link is written only when the agent creates the target in the same breath; a one-off mention stays plain text. Outbound links are maintained; the inbound side is derived (backlinks).

Left open for downstream tickets: the `Threads` mechanism — how a note references a todo (02 Q4) — and the load discipline of the orientation walk (03).

Prototype state: branch `prototype/tandem-memory-layout` — throwaway, never merged, no PR. Files under `.scratch/tandem-memory/prototype/`: `vault/` (mock of the bikeshed — human notes intermixed with the proposed agent notes: `Index.md`, `projects/Bikeshed.md`, `projects/Distribution.md`, `topics/Throwaway Branch Workflow.md`) and `orientation-walk.md`. An HTML walkthrough was ruled overkill by the human (2026-10-17); the sketch is the files themselves.
