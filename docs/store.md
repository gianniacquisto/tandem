# The store

`~/.tandem/tandem.db` is the store: one SQLite database holding the tickets and their cycle lanes, the comment logs and the projects. The server watches it, so the board updates live whenever anything changes it, including an agent. There is no second source of truth and no sidecar file to keep in step; the bikeshed's notes stay plain markdown beside it.

The tickets are rows, in stored order - `tickets.position` **is** the list order, new tickets land on top, and the order is never recomputed from `createdAt`. Each ticket's `lane` names one of the `lanes` rows, and a null lane is the default (ungrouped) area, which always comes first. The columns are typed, so a count is a query: the open and done counts come straight out of the database rather than from loading the list. See [ADR 0031](adr/0031-one-database-the-store.md) for the decision, [ADR 0006](adr/0006-stored-order-and-sections.md) for order and lanes, and [ADR 0030](adr/0030-one-entity-the-ticket.md) for the vocabulary.

The schema is versioned in the database itself: `schema_migrations` holds one row per migration - a version and when it was applied - so "which shape is this store?" is a query, and a migration that fails rolls back whole rather than leaving the store half-upgraded.

Beside the store, every changed write keeps **one previous complete copy** in `tandem.db.bak`, taken through SQLite's backup API before the write touches the database. There is one generation, no rotation. Recovery is the backup and two commands, and the journal files go first or the restored file is read under a dead WAL:

```sh
rm -f ~/.tandem/tandem.db-wal ~/.tandem/tandem.db-shm ~/.tandem/tandem.db
mv ~/.tandem/tandem.db.bak ~/.tandem/tandem.db
```

**Quit the server first.** Its watch holds `tandem.db` open for as long as it runs. On macOS and Linux the rename over the held file is allowed, and the running server notices the replacement and re-attaches to the restored store by itself. Windows does not allow it: SQLite opens the database without `FILE_SHARE_DELETE`, so the `rm` and the `mv` over it fail with *The process cannot access the file because it is being used by another process.* Close `tandem serve`, and any `tandem mcp` process, before the two commands, then start them again: the next open attaches to the restored file, and its first write switches it back to WAL. On Windows the home those two commands name is `%USERPROFILE%\.tandem`.

See [ADR 0018](adr/0018-rolling-store-backup.md).

## Moving off the JSON store

Tandem's store was `tickets.json` until v0.27.0, with the comments, sessions, sprints and settings in files beside it. A home from that era is read as it is: a read falls back to the JSON while the database has not imported it, and the first write to each domain imports its file once - copying it aside as `tickets.json.pre-sqlite-<timestamp>` and recording the import in the database (`store_imports`). After that the file is never read again, so a hand-restored `tickets.json` beside a database that already imported it is **ignored**. Copying a file back is not a recovery path; the backup is, or, deliberately, dropping the domain's import row so the next write imports the file again. The same marker silences writes from the other direction: a pre-v0.27.0 build writing `tickets.json` beside the database is ignored too - its writes never reach the store - so upgrade every writer on the Mac, not just one.

## The notes

The bikeshed is a wiki, not a log: its pages hold what a project is, how to run it, where things live, what is open now, and the decisions behind it. It does not hold the story of what was tried: git history and the ticket comments are that.

Every project has one entry point, its **Overview**:

```
~/.tandem/bikeshed/projects/<project>/<project>.md
```

The prose says what the project is, how to run it and where things live; its `## Threads` list is what is open now (the board is the full list - `tandem-cli ticket list`); and it links the project's Decisions, Risks, Processes and sub-pages. Each page names the date it was last true, `Reviewed <date>`, and the Notes page's mtime is the same signal - a page that has drifted is the page to fix.

Start at `~/.tandem/bikeshed/Index.md`, or the board's Notes page, when you do not know the project's name yet.
