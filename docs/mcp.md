# MCP tools

`tandem serve` is the MCP server: agents reach it at `POST /mcp`. How to connect a host is in the README's [Connect your agent](../README.md#connect-your-agent). Tools marked read-only in `tools/list` may run without asking the human; the rest write.

## Lean answers

The ticket tools answer the whole shape by default, every field as the CLI prints it. Pass `detail: "lean"` so an agent does not pay again for text it already holds; `detail: "full"` is the same as leaving it out, and any other value is an invalid-params error. Lean, a write (`ticket_add`, `ticket_set`, `ticket_move`, `ticket_block`, `ticket_unblock`, `ticket_complete`, and `ticket_archive` with an id) answers `{revision, ticket: {ref, id, status, assignee, label, blocked}}`, with `title` added when the write could have changed it, and never the description. Lean, `ticket_comment_add` answers `{revision, at, author}`, not the body. Lean, `ticket_list` rows and `ticket_show` omit a field that is null, false or an empty array, drop `done`, keep `id` only for a ticket with no ref, and name a blocker as `{ref, status}`.

`ticket_list` also takes `limit` and reports `total` when it cut the list. `limit: 0` is valid: it returns no rows and the total, so a caller can count matches without paying for rows. The remote CLI sends `detail: "full"` explicitly, so `TANDEM_SERVER` stdout is the local verb's stdout whatever the default.

The list below is generated from `tools()` in `rust/core/src/mcp.rs`, one line per tool with its parameter names. A test (`rust/core/tests/mcp_doc.rs`) fails when this page and the registry disagree. After a tool change, regenerate with `TANDEM_BLESS=1 cargo test -p tandem-core --test mcp_doc`.

<!-- tools:begin -->
- `ticket_list`: List the tickets in view, in stored order. Every filter is optional and they combine. Rows carry every field; detail "lean" omits null, false and empty fields. Params: `archived`, `assignee`, `blocked`, `detail`, `filter`, `label`, `limit`, `status`, `text`.
- `ticket_show`: Read one ticket whole: the only read that carries the description. Answers every field; detail "lean" omits null, false and empty fields. Params: `detail`, `id` (required).
- `ticket_add`: File a ticket. A title alone lands backlog, unowned, on top of the default area. Answers the whole ticket; detail "lean" answers a short receipt without the description. Params: `assignee`, `description`, `detail`, `label`, `origin`, `status`, `title` (required).
- `ticket_set`: Change the fields named in one write; everything unnamed is left alone. To add to the description send appendDescription (the body is kept); description replaces the whole body, and the two cannot be sent together. Answers the whole ticket; detail "lean" answers a short receipt without the description. Params: `appendDescription`, `assignee`, `description`, `detail`, `id` (required), `status`, `title`.
- `ticket_move`: Move between labels and/or within one. Omit the index to append. Answers the whole ticket; detail "lean" answers a short receipt. Params: `detail`, `id` (required), `index`, `label`.
- `ticket_block`: Record that one ticket waits on another, as an edge in the store. Answers the whole ticket; detail "lean" answers a short receipt. Params: `detail`, `id` (required), `on` (required).
- `ticket_unblock`: Remove one edge, or every edge the ticket has when `on` is absent. Answers the whole ticket; detail "lean" answers a short receipt. Params: `detail`, `id` (required), `on`.
- `ticket_complete`: The short path to done: idempotent, no reopen verb. Answers the whole ticket; detail "lean" answers a short receipt. Params: `detail`, `id` (required).
- `ticket_archive`: Archive one ticket, or every done ticket when `id` is absent. One ticket answers the whole ticket; detail "lean" answers a short receipt. Params: `detail`, `id`.
- `ticket_comment_add`: Append to the ticket's discussion log. Write the role out as the author; left out, it is the caller's own name (a member's token, or the X-Tandem-Agent name). Echoes the comment; detail "lean" answers the time and author, not the body. Params: `author`, `body` (required), `detail`, `id` (required).
- `ticket_comment_list`: List the ticket's discussion log in append order. Params: `id` (required).
- `label_list`: List every label, in render order. Params: none.
- `label_add`: Declare a label. A label must exist before tickets move to it. Params: `abbr`, `name` (required), `project`.
- `label_rename`: Rename a label, rewriting every ticket in it in one write. Params: `abbr`, `from` (required), `project`, `to` (required).
- `label_delete`: Delete a label; members fall back to the default area, never deleted. Params: `name` (required).
- `label_move`: Reorder the declared labels; the index is 0-based and clamps. Params: `index`, `name` (required).
- `project_list`: List every project with its labels, in render order. Params: none.
- `project_add`: Declare a project. A project must exist before labels name it as parent. Params: `name` (required).
- `project_rename`: Rename a project, moving its labels with it. Params: `from` (required), `to` (required).
- `project_delete`: Delete an empty project; refused while labels belong to it. Params: `name` (required).
- `doctor_report`: Report where this machine's Tandem state lives and whether it is healthy. Params: none.
- `note_tree`: Read the vault's folder tree: folders first, notes after, hidden entries skipped. Params: none.
- `note_read`: Read one note: its whole text, the content hash a save compares and its mtime. Params: `path` (required).
- `note_write`: Write a note's whole text, atomically, under the vault lock. A missing text is refused. Pass expectedHash from your note_read (or "absent" for a new note): the write is then refused if the note changed since you read it. Params: `expectedHash`, `path` (required), `text` (required).
- `note_edit`: Replace one passage of a note without rewriting it, atomically, under the vault lock. oldText must occur exactly once: zero or several matches are refused and nothing changes. An empty oldText appends newText at the end of the note (a note that already ends in newText is left as it is). Answers the path and the new hash, never the text. Params: `expectedHash`, `newText` (required), `oldText` (required), `path` (required).
- `note_search`: Find notes whose title or body contains the query, case-insensitive: one row per note with its path and the title or the snippet around the match. limit defaults to 20; total reports the matches when cut. Params: `limit`, `query` (required).
- `note_backlinks`: List the notes that link to a note: one row each, with the line of the first link. Params: `path` (required).
- `note_create`: Create an empty note. A taken path is refused, never overwritten. Params: `path` (required).
- `note_create_folder`: Create a folder, with no note in it. Params: `path` (required).
- `note_move`: Move a note or folder into another folder, rewriting every wikilink that resolves to it. Params: `into` (required), `path` (required).
- `note_health`: Check the vault's link graph: broken links, orphans and counts, the same report note links prints. Params: none.
<!-- tools:end -->
