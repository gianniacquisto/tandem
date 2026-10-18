# Conventions — the bikeshed, names, and where notes go

The bikeshed layout, naming, entry point, and link discipline for every memory note. The read walk is in [`READ-PROTOCOL.md`](READ-PROTOCOL.md); when and what to write is in [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md); the pass that writes the curated notes is [`CONSOLIDATION.md`](CONSOLIDATION.md).

## The bikeshed

The **Bikeshed** — `~/.tandem/bikeshed/` — is a plain folder of `.md` notes. The **filename is the note's identity**: renaming a note renames the file. Attachments (non-`.md` files) sit alongside and are reachable only by relative path from a note (ADR 0004).

## Two layers

- **raw** — many small per-moment notes, one file per moment, at `projects/<project>/.raw/<session>/<utc>-<kind>-<slug>.md`. Hidden, **create-only** (never edited), **no wikilinks**. Filed by *every* agent through `tandem-cli note capture`; the verb owns the kinds, body shape, and naming (`skills/tandem/SKILL.md`). Read only by the consolidation pass, which archives what it consumes to `projects/<project>/.raw/.done/<session>/`.
- **curated** — the human- and agent-readable notes under `projects/<project>/` and `topics/`. A working agent **never** edits them; the consolidation pass is the sole writer ([`CONSOLIDATION.md`](CONSOLIDATION.md)).

## Folder layout

- **`projects/`** — one folder per **project**, `projects/<project>/`. `<project>` is the **codebase/product you are working in** (`tandem` today, `code-rank` another day), not the workstream. The folder holds the project's curated notes **and** its hidden `.raw/` capture layer, so a project is **always** a folder — from the first record. A folder can hold several curated notes: the Index carries one line per note, so one project can contribute several lines.
- **`topics/`** — flat, cross-project concepts, `topics/<Topic>.md`.

No nesting below those two, so placement stays predictable — ADR 0004 permits nested folders and this convention spends none (the hidden `.raw/` is the one deliberate exception, outside the curated tree). Placement is **intermixed**: a curated note lives wherever its topic naturally lives, beside **human notes**. When a human note already covers a topic, extend it — never create a parallel agent note. One concept, one note.

## Names

- **Title Case** — `Bikeshed`, `Throwaway Branch Workflow` — matching the human notes.
- **Basename unique bikeshed-wide**: a bare `[[Name]]` needs exactly one case-insensitive filename match.
- Keep names free of `/`, `#`, and `|` — the path and wikilink separators — so every name stays one file and writable as a link.
- **No agent prefix.** There is no agent namespace and no name-based ownership; on a collision you rename *your* note.

Raw filenames are the exception: they are the verb's `<utc>-<kind>-<slug>`, never authored by hand and never linked.

## The entry point — `Index.md`

`Index.md` at the bikeshed root is the one fixed constant every session knows. It holds **pointers and one-line status only** — no rationale, no detail; those live in the project and topic notes.

```markdown
# Index

<one line: what this bikeshed is>

## Projects

- [[Note]] — one-line status.

## Topics

- [[Topic]] — one line.

## Threads

Active work lives in the Tandem todos (`tandem-cli todo list`). A project note's Threads section names the threads that belong to it.
```

Two tiers below it: the **curated note** (`Where left off` at the top) → **topic notes** via wikilinks. The app's **Backlinks** panel is the one-hop neighbourhood, derived and maintained by nobody.

The Index stays honest by being derived, never authored: a note's Index line is **re-derived from that note's `Where left off`**, and the consolidation pass rewrites it in the same run — and only when the status actually changed. That is what keeps the Index from becoming a second source of truth.

## Granularity and links

A concept **earns its own topic note** when two or more notes must link to it, or it spans projects. Otherwise it is a section in the most relevant note; a section outgrowing its length target is the signal that the concept earned a note (targets in [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md)). Creating that note is the consolidation pass's call.

Wikilinks live **only in curated notes**, and the consolidation pass is the only thing that writes them — a raw note carries none, so the graph has one writer. A curated note **wikilinks out** to every existing note it mentions by name. A broken `[[link]]` is written only when you create its target in the same breath; a one-off mention stays plain text. Outbound links are the writer's to maintain; the inbound side is derived (**backlinks**).

A `[[Name]]` resolves as a **case-insensitive filename match**, not a search — a duplicated basename is a **visible broken link**, never a silent tie-break (ADR 0005).

Raw notes are **out of the graph** by construction: the `.raw/` subtree is hidden, so `Bikeshed.isHidden` prunes it from the tree, `notePaths`, search, backlinks, and wikilink resolution — code-enforced, not a rule an agent must remember.
