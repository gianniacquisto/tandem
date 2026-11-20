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
| `.tandem-export-*.tmp`, `.tandem-import-*.tmp` | an export's copy of the store, or an import's unpack folder, that was being written when a process died | yes; the next write sweeps any export copy older than a minute, and an import folder is removed by hand |
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

The backup is a whole-file copy, so it also carries the `browser_sessions` rows and the member tables (below): keyed session digests, member names and roles, the SHA-256 digests of personal tokens and sign-in keys, and the access record. It is a copy of the home and is kept as private as the home. See [ADR 0018](adr/0018-rolling-store-backup.md).

## Browser sessions

A token-mode `tandem serve` signs a browser in with a cookie. Each session is one row of `browser_sessions` (schema v13): a keyed digest of the session id, when it was created and when it was last used. The id is never stored, the digest is keyed with the token's digest (so changing `TANDEM_TOKEN` signs every browser out), and a session is dropped after 30 days without use. These rows are not board data: writing one never moves the revision or rolls the backup, they are outside the MCP tools, and the export leaves them out. See [ADR 0049](adr/0049-a-browser-signs-in-to-a-token-guarded-serve.md).

## Members and credentials (schema v14)

A team workspace knows its people ([ADR 0052](adr/0052-members-and-personal-tokens.md)). Schema v14 adds:

| Table | Holds | In an export |
| --- | --- | --- |
| `members` | `id`, a unique `name`, a `role` (`admin` or `member`), `created_at`, and `removed_at`, a tombstone: a removed name is never reused | carried |
| `member_tokens` | the SHA-256 digest of each personal token (`tdm_`), its member, a label, `created_at`, `last_used_at` and `revoked_at` | emptied |
| `member_signin_keys` | the digest of a member's sign-in key (`tdk_`), one live key per member | emptied |

`browser_sessions` also gains a nullable `member_id`: a session row with none is an owner session, so a migrated serve signs nobody out. Team mode is one row of `store_meta` (`team_mode`), written when the first member is added and never unset.

