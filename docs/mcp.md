# MCP reference

`tandem serve` is an [MCP](https://modelcontextprotocol.io) server. An agent that connects to it gets the tools below: read and write tickets, comments, projects and labels, and read and write the notes. The words used here are explained in [concepts](concepts.md). How to register the server with Claude Code, opencode or another host is in the README's [Connect your agent](../README.md#connect-your-agent).

## Connection

| | |
| --- | --- |
| Endpoint | `POST /mcp`, by default `http://127.0.0.1:7700/mcp` |
| Transport | Streamable HTTP. Every response is a single JSON body; the server sends no event stream on `/mcp`, and `GET /mcp` answers `405`. |
| Protocol | JSON-RPC 2.0. The server answers `initialize`, `tools/list` and `tools/call`. |
| `X-Tandem-Agent` | Optional. The name writes are recorded under: lowercase letters, digits and hyphens. Any other shape answers `400`. See [actors](concepts.md#actors). |
| `Authorization` | `Bearer <token>`, required when the server is not on loopback: the owner token or a member's personal token. See [self-hosting](self-hosting.md). |
| `Origin` | A browser origin is refused `403`, unless it is the server's own address or exactly its `--public-origin`. |

The `initialize` answer carries a `health` object beside the usual fields: the server's version, its home, whether the store opened and how many tools it serves.

## Calling a tool

A call names the tool and passes its parameters as `arguments`:

```sh
curl -s http://127.0.0.1:7700/mcp \
  -H 'Content-Type: application/json' \
  -H 'X-Tandem-Agent: docs-bot' \
  -d '{"jsonrpc": "2.0", "id": 1, "method": "tools/call",
       "params": {"name": "ticket_add",
                  "arguments": {"title": "Fix the login button", "project": "Website", "detail": "lean"}}}'
```

The answer is one text item holding a JSON document: the same document the matching [CLI](cli.md) verb prints, plus `revision`, the store's revision after the call.

```json
{"jsonrpc": "2.0", "id": 1, "result": {"content": [{"type": "text",
  "text": "{\"revision\":3,\"ticket\":{\"ref\":\"WEB-1\",\"status\":\"backlog\",\"title\":\"Fix the login button\",...}}"}]}}
```

## Errors

There are two kinds, and an agent should handle both.

- **The call was refused or its arguments were wrong**: an unknown ticket, a block that would close a cycle, a project that still holds tickets, a missing required parameter, a value of the wrong type. The answer is a normal `result` with `"isError": true` and the reason as text, written so the agent can correct itself:

  ```json
  {"jsonrpc": "2.0", "id": 2, "result": {"isError": true,
    "content": [{"type": "text", "text": "Unknown ticket id \"NOPE-1\". Here is the current open list to pick the right one from: ..."}]}}
  ```

- **The request itself was wrong**: a JSON-RPC `error` instead of a `result`.

  | Code | Meaning |
  | --- | --- |
  | `-32601` | Unknown method, such as `tools/bogus`. |
  | `-32602` | Invalid params: an unknown tool, `arguments` that is not a JSON object, or a `detail` other than `lean` or `full`. |

  ```json
  {"jsonrpc": "2.0", "id": 3, "error": {"code": -32602, "message": "Invalid arguments for tool: ticket_list: detail is \"lean\" or \"full\""}}
  ```

HTTP-level refusals come before the tool runs: `400` for a malformed `X-Tandem-Agent`, `401` for a missing, wrong or revoked token, and `403` for a foreign origin, a personal token naming someone else, or a credential without the permission. A `403` for a missing permission carries a JSON-RPC error body with code `-32001`.

## Lean answers

The ticket tools answer the whole ticket by default, every field as the CLI prints it. Pass `detail: "lean"` so an agent does not pay again for text it already holds; `detail: "full"` is the same as leaving it out.

- **Writes** (`ticket_add`, `ticket_set`, `ticket_move`, `ticket_block`, `ticket_unblock`, `ticket_complete`, and `ticket_archive` with an id) answer `{revision, ticket: {ref, id, status, assignee, label, blocked}}`, with `title` when the write could have changed it, and never the description.
- **`ticket_comment_add`** answers `{revision, at, author}`, not the body.
- **`ticket_list` rows and `ticket_show`** omit a field that is null, false or an empty array, drop `done`, keep `id` only for a ticket with no ref, and name a blocker as `{ref, status}`.

`ticket_list` also takes `limit`, and reports `total` when it cut the list. `limit: 0` returns no rows and the total, to count matches cheaply.

## Access hints

Each tool carries MCP annotations a host can use to decide what to run without asking the human: **read-only** tools change nothing; **destructive** tools remove or overwrite something (a label, a note's text, a blocking edge); **idempotent** tools can be repeated safely. Each tool below states its hints.

## Tools

The section below is generated from the server's registry (`tools()` in `rust/core/src/mcp.rs`), and a test (`rust/core/tests/mcp_doc.rs`) fails when the page and the registry disagree. After a tool change, regenerate it with `TANDEM_BLESS=1 cargo test -p tandem-core --test mcp_doc`.

<!-- tools:begin -->
### Tickets

#### `ticket_list`

List the tickets in view, in stored order. Every filter is optional and they combine. Rows carry every field; detail "lean" omits null, false and empty fields.

Read-only.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `archived` | boolean | no | alone it is the archive; with a filter, the archive joins it |
| `assignee` | string | no | an exact name, self for your own, or "" for the unowned |
| `blocked` | boolean | no | only the tickets waiting on an open blocker |
| `detail` | string: `lean`, `full` | no | full (default) returns the whole ticket, every field; lean omits what you already hold and empty fields |
| `filter` | string: `open`, `done`, `all` | no | open (default), done or all |
| `label` | string | no | a label name, or "" for the tickets with no label |
| `limit` | integer | no | at most this many rows from the top; total reports the matches when cut; 0 returns no rows and the total |
| `project` | string | no | a project name |
| `status` | string | no | comma-separated values, e.g. ready,in-progress |
| `text` | string | no | a case- and accent-insensitive substring of the title or description |

#### `ticket_show`

Read one ticket whole: the only read that carries the description. Answers every field; detail "lean" omits null, false and empty fields.

Read-only.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `detail` | string: `lean`, `full` | no | full (default) returns the whole ticket, every field; lean omits what you already hold and empty fields |
| `id` | string | yes | the ticket's UUID or its ref (the project key and number, TAN-42) |

#### `ticket_add`

File a ticket. A ticket always lives in a project: name `project`, or a `label` alone implies its project; with neither it is refused and the projects are listed. A title and a project land backlog, unowned, unlabelled, on top of the project. Answers the whole ticket; detail "lean" answers a short receipt without the description.

Writes.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `assignee` | string | no | a name, self to claim it, or human for the human |
| `description` | string | no | the markdown body; no flag is no body |
| `detail` | string: `lean`, `full` | no | full (default) returns the whole ticket, every field; lean omits what you already hold and empty fields |
| `label` | string | no | an optional label inside the project; it must exist |
| `origin` | string | no | who filed it, self for this agent; written at add, never changed |
| `project` | string | no | the project the ticket lives in; required unless label is given, which implies its project |
| `status` | string: `backlog`, `on-hold`, `ready`, `in-progress`, `in-review`, `done` | no | backlog (default), on-hold, ready, in-progress, in-review or done |
| `title` | string | yes | one line, trimmed; required |

#### `ticket_set`

Change the fields named in one write; everything unnamed is left alone. To add to the description send appendDescription (the body is kept); description replaces the whole body, and the two cannot be sent together. Answers the whole ticket; detail "lean" answers a short receipt without the description.

Writes. Idempotent: repeating it changes nothing more.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `appendDescription` | string | no | text appended to the body after a blank line; the existing body is kept |
| `assignee` | string | no | a name, self, human, or "" to clear the owner |
| `description` | string | no | the markdown body, replacing the old one, or "" to clear it |
| `detail` | string: `lean`, `full` | no | full (default) returns the whole ticket, every field; lean omits what you already hold and empty fields |
| `id` | string | yes | the ticket's UUID or its ref |
| `status` | string: `backlog`, `on-hold`, `ready`, `in-progress`, `in-review`, `done` | no | backlog, on-hold, ready, in-progress, in-review or done |
| `title` | string | no | one line, trimmed; an empty title is refused |

#### `ticket_move`

Move between labels of the ticket's own project (or to no label) and/or within one; the ref never changes, and a label of another project is refused. Omit the index to append. Answers the whole ticket; detail "lean" answers a short receipt.

Writes.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `detail` | string: `lean`, `full` | no | full (default) returns the whole ticket, every field; lean omits what you already hold and empty fields |
| `id` | string | yes | the ticket's UUID or its ref |
| `index` | integer | no | 0-based within the label; omit to append |
| `label` | string | no | a label of the ticket's own project, or "" for no label; it must exist |

#### `ticket_block`

Record that one ticket waits on another, as an edge in the store. Answers the whole ticket; detail "lean" answers a short receipt.

Writes. Idempotent: repeating it changes nothing more.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `detail` | string: `lean`, `full` | no | full (default) returns the whole ticket, every field; lean omits what you already hold and empty fields |
| `id` | string | yes | the ticket's UUID or its ref |
| `on` | string | yes | the ticket it waits on, as a UUID or a ref |

#### `ticket_unblock`

Remove one edge, or every edge the ticket has when `on` is absent. Answers the whole ticket; detail "lean" answers a short receipt.

Writes. Destructive: it removes or overwrites.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `detail` | string: `lean`, `full` | no | full (default) returns the whole ticket, every field; lean omits what you already hold and empty fields |
| `id` | string | yes | the ticket's UUID or its ref |
| `on` | string | no | the edge to remove; omit to remove every edge the ticket has |

#### `ticket_complete`

The short path to done: idempotent, no reopen verb. Answers the whole ticket; detail "lean" answers a short receipt.

Writes. Idempotent: repeating it changes nothing more.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `detail` | string: `lean`, `full` | no | full (default) returns the whole ticket, every field; lean omits what you already hold and empty fields |
| `id` | string | yes | the ticket's UUID or its ref (the project key and number, TAN-42) |

#### `ticket_archive`

Archive one ticket, or every done ticket when `id` is absent. One ticket answers the whole ticket; detail "lean" answers a short receipt.

Writes. Destructive: it removes or overwrites.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `detail` | string: `lean`, `full` | no | full (default) returns the whole ticket, every field; lean omits what you already hold and empty fields |
| `id` | string | no | the ticket's UUID or its ref; omit to archive every done ticket |

#### `ticket_comment_add`

Append to the ticket's discussion log. Write the role out as the author; left out, it is the caller's own name (a member's token, or the X-Tandem-Agent name). Echoes the comment; detail "lean" answers the time and author, not the body.

Writes.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `author` | string | no | who signs the line; write the role out |
| `body` | string | yes | the comment body; an empty body is refused |
| `detail` | string: `lean`, `full` | no | full (default) returns the whole ticket, every field; lean omits what you already hold and empty fields |
| `id` | string | yes | the ticket's UUID or its ref |

#### `ticket_comment_list`

List the ticket's discussion log in append order.

Read-only.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | string | yes | the ticket's UUID or its ref (the project key and number, TAN-42) |

### Labels

#### `label_list`

List every label, in render order.

Read-only.

No parameters.

#### `label_add`

Declare a label inside a project (project is required). A label must exist before tickets move to it. Creates the notes folder projects/<project>/<label>/ (none when the label is named like its project); a folder that could not be made is a note on the answer.

Writes.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | string | yes | the label name; required |
| `project` | string | yes | the parent project; required |

#### `label_rename`

Rename a label, rewriting every ticket in it in one write. A new project is refused while the label holds tickets. Moves its notes folder, and rewrites [[label:...]] links; what could not be moved is a note on the answer and shows in note_health.

Writes.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `from` | string | yes | the label's current name |
| `project` | string | no | the new parent project; omit to keep the label's project; refused while the label holds tickets |
| `to` | string | yes | the new name |

#### `label_delete`

Delete a label; its tickets lose the label and stay in their project with their refs, never deleted.

Writes. Destructive: it removes or overwrites.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | string | yes | the label name |

#### `label_move`

Reorder the declared labels; the index is 0-based and clamps.

Writes.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `index` | integer | no | 0-based in the label list; omit to append |
| `name` | string | yes | the label name |

### Projects

#### `project_list`

List every project with its key and labels, in render order.

Read-only.

No parameters.

#### `project_add`

Declare a project and the ref key it issues (abbr; omitted, the first three letters of the name). A project must exist before labels or tickets name it. Creates the notes folder projects/<name>/; a folder that could not be made is a note on the answer.

Writes.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `abbr` | string | no | the ref key it issues; omit for the first three alphanumerics |
| `name` | string | yes | the project name; required |

#### `project_rename`

Rename a project, moving its labels and tickets with it; abbr moves the key until the project has issued a number. Moves the notes folder projects/<old>/ and rewrites [[project:...]] links; what could not be moved is a note on the answer and shows in note_health.

Writes.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `abbr` | string | no | the new ref key; refused once the project has issued a number |
| `from` | string | yes | the project's current name |
| `to` | string | yes | the new name |

#### `project_delete`

Delete an empty project; refused while labels or tickets belong to it.

Writes. Destructive: it removes or overwrites.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | string | yes | the project name |

### Diagnostics

#### `doctor_report`

Report where this machine's Tandem state lives and whether it is healthy.

Read-only.

No parameters.

### Notes

#### `note_tree`

Read the vault's folder tree: folders first, notes after, hidden entries skipped.

Read-only.

No parameters.

#### `note_read`

Read one note: its whole text, the content hash a save compares and its mtime.

Read-only.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `path` | string | yes | the path relative to the notes root |

#### `note_write`

Write a note's whole text, atomically, under the vault lock. A missing text is refused. Pass expectedHash from your note_read (or "absent" for a new note): the write is then refused if the note changed since you read it.

Writes. Destructive: it removes or overwrites.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `expectedHash` | string | no | the hash note_read returned; the write is refused if the note changed since. Use "absent" for a note that does not exist yet. Omit to overwrite |
| `path` | string | yes | the path relative to the notes root |
| `text` | string | yes | the note's whole text; a missing text is refused |

#### `note_edit`

Replace one passage of a note without rewriting it, atomically, under the vault lock. oldText must occur exactly once: zero or several matches are refused and nothing changes. An empty oldText appends newText at the end of the note (a note that already ends in newText is left as it is). Answers the path and the new hash, never the text.

Writes. Destructive: it removes or overwrites.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `expectedHash` | string | no | the hash note_read returned; the edit is refused if the note changed since. Omit to edit the note as it is |
| `newText` | string | yes | the replacement, or the text to append |
| `oldText` | string | yes | the text to replace; it must occur exactly once in the note, else the edit is refused and nothing changes. Empty appends newText at the end of the note (a note that already ends in newText is left as it is) |
| `path` | string | yes | the path relative to the notes root |

#### `note_search`

Find notes whose title or body contains the query, case-insensitive: one row per note with its path and the title or the snippet around the match. limit defaults to 20; total reports the matches when cut.

Read-only.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `limit` | integer | no | at most this many rows, default 20; total reports the matches when cut; 0 returns no rows and the total |
| `query` | string | yes | text to find, case-insensitive, in a note's title or body; empty answers no rows |

#### `note_backlinks`

List the notes that link to a note: one row each, with the line of the first link.

Read-only.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `path` | string | yes | the path relative to the notes root |

#### `note_create`

Create an empty note. A taken path is refused, never overwritten.

Writes.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `path` | string | yes | the path relative to the notes root |

#### `note_create_folder`

Create a folder, with no note in it.

Writes.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `path` | string | yes | the path relative to the notes root |

#### `note_move`

Move a note or folder into another folder, rewriting every wikilink that resolves to it.

Writes. Destructive: it removes or overwrites.

| Parameter | Type | Required | Description |
| --- | --- | --- | --- |
| `into` | string | yes | the folder it moves into; the root is "" |
| `path` | string | yes | the note or folder that moves |

#### `note_health`

Check the vault's link graph: broken links (typed project: and label: links included), orphans, folders under projects/ that match no project or label, board projects and labels with no folder (missingFolders), and counts, the same report note links prints.

Read-only.

No parameters.
<!-- tools:end -->
