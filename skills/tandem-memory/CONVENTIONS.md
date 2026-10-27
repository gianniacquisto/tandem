# Conventions — the bikeshed, names, and where notes go

The bikeshed layout, naming, entry point, and link discipline for every memory note. The read walk is in [`READ-PROTOCOL.md`](READ-PROTOCOL.md); when and what to write is in [`WRITE-PROTOCOL.md`](WRITE-PROTOCOL.md); the pass that writes the curated notes is [`CONSOLIDATION.md`](CONSOLIDATION.md).

## The bikeshed

The **Bikeshed** — `~/.tandem/bikeshed/` — is a plain folder of `.md` notes. The **filename is the note's identity**: renaming a note renames the file. Attachments (non-`.md` files) sit alongside and are reachable only by relative path from a note (ADR 0004).

## Two layers

- **saddlebag** — small create-only notes, one file each, at `projects/<project>/.raw/<session>/<utc>-<kind>-<slug>.md`. Hidden, **create-only** (never edited), **no wikilinks**. Filed by *every* agent through `tandem-cli note capture`; the verb owns the kinds, body shape, and naming (`skills/tandem/SKILL.md`). Read only by the consolidation pass, which archives what it consumes to `projects/<project>/.raw/.done/<session>/`.
- **scratch** — the human's scratch pad: a visible `scratch/` directory of daily notes, the human's working space. The consolidation pass reads it and folds what it holds into the curated layer. It is **read-only to the consolidator** — the pass never edits or archives the scratch; the human prunes.
- **curated** — the human- and agent-readable notes under `projects/<project>/` and `topics/`. A working agent **never** edits them; the consolidation pass is the sole writer ([`CONSOLIDATION.md`](CONSOLIDATION.md)).

The curated layer has **exactly three inputs** — the agents' saddlebag captures, the human's scratch, and the ended session records' summaries ([`CONSOLIDATION.md`](CONSOLIDATION.md)) — and exactly one writer: the consolidation pass. Three doors, one writer; no other input, no other writer.

## Folder layout

- **`projects/`** — one folder per **project**, `projects/<project>/`. `<project>` is the **codebase/product you are working in** (`tandem` today, `code-rank` another day), not the workstream. The folder holds the project's curated notes **and** its hidden saddlebag (`.raw/`), so a project is **always** a folder — from the first record. A folder can hold several curated notes: the Index carries one line per note, so one project can contribute several lines.
- **`topics/`** — flat, cross-project concepts, `topics/<Topic>.md`.

A project folder's notes sit **flat beside each other** until it holds **more than 7 curated notes**; past that threshold the project may **group** them into **one level** of sub-folders — `projects/<project>/<group>/<Note>.md`, and no deeper. A group name follows the note-name hygiene (no `/`, `#`, `|`; no hidden name). The **threshold** is what keeps placement predictable: below it a project's notes are flat and side by side; above it its sub-pages are grouped (the scaffold pages stay at the root). ADR 0004 permits nested folders, so this convention spends that permission only where the pile asks for it — `topics/` stays flat, and the hidden saddlebag (`.raw/`) remains the one deliberate exception, outside the curated tree. **Which group a note lands in is the consolidation pass's call**: the convention states the shape a project *may* take, not the shape it *must*. Placement is **intermixed**: a curated note lives wherever its topic naturally lives, beside **human notes**. When a human note already covers a topic, extend it — never create a parallel agent note. One concept, one note.

## The scaffold — every project's fixed curated pages

Every project's curated layer is a **fixed scaffold** of four pages, **created on demand** — the consolidation pass creates all four when it creates the project — and maintained by consolidation alone. The scaffold is the project's structure, not whatever notes accumulated:

| Page | File | Holds |
| --- | --- | --- |
| **Overview** | `projects/<project>/<project>.md` — the anchor note | what this is, how to run it, where things live, and what is open — its `Where left off` state and next move (the summary the front page shows) and its `## Threads` list — plus links to the project's other notes; human-readable prose, and the agent's entry point |
| **Decisions** | `projects/<project>/<project> Decisions.md` | one line per decision, dated: the choice, its rationale, and a link to its ADR or ticket |
| **Risks** | `projects/<project>/<project> Risks.md` | one line per open risk, dated: the risk, its consequence, and the ticket that owns it when one exists; a retired risk leaves the page |
| **Processes** | `projects/<project>/<project> Processes.md` | one line per process, dated: what it is, and a link to the note or ticket that documents it |

