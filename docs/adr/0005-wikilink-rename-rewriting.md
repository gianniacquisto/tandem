# Wikilink rename: app-performed renames rewrite referencing wikilinks — and only wikilinks

[ADR 0004](0004-vault-protocol.md) settles how a rename-with-link-rewrite is *performed* (one-sided sidecar lock, per-file atomic rewrites before the note rename, no journal). This ADR settles what the rewrite *covers* and what the vault does with colliding names. We decided: a rename/move performed by the app rewrites every `[[wikilink]]` in the vault that resolves to the renamed note — in any form (bare, qualified, aliased), in every note's body including the note itself — and nothing else: markdown links are never rewritten, wikilinks in frontmatter are never rewritten (frontmatter stays opaque, ADR 0004), and a rename an agent performs on disk rewrites nothing, leaving visible broken links. When two notes share a basename, a bare link to that name is a **broken link**, not a silently chosen match.

## Why these boundaries

- **Rewrite everything (Obsidian's default)** — Obsidian also rewrites markdown links and embeds. Markdown link targets can be percent-encoded paths relative to the containing note, so that needs a real markdown-link parser for links the app itself never writes; v1 keeps the rewrite scope to the grammar it owns, and the spec tells agents to use wikilinks.
- **Never rewrite** — every rename would silently strand its references as broken links the user must find and fix one by one; the multi-file write is the price of "a rename is one action".
- **Rewrite on agent renames too (watch and repair)** — a watcher-driven rewrite would race the very agents the vault exists for, and the app cannot tell a deliberate agent rename from other on-disk churn; app-performed renames are the only rewrite trigger.
- **Silent tie-break on duplicate names (Obsidian's closest-to-root)** — a bare link to a duplicated name would resolve to *one* of the notes under Obsidian's community-documented tie-break (not in its own help); here it is a visible broken link instead — deterministic, fixable in one edit, and it cannot silently keep pointing at the wrong note after an agent creates a duplicate.

## Consequences

- An agent or external-editor rename leaves broken links behind, by design — the spec states the asymmetry outright, as it does for deletion (ADR 0004).
- Rewriting is grammar-level text substitution over note bodies — no markdown parse, no stored link index. Resolution is derived from the current file set, so creating or deleting notes never rewrites anything; only rename does.
- Notes whose on-disk names contain `#` or `|` (agent/external creation only — the app never creates such names) are unnameable by wikilink: any `[[…]]` mentioning them splits into a fragment/alias and resolves elsewhere or breaks. The spec tells agents to keep names free of both.
