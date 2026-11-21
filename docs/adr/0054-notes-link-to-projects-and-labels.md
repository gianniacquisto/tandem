# Notes link to projects and labels

> **Status:** In force. Amends [ADR 0005](0005-wikilink-rename-rewriting.md). See [the index](README.md).

An orphan was a note nothing links to, so all seven daily notes were listed, and a link a note sent out did not count. A note should be able to say what it is about, and the orphan list should name only notes that float free. This record settles the model; the folder upkeep, the notes UI and the graph view build on it.

Decided 2026-11-20 (TAN-528).

## Decision

- **Typed wikilinks.** `[[project:Name]]` and `[[label:Name]]`. The prefix is case-insensitive, and only `project:` and `label:` are recognised: any other `[[a:b]]` is a plain note target, as before. `#fragment` and `|alias` parse as for any wikilink. The name resolves case-insensitively against the board: a project name, or a label name (label names are unique board-wide). A name the board does not know is a broken link, reported as written (`label:clii`). The resolver tries the typed prefix first, then the note target.
- **The board's names are a value.** The bikeshed never opens the store. `BoardNames` (the projects and each project's labels) is built from the board in `rpc/mod.rs` and the CLI and passed into `health` and the resolver. That is the one join point: names from the board, folders from the vault.
- **The folder convention.** A project's folder is `projects/<project>/`. A label's folder is `projects/<project>/<label>/`; a label named like its own project maps to the project folder itself. A folder matches its project or label case-insensitively. The folders are created and moved with the board (see Consequences).
- **An orphan** is a note with no resolved link in or out (typed links count as links out) and that is not filed anywhere under a matched project folder (`projects/<p>/...`, any depth, `<p>` a board project). The Hub and project entry-note exemption is gone. Daily notes live in `daily/`, are never filed under a project, and leave the orphan list only by linking to something.
- **Unmatched folders.** `note_health` reports `unmatchedFolders`: a direct child of `projects/` that matches no project (`no project`), and a direct child of a matched project folder that matches none of its labels (`no label`). Deeper folders are free. `counts.unmatchedFolders` totals them. A vault with no `projects/` folder reports none.
- **The graph's node and edge types**, for the graph view. Nodes: `project`, `label`, `folder` (any other folder) and `note`. Edges: `contains` (a folder holds a subfolder or a note; a project folder holds its label folders) and `links` (a note to a note, a project or a label). `contains` edges are not links for the orphan rule: filing is covered by the project-folder exemption.

## Consequences

- **The vault follows the board (TAN-531).** `project_add` creates `projects/<project>/` and `label_add` creates `projects/<project>/<label>/` (nothing for a label named like its project), through `Bikeshed::create_folder`, so the path guard of ADR 0008 applies. A folder that matches ignoring case is reused and never renamed for case. A name that is not a single path component gets no folder.
- **Rename moves, never leaves broken links.** `project_rename` and `label_rename` (and a re-parent) move the folder with the rewrite of ADR 0005, then rewrite `[[project:Old]]` and `[[label:Old]]` to the new name, keeping fragment and alias, skipping frontmatter and code, per-file atomic (ADR 0004). Delete leaves the folder and its notes.
- **The board commits first.** The join lives in the rpc and serve layer, never in `Board`. A failed vault step does not undo the board write: the answer carries a `note` and `note_health` lists the leftovers. `tandem serve` creates any missing folder on start and on every project or label write, and never moves or deletes.
- **`missingFolders`.** `note_health` adds `missingFolders` (`project`, `label` or null, `path`) and `counts.missingFolders`: the board's projects and labels with no folder, including names that cannot have one.
- **Graph (TAN-536).** `Bikeshed::graph(&BoardNames, &GraphScope)` reads the vault once as the typed nodes and edges above, through the same folder matching, typed-link resolver and orphan rule as `note_health`. `GET /notes/graph.json?project=<name>` serves it (Board permission) as `{nodes, edges, total, shown, cap}`; a project scope keeps that project's subtree, the notes that link into it and their parent folder, and an unknown project is an error naming the projects. The answer is capped at 1000 nodes: every project, label and folder stays, notes go by link degree then path, `total` counts the nodes before the cap and `shown` after it. A project or label a link names whose folder is missing is a node with a null `path`; a broken link is no node.

## Amendments

- **[ADR 0005](0005-wikilink-rename-rewriting.md)**: its wikilink grammar gains the typed form. A typed link is never rewritten by a rename, since it names a board entity and not a note.

## Why

Frontmatter `project:` and `labels:` were rejected: ADR 0004 keeps frontmatter opaque. Plain links to entry notes were rejected: bare names clash with notes such as `Agent System/CLI.md`. Labels and folders are one vocabulary, the label's folder, so they cannot drift. Structure is enforced by the report and nothing is refused, because a hard rule would refuse notes outside project folders.
