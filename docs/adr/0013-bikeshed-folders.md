# Bikeshed folders: in-app creation, plain directories

[ADR 0004](0004-vault-protocol.md) settled the vault as a plain folder of `.md` notes with **nested folders allowed**: the tree shows folders first, the watcher recurses, and wikilink resolution runs against the full recursive note set. Everything downstream already understands folders — but the app had no way to *create* one. The human could only structure the vault from Finder, or by letting the create-a-broken-link path make a parent folder as a side effect.

We decided: **the app gains explicit folder creation**, so the human can lay out project folders (`/projects/<project>/…`) from the GUI. Folders stay a pure filesystem concept — no bespoke metadata, no manifest — and creation validates the name with the **same hygiene rule as a note name, minus the `.md` requirement**.

This reverses no decision. ADR 0004 already permits nesting; it constrains only *names*, not the folder/file distinction, and this ADR adds the missing creator rather than changing a rule. Wikilink resolution is live and by basename/path, so an empty folder is harmless — it resolves to nothing and breaks nothing.

Scope is deliberately narrow: **creation only**. The GUI exposes it in two places — the sidebar `+` menu and a folder row's context menu — and introduces no new move mechanism: moving a note stays the existing Rename… path change (ADR 0005).

## Why

- **The structure the todo asks for is only a folder tree.** "Projects structured per project with notes inside" is `/projects/<project>/…` — exactly the nesting ADR 0004 already makes safe.
- **The core already models folders everywhere.** `list()` recurses, the watcher recurses, `delete` is folder-aware, and `create` / `write` / `rename` already create missing parents. The only gap was an operation that makes a directory *without* a note.
- **No new stored state, so no migration.** A folder is a directory on disk; nothing in the store format, the lock, or the watcher changes, and the app keeps reading a vault an agent wrote by hand.
- **One hygiene rule, two entry points.** Folder names reuse `validateNewName`'s component rule (no empty / `.` / `..` component, no hidden component, no `#` or `|` — the wikilink grammar's separators), dropping only the trailing-`.md` check. A folder has no extension, so requiring one would be wrong; every other rule still guards the same wikilink grammar and the hidden-file-invisibility invariant.
- **Creation is a small, safe primitive.** It validates before the lock, refuses a taken target (a file *or* a folder) and an ancestor that is a file as `alreadyExists` (never an overwrite), then creates the directory with intermediates under the vault lock — the shape of `create`, minus the note.

## Rejected

- **A folder-rename or folder-move operation.** The core's `rename` rewrites links and is deliberately notes-only (`!oldIsDir`); generalising it to directories raises questions (rewrite links under the whole subtree?) out of proportion to the todo. A note already moves by the Rename… sheet, because rename *is* the path change — so the missing folder primitive is creation alone.
- **A vault manifest or `.bikeshed/` metadata file for structure.** Folders are the structure; a parallel index would be a second source of truth for something the filesystem already expresses (ADR 0004's "the filesystem is the notes interface").
- **Auto-creating a folder whenever a broken link is clicked.** That path already creates parent folders for a note; the human asked for an explicit way to make an *empty* folder, which only matters as a deliberate act.
- **A hidden-file marker to keep empty folders out of the tree.** Empty folders are harmless (resolution is live and by name), and hiding them would hide the very structure the change exists to make.
- **An always-on "New Folder" row with an inline rename field.** A text-entry affordance inside the tree is a second editing surface; the collision-free auto-name plus the (out-of-scope) future rename keeps this change to one operation.

## Consequences

- `Bikeshed.createFolder(path:)` joins the public operations: validate, refuse collisions, `createDirectory(withIntermediateDirectories:)` under the lock; returns the created path; writes no `.md` file.
- `Bikeshed.validateNewFolderName` is the note rule minus `.md`, sharing one private component check with `validateNewName` (ADR 0008's "one rule, not copies").
- `BikeshedModel.createFolder(in:)` derives a collision-free `New Folder` / `New Folder 2` …, reloads the tree, and expands the parent so the new folder is visible — mirroring `createNote(in:)`.
- The sidebar header's `+` becomes a small menu (**New note** / **New folder**); a folder row's context menu gains **New folder** beside **New note** and **Delete**. The note row is unchanged.
- The `+` menu does not open the new folder or select it — an empty folder has nothing to open; the model only expands the parent so the new row is visible.
- Folder creation is unit-tested in `TandemCoreTests` (nested creation in `list()`, collisions, name validation, ancestor-is-a-file, no note written); folder rename and move remain out of scope, and notes continue to move via the path rename (ADR 0005).
