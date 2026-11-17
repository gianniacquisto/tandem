# The store

`~/.tandem/tandem.db` is the store: one SQLite database holding the tickets and their labels, the comment logs and the projects. The server watches it, so the board updates live whenever anything changes it, including an agent. There is no second source of truth and no sidecar file to keep in step; the bikeshed's notes stay plain markdown beside it.

The tickets are rows, in stored order - `tickets.position` **is** the list order, new tickets land on top, and the order is never recomputed from `createdAt`. Each ticket's `label` names one of the `labels` rows, and a null label is the default (ungrouped) area, which always comes first. See [ADR 0031](adr/0031-one-database-the-store.md) for the decision, [ADR 0006](adr/0006-stored-order-and-sections.md) for order and labels, and [ADR 0030](adr/0030-one-entity-the-ticket.md) for the vocabulary.

The schema is versioned in the database itself: `schema_migrations` holds one row per migration - a version and when it was applied - so "which shape is this store?" is a query, and a migration that fails rolls back whole rather than leaving the store half-upgraded. `tandem doctor` prints the version beside the open and done counts.

## What lives in the home

| path | what it is | safe to delete? |
| --- | --- | --- |
| `tandem.db` | the store | no: this is your data |
| `tandem.db-wal`, `tandem.db-shm` | SQLite's write-ahead log and its index, present while the store is in use | only together with a restore (below), and only with `tandem serve` stopped |
| `tandem.db.bak` | the rolling backup: the store as it was before the last changed write | yes, but it is your only backup |
| `.tandem-*.tmp` | a backup that was being written when a process died | yes; the next write sweeps any older than a minute |
| `bikeshed/` | the notes, plain markdown | no: this is your data |
| `bikeshed.lock` | the lock the note writes take; it holds nothing | yes, when nothing is running |

The home is `~/.tandem` unless `TANDEM_HOME` or `--home` names another. On Windows it is `%USERPROFILE%\.tandem`.

## The backup

Beside the store, every changed write keeps **one previous complete copy** in `tandem.db.bak`, taken through SQLite's backup API before the write touches the database. There is one generation, no rotation.

Be clear about what that covers. The backup is the store exactly **one write ago**. It protects against a write that goes wrong while it is happening. It is not a way back from a mistake you notice later: the next write replaces it, so two writes after a wrong bulk edit the backup holds the wrong edit too. Before anything risky (a large reorganisation, running a build from a branch against your real home), copy `tandem.db` somewhere yourself, with `tandem serve` stopped. The notes are not covered by the backup at all: `bikeshed/` is a folder of files, and keeping it in git or under your usual file backup is the way to protect it.

Recovery is the backup and two commands, and the journal files go first or the restored file is read under a dead WAL:

```sh
rm -f ~/.tandem/tandem.db-wal ~/.tandem/tandem.db-shm ~/.tandem/tandem.db
mv ~/.tandem/tandem.db.bak ~/.tandem/tandem.db
```

**Quit the server first.** `tandem serve` holds `tandem.db` open for as long as it runs. On macOS and Linux the rename over the held file is allowed, and the running server notices the replacement and re-attaches to the restored store by itself. Windows does not allow it: SQLite opens the database without `FILE_SHARE_DELETE`, so the `rm` and the `mv` over it fail with *The process cannot access the file because it is being used by another process.* Close `tandem serve`, and any process holding the store open, before the two commands, then start them again: the next open attaches to the restored file, and its first write switches it back to WAL.

See [ADR 0018](adr/0018-rolling-store-backup.md).

## Upgrading

A newer `tandem` may carry a newer schema. Reads never change the store. The first **write** by the newer binary migrates the store forward, and it rolls `tandem.db.bak` first, so the backup holds the store as it was before the migration.

After that, an older binary refuses the store: it does not read a schema newer than its own. So upgrade every copy on the machine, not just one, and restart `tandem serve`: a server started before the upgrade is still the old binary, and it is the one process the board and every agent session talk to.

- If the board or your agent reports that the store is a schema this build does not read, an old `tandem` is still on PATH or an old `tandem serve` is still running. Run the installer again and restart the server.
- `tandem doctor` shows the schema version the store carries.
- Running a build from a source checkout against your real home migrates it like any other newer binary. Point a development build at a throwaway home with `--home` unless you mean to upgrade.

## Homes from before the Rust port

A home written by the Swift app is not read by this binary. Its `tandem.db` is refused with a message naming its schema version, and the file is left untouched; a `tickets.json` from the older JSON store is ignored, so a home that holds only that reads as an empty board. The one real home from that era was imported once, by hand; [ADR 0031](adr/0031-one-database-the-store.md) records it. There is no automatic import.

## The notes

The bikeshed is a wiki, not a log: its pages hold what a project is, how to run it, where things live, what is open now, and the decisions behind it. It does not hold the story of what was tried: git history and the ticket comments are that. See [ADR 0033](adr/0033-wiki-is-documentation.md).

Notes are markdown files in folders under `~/.tandem/bikeshed/`, linked with `[[wikilinks]]`. You can edit them with any editor: the notes page and the agent's MCP tools read the files as they are on disk. A rename or a move made through Tandem (the notes page, `tandem-cli note rename`, the `note_move` tool) rewrites the wikilinks that pointed at the old path; a rename made in a file manager does not.

The convention this repository's own notes follow gives every project one entry point, its **Overview**:

```
~/.tandem/bikeshed/projects/<project>/<project>.md
```

The prose says what the project is, how to run it and where things live; its `## Threads` list is what is open now (the board is the full list); and it links the project's Decisions, Risks, Processes and sub-pages. Each page names the date it was last true, `Reviewed <date>` - a page that has drifted is the page to fix. Tandem does not enforce the layout: it is a way to keep a wiki findable, and yours can differ.

Start at `~/.tandem/bikeshed/Index.md`, or the board's Notes page, when you do not know the project's name yet.

A delete moves the note or folder into `~/.tandem/bikeshed/.trash/`, under a UTC timestamp folder, and the notes page offers an undo until the page moves on. Trashed notes are kept for 30 days: the next delete after that removes them for good, so the trash is not a backup, and the bytes stay recoverable only until then.