A token or a key is 32 random bytes from the OS generator, base64url, behind its fixed prefix. Only the SHA-256 digest is stored, looked up by that indexed digest: the values are high-entropy random, so a plain hash is enough and no key-stretching buys anything. The value is shown once, when it is minted. Like a session row, a credential write (and a token's last-used time, rewritten at most once a day) never moves the revision or rolls the backup. Removing a member deletes their sessions, tokens and sign-in key in the same write, and the removal ends their open change streams. The archive carries `members` and empties both credential tables, as it empties `browser_sessions`.

The People screen is a second caller of these store functions: it holds no SQL. Reissuing a sign-in key replaces the member's one live row in `member_signin_keys`, so the old digest stops matching at once, and the access record notes `signin-key-issued`, never the key. A person's last sign-in is the newest `last_used_at` of their browser sessions, so it reads "Not signed in yet" once every session of theirs has ended. The access record page reads the newest 50 rows by id, with an older-entries link carrying the last id a page showed.

A personal token has a label, the name its member gave it (1 to 60 characters of one line of plain text: the store function that makes a token refuses an empty label, a longer one and one with a control character, a line or paragraph separator or a character that hides or reorders text, so a caller other than the form cannot store one). A member holds at most 20 live tokens, counted in the same write as the insert. The label is the only part of a token in the access record: the value and its digest never are. To connect an AI coding agent, a member makes a token on the Personal tokens screen and puts it in the tool's MCP settings as `Authorization: Bearer <token>`; the tool acts as that member and is limited as described in [the CLI guide](cli.md).

## The access record (schema v15)

Who changed access to a team workspace, and who exported it, is a durable record an admin can read ([ADR 0052](adr/0052-members-and-personal-tokens.md), part 2). Schema v15 adds one table:

| Table | Holds | In an export |
| --- | --- | --- |
| `access_record` | one row per event: `at`, the `actor` (a member name, `owner` or `operator`), the `credential` kind (`owner`, `member-session`, `member-token`, `operator`), the `event`, the `target` member, a short `detail` (a role, or a token's label, at most 60 characters) and, for an export, the `door` | carried |

The events are: member added, removed or given a new role (`member-added`, `member-removed`, `member-role-changed`), personal token created or revoked (`token-created`, `token-revoked`), sign-in key issued (`signin-key-issued`), an export through the board's door (`export`), and every act of the operator. A row is written in the same transaction as the act it records, so there is no change without a row and no row without a change. It never holds a token, a digest, a key, a header value, a note or a ticket body.

- **Append-only.** No route, tool or store method updates or deletes a row, and two triggers abort a raw `UPDATE` or `DELETE`. Rows are kept: at about 200 bytes an event there is no prune rule.
- **Not board data.** Like a session row, a record row moves neither the revision nor the rolling backup, and it wakes no change stream.
- **Only authorised acts write a row.** A refused attempt (a member asking for the export, a token on a page) is a log line with its reason and no value, never a row, so a hostile caller cannot grow the table.
- **The CLI export writes no row.** `tandem export` opens the home directly and is read-only, so it keeps its one log line on stderr; the board's download writes a row beside its log line.
- **It travels.** The archive carries `access_record` (it holds no secret, and an import only lands in an empty home), so who exported and when moves with the workspace.
- **The last admin cannot be lost.** The one removal function and the one role-change function count the live admins inside their own write transaction: the last admin cannot be removed or demoted, by any door, and two admins demoting each other at once leave one. The owner credential (`TANDEM_TOKEN`) is the only recovery for a workspace with no admin.

## Moving and restoring a home

`tandem export --out <path>` writes the whole home, the store and the notes, as one `.tar.gz`, and `tandem import <path>` restores it into an empty home. It is how a board moves to another machine, into a hosted workspace or out of one, and how a self-hoster keeps a copy that includes the notes. See [ADR 0050](adr/0050-the-home-leaves-and-arrives-as-one-archive.md).

```sh
tandem export --out ~/tandem-2026-11-17.tar.gz          # beside a running serve is fine
tandem --home ~/new-home import ~/tandem-2026-11-17.tar.gz   # serve stopped on that home
```

What the archive holds, in order: `tandem.db` (one committed generation of the store), every note under `bikeshed/`, and `manifest.json` (the format, the schema version and each entry's size and SHA-256) last. `tar -tzf` lists it. It never holds `tandem.db.bak`, a lock file, a temp file, a WAL, or a symlink.

- **No sign-ins and no tokens travel.** Browser sessions and token digests are left out on purpose: after a move everyone signs in again and agents get new tokens, and a mislaid archive opens no workspace. The member names and roles and the access record do travel. The file is **not encrypted**, and it holds every ticket and note, so keeping it safe is your job.
- **Consistent means a store at one instant and whole notes.** The store is one committed generation; each note is a whole version. The two are taken moments apart, not in one transaction, so a note you edit by hand in another program during an export may be caught on either side of the edit.
- **The export is not a backup schedule.** It makes no rotation and replaces nothing. The rolling backup above is untouched, and nothing restores a `tandem.db.bak` from an archive.
- **Import only goes into an empty home.** A home with a `tandem.db`, a `tandem.db.bak`, a WAL or journal file or any note is refused by name; there is no merge and no force, so point `--home` at a new directory. A lock file alone does not count. Run it with `tandem serve` stopped on that home. Nothing is written until the archive has passed every check (the digests, a store that opens and is not newer than this build, no entry that escapes the home), and a refusal leaves the home as it was. An archive from an older schema imports as it is and migrates on the first write, which rolls the backup first.
- **A crash between the two renames.** Import moves `bikeshed/` into place first and `tandem.db` last, so the store's arrival is the commit. A crash in between leaves a home with notes and no store, and the next import refuses it by name as not empty. Remove that `bikeshed/` folder and run the import again.
- **The board's door.** A running `tandem serve` offers the same archive as one download: **Export workspace** in the header menu (beside the theme choice) is a plain link to `GET /export`, which answers `tandem-export-<UTC date>.tar.gz` as an attachment. It is streamed, never built in memory, with no `Content-Length`; the store copy is staged before the first byte, so the board keeps taking writes during a download. A cut download has no manifest, so `tandem import` refuses it as damaged. The route asks the same permission check as the CLI, and the menu entry is drawn only when the check allows the viewer. It refuses a request whose `Sec-Fetch-Site` header is present and is neither `same-origin` nor `none` (an absent header passes, so `curl` works). Off loopback it wants the token like every route: `curl -H "Authorization: Bearer $TANDEM_TOKEN" -OJ https://<host>/export`. It is not an MCP tool and has no remote CLI verb.
- **What the export permission does and does not stop.** One check decides who may export. It is a thin caller of the one permission function: the owner credentials (loopback, `--allow-unauthenticated`, `TANDEM_TOKEN` and an owner browser session) always may, so a solo workspace exports as ever; in a team workspace only an admin's browser session may, and a member's session and every personal token are refused with a `403` page. It refuses a principal it does not know. That stops the one-step bulk download. It does not stop a member who can read every ticket and note from copying them, and it does not bind the offline CLI: anyone who can read the home's files can copy them, and file permissions are that boundary. Each export that starts writes one line (who, when, through which door) to the server log (`door=board` for the download), or to stderr for the CLI; no content and no path. The board's download also writes a row of the access record (above); the CLI does not.

## Upgrading

A newer `tandem` may carry a newer schema. Reads never change the store. The first **write** by the newer binary migrates the store forward, and it rolls `tandem.db.bak` first, so the backup holds the store as it was before the migration.

After that, an older binary refuses the store: it does not read a schema newer than its own. So upgrade every copy on the machine, not just one, and restart `tandem serve`: a server started before the upgrade is still the old binary, and it is the one process the board and every agent session talk to.

- If the board or your agent reports that the store is a schema this build does not read, an old `tandem` is still on PATH or an old `tandem serve` is still running. Replace the binary with the installer's `--reinstall` mode (running it without `--reinstall` leaves an existing install alone, see [installing](install-paths.md#install-upgrade-reinstall-uninstall)) and restart the server.
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