- **Fixed** — exactly these four pages per project; the scaffold never grows a fifth. The scaffold pages carry the project in their names, so basenames stay bikeshed-unique and bare `[[links]]` keep resolving (ADR 0005). The scaffold pages sit at the project root; [grouping](#folder-layout) applies to the project's other notes.
- **Decisions indexes, never copies** — the page carries an entry for **every ADR and every decision recorded on the board it can reach**, each linked to its ADR or its ticket. The ADR **stays in the repo** (`docs/adr/`): the page is the human's index of it, not a copy of it.
- **What fits no page is a linked sub-page** — a subsystem, or a pile of gotchas or facts that fits no scaffold page, earns the note that owns it ([Granularity and links](#granularity-and-links)): the full curated skeleton, placed by the rules above, linked from the Overview's notes.
- **Personas hold no partition** — no per-persona folders and no per-persona notes: identity lives in the prompts and the roster.

## One wiki, two readers — the page roles

The bikeshed is **documentation for both its readers**: the human who reads a page and acts on it, and the agent that opens it before touching code ([ADR 0033](../../docs/adr/0033-wiki-is-documentation.md)). One page serves both — a page written plainly enough for a human to follow is the page an agent can orient from — so each page states one job and keeps it.

What the wiki holds is **current truth** — what the project is, how to run it, where things live, what is open now — and **decisions**, as references with a short rationale. **The journal is out**: the narrative of what was tried, in what order and with what result belongs to git history and the ended session records ([ADR 0027](../../docs/adr/0027-session-records.md)), never to a page. No changelog, no dated diary, no `Next:` line — a stale next-step is worse than no line, because a human reads it as status and an agent reads it as truth.

| Page | Its one job |
| --- | --- |
| **Index** | pointers and one-line status — the way in, never the detail |
| **Overview** | what this is, how to run it, where things live, and what is open — its `## Threads` — in human-readable prose; the agent's entry point |
| **Decisions** | one line per decision, dated, linked to its ADR (which stays in the repo) or its ticket |
| **Risks** | one line per open risk, dated; a retired risk leaves the page |
| **Processes** | one line per process, dated, linked to the note or ticket that documents it |
| **Sub-pages and topics** | the project documentation proper — install, first run, the CLI's groups, the app's tabs, the recovery paths — one coherent topic per note |

The **README is the front door**: it installs the app and the CLI, and it points at `~/.tandem/bikeshed/projects/<project>/<project>.md` for the rest — so a human reaches a project's Overview, and finds install, first run, where things live and what is open there, without reading an ADR.

**Freshness rule.** A page names the date it was last true — `Reviewed <date>`, at the top under its title — or carries none, and the Notes tab's mtime is the reader's signal. A pass that changes a fact **amends the page in the same write**, as [`AGENTS.md`](../../AGENTS.md) requires for the session's capture: the page is corrected where it was read, never left for a later pass and never contradicted by a second appended line.

## Names

- **Title Case** — `Bikeshed`, `Throwaway Branch Workflow` — matching the human notes.
- **Basename unique bikeshed-wide**: a bare `[[Name]]` needs exactly one case-insensitive filename match. A group changes a note's **path**, not its **name** — a nested note is linked exactly like a flat one.
- Keep names free of `/`, `#`, and `|` — the path and wikilink separators — so every name stays one file and writable as a link.
- **No agent prefix.** There is no agent namespace and no name-based ownership; on a collision you rename *your* note.

Capture filenames are the exception: they are the verb's `<utc>-<kind>-<slug>`, never authored by hand and never linked.

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

Active work lives in the Tandem's open tickets (`tandem-cli ticket list`). A project's Overview `Threads` section names the threads that belong to it.
```

Two tiers below it: the **Overview** (`Where left off` at the top) → the project's pages and sub-pages via wikilinks. The app's **Backlinks** panel is the one-hop neighbourhood, derived and maintained by nobody.

A **grouped** note still gets exactly one Index line: the line names the note, never its group. A folder is placement, not a level of the Index — the Index stays one line per note however the tree is arranged.

The Index stays honest by being derived, never authored: a note's Index line is **re-derived from that note's `Where left off`** — and from a scaffold entry page's one-line intro, which has no `Where left off` — and the consolidation pass rewrites it in the same run, only when the status actually changed. That is what keeps the Index from becoming a second source of truth.

## Granularity and links

A **coherent topic earns its own note**. The test is **topical coherence** — the concept carries its own vocabulary and its own durable decisions and gotchas — *not* how many notes link it yet. A subsystem inside one project qualifies as readily as a cross-project concept; write it down the moment it is coherent, before the links arrive. Placement follows ownership: a concept one project owns lives at `projects/<project>/<Anchor>.md` — or `projects/<project>/<group>/<Anchor>.md` in a project that is grouped; a cross-project concept lives at `topics/<Topic>.md`. Creating that note, and the group it lands in, is the consolidation pass's call.

The **Overview is the project's overview and index**: it holds the current state, the next move, and links out to the project's notes — its scaffold pages and its sub-pages. Subsystem detail lives in the note that owns it, so the Overview never accumulates it. One concept, one note.

Wikilinks live **only in curated notes**, and the consolidation pass is the only thing that writes them — a capture carries none, so the graph has one writer. A curated note **wikilinks out** to every existing note it mentions by name. A broken `[[link]]` is written only when you create its target in the same breath; a one-off mention stays plain text. Outbound links are the writer's to maintain; the inbound side is derived (**backlinks**).

A `[[Name]]` resolves as a **case-insensitive filename match**, not a search — a duplicated basename is a **visible broken link**, never a silent tie-break (ADR 0005).

Captures are **out of the graph** by construction: the `.raw/` subtree is hidden, so `Bikeshed.isHidden` prunes it from `notePaths`, search, backlinks, and wikilink resolution — code-enforced, not a rule an agent must remember. The app's tree prunes it too, unless the human turns on **Show hidden files** (ADR 0023); that is a view change, and the graph stays the same either way.
