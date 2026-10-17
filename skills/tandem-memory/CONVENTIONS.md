# Conventions — the vault, names, and where notes go

The vault layout, naming, entry point, and link discipline for every memory note. The read walk is in [`READ-PROTOCOL.md`](READ-PROTOCOL.md); when and what to write is in [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md).

## The vault

The **Bikeshed** — `~/.tandem/bikeshed/` — is a plain folder of `.md` notes. The **filename is the note's identity**: renaming a note renames the file. Attachments (non-`.md` files) sit alongside and are reachable only by relative path from a note (ADR 0004).

## Two folder kinds

- **`projects/`** — one **anchor note** per project, `projects/<Project>.md`. It becomes a folder, `projects/<Project>/` with the anchor inside, only when the project earns a second note.
- **`topics/`** — flat, cross-project concepts, `topics/<Topic>.md`.

No nesting below those two, so placement stays predictable — ADR 0004 permits nested folders, and this convention spends none. Placement is **intermixed**: a note lives wherever its topic naturally lives, beside **human notes**. When a human note already covers a topic, extend it — never create a parallel agent note. One concept, one note.

## Names

- **Title Case** — `Bikeshed`, `Throwaway Branch Workflow` — matching the human notes.
- **Basename unique vault-wide**: a bare `[[Name]]` needs exactly one case-insensitive filename match.
- Keep names free of `/`, `#`, and `|` — the path and wikilink separators — so every name stays one file and writable as a link.
- **No agent prefix.** There is no agent namespace and no name-based ownership; on a collision you rename *your* note.

## The entry point — `Index.md`

`Index.md` at the vault root is the one fixed constant every session knows. It holds **pointers and one-line status only** — no rationale, no detail; those live in the project and topic notes.

```markdown
# Index

<one line: what this vault is>

## Projects

- [[Project]] — one-line status.

## Topics

- [[Topic]] — one line.

## Threads

Active work lives in the Tandem todos (`tandem-cli todo list`). A project note's Threads section names the threads that belong to it.
```

Two tiers below it: the **anchor note** (`Where left off` at the top) → **topic notes** via wikilinks. The app's **Backlinks** panel is the one-hop neighbourhood, derived and maintained by nobody.

The Index stays honest by being derived, never authored: a project's Index line is **re-derived from that anchor's `Where left off`**, and a session that updates the anchor updates its Index line in the same session — and only when the status actually changed. That is what keeps the Index from becoming a second source of truth.

## Granularity and links

A concept **earns its own note** when two or more notes must link to it, or it spans projects. Otherwise it is a section in the most relevant note; a section outgrowing its length target is the signal that the concept earned a note (targets in [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md)).

A new note **wikilinks out** to every existing note it mentions by name. A broken `[[link]]` is written only when you create its target in the same breath; a one-off mention stays plain text. Outbound links are yours to maintain; the inbound side is derived (**backlinks**).

A `[[Name]]` resolves as a **case-insensitive filename match**, not a search — a duplicated basename is a **visible broken link**, never a silent tie-break (ADR 0005).
