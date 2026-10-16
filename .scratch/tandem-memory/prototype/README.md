# PROTOTYPE — tandem-memory layout sketch (ticket 01)

Throwaway, from day one. Answers ticket 01 — *"the layout discipline, and the orientation surface a new session uses to pick up where the work left off"* — with a concrete sketch to react to. Never merged, no PR. Branch: `prototype/tandem-memory-layout`.

## The sketch

- **`vault/`** — a mock of `~/.tandem/bikeshed/`: the current human notes (`Welcome`, `Ideas`, `projects/Alpha`) intermixed with the agent-created notes this sketch proposes.
  - `Index.md` — the proposed entry point: vault-level pointers + one-line status only.
  - `projects/Bikeshed.md` — the full sample: a project anchor with the highlight taxonomy applied (Where left off / Decisions + rationale / Gotchas / Facts / Threads). Content is real, from the resolved bikeshed map and ADRs 0004/0005.
  - `projects/Distribution.md` — a deliberately thin second project note; content illustrative, the shape is the point.
  - `topics/Throwaway Branch Workflow.md` — a cross-project topic note; content real (this repo's own branch practice).
- **`orientation-walk.md`** — the cold-start walk the entry point supports (five steps, bounded by construction).

## The four questions the sketch takes a position on

1. **Naming** — Title Case, human-consistent (`Welcome`, `Alpha`), no `#`/`|`, basename unique vault-wide (bare wikilinks need it). No agent prefix: a prefix would own a namespace by name, contradicting intermixed placement.
2. **Placement** — two folder kinds, predictable: `projects/` (one note per project; the note becomes a folder when a second note is earned) and `topics/` (flat, cross-project concepts). A human note that already covers a topic is extended, never shadowed.
3. **Entry point** — two tiers: `Index.md` at the vault root (the one fixed constant every session knows) → project anchor note (Where left off at top) → topic notes via wikilinks; the Backlinks dock is the one-hop neighbourhood. The Index holds pointers + one-line status only and its lines are re-derived from the project note, so it cannot become a second source of truth.
4. **Granularity + links** — a concept earns its own note when 2+ notes must link to it or it spans projects; otherwise it is a section. Link out to every existing note a new note mentions; a broken link is written only when the agent creates the target in the same breath.

The read *protocol* proper — how much to load, stop conditions — is ticket 03's; this sketch settles the shape the protocol walks.
