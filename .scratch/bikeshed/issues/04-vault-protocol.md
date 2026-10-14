# 04 - Vault protocol: file rules, frontmatter, conflicts, deletion

Type: grilling
Status: open

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

Deliverable: a normative protocol section for the spec, plus a draft ADR if the decisions meet the ADR bar (hard to reverse, surprising without context, real trade-off).
