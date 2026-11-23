# CLI reference

`tandem-cli` reads and writes the board from a shell or a script. It is the same executable as `tandem`: the installer links one name to the other, so `tandem doctor` and `tandem-cli doctor` are the same command. The words used here (ticket, ref, project, label, status, block) are explained in [concepts](concepts.md).

Agents normally use the [MCP tools](mcp.md) instead. Every agent verb below has an MCP tool with the same rules and the same answer.

## Conventions

- **Output.** Every verb prints one JSON object on stdout. Errors go to stderr as one line of plain text.
- **Exit codes.** `0` success, `1` the request was refused or failed, `2` a usage error. See [configuration](configuration.md#exit-codes).
- **Tickets** are named with `--id`, which takes a ref (`TAN-42`, any case) or a UUID.
- **Reading from stdin.** A flag documented as `<string|->` reads its value from stdin when given `-`, for long or multi-line text.
- **`self`.** Where a flag takes `self`, it means the name in `TANDEM_AGENT`. It is refused when `TANDEM_AGENT` is unset.
- **Empty string.** `--assignee ""` clears the assignee. On `ticket list`, `--assignee ""` and `--label ""` select the tickets with no assignee or no label.
- **Help.** `tandem-cli --help` prints every verb; `--help` after any verb or group prints that one's usage, such as `tandem-cli ticket set --help`.
- **Home.** Every verb takes `--home <path>`. See [configuration](configuration.md#the-home).
- **Remote.** With `TANDEM_SERVER` set, verbs run against a running server. See [remote target](#remote-target).

## Synopsis

Every agent verb at a glance. `tandem-cli --help` prints the same text. Each verb is detailed below.

```sh
tandem-cli ticket list [--filter open|done|all] [--status <value[,value]>]
                       [--assignee <name|self>] [--project <name>] [--label <name>]
                       [--text <string>]
                       [--archived] [--blocked]
tandem-cli ticket show --id <id>
tandem-cli ticket add --title <string> [--status <value>] [--assignee <name|self>]
                      [--project <name>] [--label <name>] [--origin <name|self>]
                      [--description <string|->]
tandem-cli ticket set --id <id> [--title <string>] [--status <value>]
                      [--assignee <name|self>] [--description <string|->]
                      [--append-description <string|->]
tandem-cli ticket move --id <id> [--label <name>] [--index <number>]
tandem-cli ticket block --id <id> --on <id>
tandem-cli ticket unblock --id <id> [--on <id>]
tandem-cli ticket complete --id <id>
tandem-cli ticket archive [--id <id>]
tandem-cli ticket comment add --id <id> [--author <name>] --body <string|->
tandem-cli ticket comment list --id <id>

tandem-cli label list
tandem-cli label add --name <name> --project <name>
tandem-cli label rename --from <name> --to <name> [--project <name>]
tandem-cli label delete --name <name>
tandem-cli label move --name <name> [--index <number>]
tandem-cli project list
tandem-cli project add --name <name> [--abbr <key>]
tandem-cli project rename --from <name> --to <name> [--abbr <key>]
tandem-cli project delete --name <name>

tandem-cli note rename --from <path> --to <path>
tandem-cli note stats [--project <name>]
tandem-cli note links
tandem-cli doctor
```

## Example

A short session:

```sh
$ tandem-cli project add --name Website --abbr WEB
{"projects":[{"name":"Website","key":"WEB","labels":[]}]}
$ tandem-cli ticket add --title "Fix the login button" --project Website
{"archived":false,"assignee":null,...,"ref":"WEB-1","status":"backlog","title":"Fix the login button"}
$ echo "Root cause is the click handler." | tandem-cli ticket comment add --id WEB-1 --body -
{"at":"2026-11-21T13:34:42Z","author":"","body":"Root cause is the click handler."}
$ tandem-cli ticket complete --id WEB-1
```

## Tickets

### ticket list

Lists tickets in stored order (the order the board draws), as summaries.

```
tandem-cli ticket list [--filter open|done|all] [--status <value[,value]>]
                       [--assignee <name|self>] [--project <name>] [--label <name>]
                       [--text <string>] [--archived] [--blocked]
```

| Flag | Default | Meaning |
| --- | --- | --- |
| `--filter` | `open` | `open`, `done` or `all`. Archived tickets are left out of all three. |
| `--status` | any | One status, or several separated by commas: `--status ready,in-progress`. |
| `--assignee` | any | An exact name, `self`, or `""` for unassigned tickets. |
| `--project` | any | Only this project's tickets. |
| `--label` | any | Only this label's tickets, or `""` for tickets with no label. |
| `--text` | none | A substring of the title or description, ignoring case and accents. |
| `--archived` | off | Alone, lists the archive instead. With another filter, adds archived tickets to it. |
| `--blocked` | off | Only tickets waiting on an open blocker. Combines with every other flag. |

Answers `{"tickets": [...]}`. Each row carries every ticket field except the body, which is replaced by `descriptionLength`. Use `ticket show` for the description.

### ticket show

Shows one ticket whole, including its description.

```
tandem-cli ticket show --id <id>
```

Answers the ticket: `id`, `ref`, `number`, `title`, `description`, `project`, `label`, `status`, `assignee`, `origin`, `blocked`, `blockedBy`, `blockedReason`, `blocks`, `archived`, `done`, `link`, `createdAt` and `completedAt`. `blockedReason` is the text of the description's `## Blocked on` section, if it has one.

### ticket add

Files a new ticket, on top of its project.

```
tandem-cli ticket add --title <string> [--project <name>] [--label <name>] [--status <value>]
                      [--assignee <name|self>] [--origin <name|self>] [--description <string|->]
```

| Flag | Default | Meaning |
| --- | --- | --- |
| `--title` | required | One line. |
| `--project` | the label's project | The project. Required unless `--label` names a label, which implies it. With neither, the verb fails and lists the projects. |
| `--label` | none | A label in that project. |
| `--status` | `backlog` | Any status. |
| `--assignee` | none | A name, or `self`. |
| `--origin` | `human` | Who filed it: a name, or `self`. Never changes afterwards. |
| `--description` | none | The markdown body, or `-` for stdin. When given, it must not be empty. |

Answers the new ticket, as `ticket show` does.

### ticket set

Changes one ticket's fields in one write. Any flag you leave out leaves its field as it is.

```
tandem-cli ticket set --id <id> [--title <string>] [--status <value>] [--assignee <name|self>]
                      [--description <string|->] [--append-description <string|->]
```

| Flag | Meaning |
| --- | --- |
| `--title` | A new title. An empty title is refused. |
| `--status` | A new status. Setting a done ticket back to another status reopens it and clears `completedAt`. |
| `--assignee` | A name, `self`, or `""` to clear it. |
| `--description` | **Replaces** the whole body. `""` clears it; `-` reads stdin. |
| `--append-description` | Adds text after the body, separated by a blank line, and keeps the rest. `-` reads stdin. An empty addition is refused. |

`--description` and `--append-description` together are refused. Answers the ticket.

### ticket move

Moves a ticket to another label in its own project, or to another position in its label. The ref never changes.

```
tandem-cli ticket move --id <id> [--label <name>] [--index <number>]
```

| Flag | Default | Meaning |
| --- | --- | --- |
| `--label` | no label | The label to move the ticket to: a label of the ticket's own project. A label of another project is refused, and so is `""`. |
| `--index` | the end | The 0-based position inside the label. |

Answers the ticket.

> **Warning:** leaving out `--label` moves the ticket to **no label**, even when you only mean to reorder it. To change a ticket's position inside its label, name that label too: `tandem-cli ticket move --id TAN-42 --label frontend --index 0`. The `ticket_move` MCP tool behaves the same way when `label` is left out. This is a known issue and will change in a later release, so that leaving out the label keeps the current one.

### ticket block, ticket unblock

Records that a ticket waits on another, or stops waiting. See [blocks](concepts.md#blocks).

```
tandem-cli ticket block --id <id> --on <id>
tandem-cli ticket unblock --id <id> [--on <id>]
```

`--id` is the ticket that waits, and `--on` the ticket it waits on. `block` is refused, writing nothing, when the edge would point at the ticket itself, close a cycle, or wait on a ticket that is already done or archived. `unblock` with `--on` removes that one edge; without it, every edge the ticket waits on. Removing an edge that does not exist succeeds and changes nothing. Both answer the waiting ticket.

### ticket complete

Moves a ticket to `done`. Running it on a done ticket succeeds and changes nothing.

```
tandem-cli ticket complete --id <id>
```

### ticket archive

Archives one ticket, or every done ticket at once.

```
tandem-cli ticket archive [--id <id>]
```

With `--id`, answers that ticket. Without it, archives every done ticket and answers `{"archived": <count>}`. See [archive](concepts.md#archive).

### ticket comment add, ticket comment list

Adds a comment to a ticket's log, or lists the log in the order it was written.

```
tandem-cli ticket comment add --id <id> --body <string|-> [--author <name>]
tandem-cli ticket comment list --id <id>
```

`--body -` reads the comment from stdin. `--author` defaults to the run's actor (`TANDEM_AGENT` locally, the member or `X-Tandem-Agent` name through a server), and is empty when there is none. `add` answers the comment, `{"at", "author", "body"}`; `list` answers `{"comments": [...]}`.

## Projects

```
tandem-cli project list
tandem-cli project add --name <name> [--abbr <key>]
tandem-cli project rename --from <name> --to <name> [--abbr <key>]
tandem-cli project delete --name <name>
```

| Verb | Notes |
| --- | --- |
| `project list` | Every project with its key and its labels, in board order. Answers `{"projects": [{"name", "key", "labels"}]}`. |
| `project add` | A new, empty project. `--abbr` is its key: upper-case letters and digits, unique across projects. Default: the first three letters of the name (`Tandem` becomes `TAN`). Also creates the notes folder `projects/<name>/`; an existing folder that matches ignoring case is reused. |
| `project rename` | Renames a project. Its labels, tickets and refs move with it. `--abbr` changes the key, and is refused once the project has filed a ticket. Also moves the notes folder `projects/<old>/` to `projects/<new>/` and rewrites every `[[project:Old]]` link, keeping its fragment and alias. |
| `project delete` | Deletes an empty project. A project that still holds labels or tickets is refused, and the refusal names them. Its notes folder and notes are left alone; `note links` lists the folder as unmatched. |

Each write answers the project list. The board write commits first and the notes follow: when a folder could not be made or moved, or a link not rewritten (a name that is not one path component, a target folder that already exists, a lock), the answer carries a `note` naming what was left, and `note links` shows the leftovers as `missingFolders`, `unmatchedFolders` and broken typed links. The write itself stands.

## Labels

```
tandem-cli label list
tandem-cli label add --name <name> --project <name>
tandem-cli label rename --from <name> --to <name> [--project <name>]
tandem-cli label delete --name <name>
tandem-cli label move --name <name> [--index <number>]
```

| Verb | Notes |
| --- | --- |
| `label list` | Every label, empty ones included, in board order. Answers `{"labels": [...]}`. |
| `label add` | A new label in a project. `--project` is required. A name that is already a label is refused. |
| `label rename` | Renames a label; its tickets move with it. `--project` moves the label to another project, and is refused while the label holds any ticket. |
| `label delete` | Deletes a label. Its tickets lose the label and stay in their project with their refs. |
| `label move` | Moves a label to a 0-based position in the list; leaving out `--index` moves it to the end. |

Each write answers the label list. `label delete` also answers `moved`, the number of tickets that lost the label.

## Notes

These verbs act on the [notes](concepts.md#notes). Agents read and write individual notes through the `note_*` [MCP tools](mcp.md); there is no CLI verb that prints or writes a note's text, because the notes are plain files you can open directly.

```
tandem-cli note rename --from <path> --to <path>
tandem-cli note stats [--project <name>]
tandem-cli note links
```

| Verb | Notes |
| --- | --- |
| `note rename` | Renames or moves a note, rewriting every wikilink that points at it. Paths are relative to the notes root. Local only. |
| `note stats` | The notes-health readout, for every project or for one with `--project`. Answers `{"notes", "projects", "totals"}`; the old `bikeshed` field holds the same value as `notes` for one more release, with a warning on stderr. Local only. |
| `note links` | Checks the links between notes. Answers `{"broken", "orphans", "counts"}`. |

## doctor

Checks that the install and the home are healthy. It only reads.

```
tandem-cli doctor
```

Answers `{"ok", "checks": [{"name", "ok", "detail"}]}`, one check each for the home, the store, its integrity, the references between rows, disk space, the binary and schema versions, and the notes (`notes`). It exits `1` when any check fails, and the failing check's `detail` says what to do.

A home an older release left, with its notes still in `bikeshed/`, is reported in the `notes` row: green with a note that the next run moves the folder, or red when both folders hold notes (merge them into `notes/` and remove `bikeshed/`). `doctor` never moves anything itself. For one release the report also carries a `bikeshed` row, the deprecated name of `notes`, and `doctor` prints one warning on stderr: see [versions and compatibility](versioning.md#deprecated-now).

## export and import

These two are for the person who owns the home, not for agents: they have no MCP tool and no remote form.

```
tandem-cli export --out <path|->
tandem-cli import <path|-> [--max-bytes <n>]
```

`export` writes the home (the store and `notes/`) into one `.tar.gz`, readable only by you (mode `0600`). It refuses a path that exists rather than overwrite it; `--out -` writes to stdout. It needs no running server and is safe to run beside one. It prints the path and the size, and one record line (who, when, `door=cli`) on stderr.

`import` restores an archive into an **empty** home: a home that already holds a store, a backup or notes (under `notes/` or the older `bikeshed/`) is refused, with no merge and no force. `-` reads stdin, and `--max-bytes` caps the unpacked size. Nothing is written until every check passes. Stop `tandem serve` on that home first, and pass `--home <empty dir>` to restore somewhere new.

The archive is not encrypted and carries no sign-in session and no token. The board offers the same archive as a download (Export workspace in the menu, or `GET /export`). See [moving and restoring a home](store.md#moving-and-restoring-a-home).

## serve

Starts the board and the MCP endpoint. Its flags, environment variables and start-up checks are in [configuration](configuration.md#tandem-serve).

```
tandem serve [--host <ip>] [--port <port>] [--open] [--public-origin <origin>] [--allow-unauthenticated]
```

## Remote target

Set `TANDEM_SERVER` to the URL of a running `tandem serve`, and every verb runs against that server instead of the local store. Each verb becomes one MCP call to `<url>/mcp`, with the same stdout and the same exit codes as a local run.

```sh
export TANDEM_SERVER=https://board.example.com
export TANDEM_TOKEN=tdm_...   # a personal token, or the owner token
tandem-cli ticket list --status ready
```

- `TANDEM_TOKEN` is sent as `Authorization: Bearer`. The CLI refuses to send it over plain `http://` to a host that is not loopback, and never prints it.
- `TANDEM_AGENT` is sent as the `X-Tandem-Agent` header, so writes record the same actor as a local run.
- `--home` is ignored: the server's home is the one used.
- `serve`, `export`, `import`, `note rename` and `note stats` have no remote form and refuse, naming themselves.

## Background

[ADR 0047](adr/0047-serve-is-the-single-door.md) (one server for every session), [ADR 0050](adr/0050-the-home-leaves-and-arrives-as-one-archive.md) (export and import), [ADR 0053](adr/0053-one-prefix-per-project.md) (projects and refs), [ADR 0030](adr/0030-one-entity-the-ticket.md) (the ticket). What may change between releases: [versions and compatibility](versioning.md).
