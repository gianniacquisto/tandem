# Bikeshed move: a move is a core operation, the same transaction as a rename

[ADR 0005](0005-wikilink-rename-rewriting.md) settled that an app-performed **rename rewrites every wikilink that resolves to the renamed note** — one locked transaction, rewrite first, then a single `rename(2)`. [ADR 0013](0013-bikeshed-folders.md) added folders, and deliberately stopped at creation: "introduces no new move mechanism: moving a note stays the existing Rename… path change", and a folder could not be moved at all.

That left a hole: notes could be re-parented only by typing a new path into the Rename… sheet, and folders could not be re-parented at all. Re-organising the bikeshed meant Finder plus broken wikilinks — the exact failure the rename rewrite exists to prevent.

We decided: **a move is a core operation** — `Bikeshed.move(path:into:)` — and the tree exposes it as **drag to re-parent**: drop a note or a folder on another folder, and the file moves with its links intact.

## Decision

- **One operation, one transaction.** A move takes a note or a folder and a destination folder, and under the bikeshed lock it rewrites every wikilink in the bikeshed that resolves to a moved note — bare, qualified, aliased, `#fragment`, in any order, in every note including the moved ones — re-canonicalised against the post-move file set (bare when the new basename is unique, qualified when it is not), and then moves the file with a single `rename(2)`. The rewrite is the same machinery as a rename, generalised from one target to a set: the same scanner, the same wikilinks-only rule (markdown links, embeds, code and frontmatter stay byte-identical), the same per-file atomic writes.
- **A folder move moves the subtree and rewrites the links to every note inside it.** The folder's directory moves with one `rename(2)` — its attachments and its nested folders travel with it — and the rewrite covers every note the folder carried, so no link inside or outside the moved folder is left pointing at the old path.
- **Two refusals, with a reason.** A folder moved into itself or into one of its own descendants is a **cycle** (`moveCycle`) — the tree would become cyclic. A name already taken in the destination folder is a **collision** (`alreadyExists`) — never an overwrite, never a silent suffix. Both are refused with nothing written, and the app shows the reason.
- **The move never renames.** The item's own name is untouched, so a move does not run the create/rename naming validation; only the destination's path hygiene is checked.
- **Moving an item into the folder it is already in is a no-op** — nothing written, no error.

## Why

- **A raw `mv` in the GUI breaks links silently.** The whole point of the rename rewrite is that a path change carries its references with it. A file move without the rewrite is a worse outcome than no move at all: the human believes the bikeshed is tidy while a dozen notes point at nothing.
- **The rewrite is already written and already right.** Generalising it from one target to a map of targets is a small change; re-implementing it for folders would be a second implementation of a rule ADR 0005 already settled.
- **The lock already covers it.** One locked transaction is what makes a multi-file rewrite safe: readers never see a half-rewritten bikeshed, and a crash leaves at most broken links, never torn files.
- **A folder is a first-class item.** It shows in the tree, it can be created, deleted and renamed-by-move — leaving it un-movable made the tree's structure something the human could only rearrange from Finder.
- **Two refusals, not one.** A cycle is not a collision and a collision is not a cycle; they need different words, and a human who is told "a folder cannot be moved into itself" knows what to do next.

## Rejected

- **A raw `mv` in the GUI** — it breaks links silently (the ticket's own "decided against").
- **A `move` that renames at the same time** — rename and move are different gestures with different rules; the Rename… sheet stays the renaming path, and a move keeps the name.
- **One `move` per note, applied N times for a folder.** A folder move that moved its notes one by one would rewrite the links in N passes, each against a file set the previous pass had already changed, and would leave the folder half-moved if one note's move failed. The subtree moves as one unit.
- **A "Move to…" menu as the only affordance.** A menu of every folder in the bikeshed is a second surface for the same operation and a worse one for a tree that is meant to be navigated by eye; the drag is the gesture the tree already suggests.
- **Repairing links to a note an agent moved on disk.** ADR 0005 settled that asymmetry on purpose: the app rewrites the links it breaks itself, and only those.

## Consequences

- `Bikeshed.move(path:into:)` joins the public operations and returns a `MoveOutcome`: the moved item's new path, and every note whose path changed with it (one for a note, all of them for a folder). The bikeshed root is the empty destination path.
- The rename machinery is now shared: `canonicalLinkTarget` takes the post-move file set, and `rewriteLinks` takes a map of moved paths, so `rename` and `move` cannot drift apart.
- The tree's rows are drag sources; a folder row is a drop target, and the tree's background is the bikeshed root. A drop that lands on no folder moves the item to the root.
- `BikeshedModel.move(_:into:)` saves the open note's pending edits first — the move rewrites the note's own text on disk, and a stale buffer would overwrite that rewrite on the next save — then refreshes the tree, the note index and the backlinks, and re-points the editor at the moved note's new path.
- A cycle and a collision surface as the core's own message, in the app's error banner.
- The move is unit-tested in `TandemCoreTests` (`BikeshedMoveTests`): note move, folder move, the canonicalisation of the rewritten targets, and both refusals.
- ADR 0013's "no new move mechanism" was a scope note for folder *creation*, and is superseded by this ADR: the tree now moves notes and folders by drag.
