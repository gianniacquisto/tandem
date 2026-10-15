# 04 - Vault protocol: file rules, frontmatter, conflicts, deletion

Type: grilling
Status: resolved

## Question

Pin down the on-disk protocol for the bikeshed — the notes analogue of ADR 0001 (store protocol) for todos — so the spec has one protocol to follow. High level is settled: plain `.md` files, `~/.tandem/bikeshed/`, filename = note identity, nested folders, no metadata dir, agents edit files directly, the app watches the folder live.

Nail down:

1. **File naming**: allowed characters (spaces? `#`? leading dots?), extension casing (`.md` only?), and what the app does with non-`.md` files (attachments/images — ignored? shown in the tree?) and hidden files.
2. **Frontmatter**: are YAML frontmatter blocks in notes tolerated (agents will write them)? Stripped in preview, rendered, or passed through as raw?
3. **Metadata**: created/modified come from file mtime — shown anywhere (tree, editor footer) or not at all?
4. **Save semantics**: does the editor autosave (debounced write on change) or save explicitly? What does the UI show about unsaved state?
5. **External-change conflict policy**: the user has a note open with unsaved edits and the file changes on disk (agent edit) — the analogue of todos' lenient-read / strict-write. Overwrite, keep, reload, or notify?
6. **Deletion**: UI delete = delete the file — to Trash or permanent? An agent deleting a file = the note simply disappears from the tree — confirm that is the whole protocol.
7. **Rename mechanism**: the rename *behavior* (whether links follow) belongs to the wikilink-model ticket; this ticket defines how the rename write itself is performed safely (temp + rename? same-directory atomicity?) and how it composes with any link rewriting.

> Cross-ticket resolution (from [03-wikilink-model](03-wikilink-model.md)): the wikilink grammar splits targets at the first `#` and first `|` — the app never creates names containing either (item 1's `#` reservation extends to `|` for the same reason). A note on disk whose name contains `#` or `|` (agent-created) is unnameable by wikilink: it parses as fragment/alias and resolves elsewhere or breaks. Wikilinks inside YAML frontmatter are not rewritten on rename (item 2: frontmatter stays opaque).

Deliverable: a normative protocol section for the spec, plus a draft ADR if the decisions meet the ADR bar (hard to reverse, surprising without context, real trade-off).

## Answer

Settled 2026-10-15 (grilling), reconciled same day with the resolved wikilink-model ticket. Normative protocol for the spec; the locking/rename decisions and their rationale live in [ADR 0004](../../../docs/adr/0004-vault-protocol.md).

1. **Identity & naming** — a note is any file ending `.md` (case-insensitive). The app creates only names free of `/`, `#`, and `|` (spaces fine); the UI rejects `#` or `|` on create/rename (the wikilink grammar splits targets on both — ticket 03). Violating files that exist on disk are still shown and edited — the rule constrains creation, not tolerance. Hidden files are ignored everywhere. Non-`.md` files are **attachments**: invisible in the tree, reachable only by relative path from the referencing note.
2. **Frontmatter** — tolerated, opaque. Only a *leading, properly terminated* `---` block counts; it's stripped from live preview, untouched in source. The app never parses, validates, or edits it (a rename's link rewrite never touches wikilinks inside it — that boundary is the wikilink model's, not frontmatter processing). Malformed or non-leading = content.
3. **Metadata** — nothing displayed in v1. mtime/birthtime stay the on-disk truth, used internally (watcher, conflict detection).
4. **Save** — autosave: debounced ~1 s after the last keystroke, plus on note switch, window blur, and ⌘S. No persistent unsaved badge; at most a transient "saving…". A failed write keeps the buffer, surfaces the error, and retries.
5. **External changes** — the buffer tracks the on-disk hash. Clean buffer → reload silently. Dirty buffer → never clobber: banner with **[Overwrite with mine] / [Take theirs]**; a pending autosave is withheld and the banner shown. The app recognises its own writes by the hash it just wrote.
6. **Deletion** — UI delete (note or folder) moves to the macOS Trash, recoverable; no tombstone; backlinks become broken links per the wikilink model. Agent deletion is permanent and unmediated — the spec states that asymmetry outright.
7. **Locking & rename mechanics** — app writes take an exclusive `flock` on `~/.tandem/bikeshed.lock` (outside the vault, never renamed) held across the whole write, multi-file operations included; readers take no lock. Every write: same-directory temp + fsync + rename; renames are plain `rename(2)`. Rename-with-link-rewrite runs under the lock in one transaction, in the order the wikilink model settled: rewrite the referencing notes (atomic per file), then rename the note. No journal — a mid-sequence crash yields no torn files and no data loss, worst case some broken links. The lock serializes app-vs-app only; agents and external editors bypass it by design.
8. **First launch** — a missing folder is an empty vault; the app creates it lazily (at launch or first write), never an error state, and seeds no welcome note.

Cross-references:

- Rename *behavior* (whether wikilinks follow a rename) is owned by [03 - Wikilink model](03-wikilink-model.md); this protocol defines only the write mechanics.
- Naming constraint settled by that same ticket: `#` and `|` are forbidden in created note names because the wikilink grammar splits targets on them; pre-existing violators on disk are still shown and edited (item 1).
