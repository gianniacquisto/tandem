---
name: tandem
description: "Read and write the shared Tandem ticket list, manage its cycle lanes, and rename bikeshed notes, from `tandem-cli` on this Mac. Reach for it when the user says to add a todo, asks what's on their list, wants a ticket's status or assignee changed, wants a cycle lane created, renamed, or deleted, or listed, asks to mark a todo done, or wants a note renamed (reopened under a new name). Also runs a local setup check when the user asks whether Tandem is installed or working. The same tickets a human sees and edits in the Tandem app."
---

# Tandem

`tandem-cli` is Tandem's agent-facing interface on this Mac — the binary the app bundles (`/Applications/Tandem.app/Contents/MacOS/tandem-cli`, stable across upgrades), so an agent reads and writes the same list and notes a human uses in the app. `tandem-cli --help` prints the syntax and exits `0`.

## The store

`~/.tandem/tandem.db`, the human's real list — one SQLite database holding the tickets and lanes, the comment logs, the session and sprint records and the settings (ADR 0031). A from-source build (`swift run tandem-cli`) writes `~/.tandem-dev/` instead, so development can never reach the real list; notes live beside it (`~/.tandem/bikeshed/`, `~/.tandem-dev/bikeshed/` from source). Every changed write keeps one previous complete copy beside it — `tandem.db.bak`, taken through SQLite's own backup API — and recovery is the backup and two commands, the journal files first: `rm -f ~/.tandem/tandem.db-wal ~/.tandem/tandem.db-shm ~/.tandem/tandem.db` then `mv ~/.tandem/tandem.db.bak ~/.tandem/tandem.db`. A store from before v0.27.0 is a JSON file: it is read as the fallback and imported once on the domain's first write, and after that a copy put back beside the database is **ignored** — the backup is the recovery, not the file.

## Tickets

Ten verbs under `ticket`: `list`, `show`, `add`, `set`, `move`, `block`, `unblock`, `complete`, `answer` and `archive`, plus `comment` with sub-verbs of its own. Every object they return has the same shape:

| field | meaning |
| --- | --- |
| `id` | the UUID; pass it back to any id-taking verb |
| `ref` | the human handle, `TAN-42` — a lane key plus a per-lane number, null in the default area; pass it **anywhere `--id` is taken** |
| `number` | the number inside the lane (`42`), null in the default area |
| `title` | one line, trimmed |
| `status` | `backlog` (default), `on-hold`, `ready`, `in-progress`, `in-review`, `done` |
| `archived` | true when set aside — out of the default views, still in the store |
| `description` | markdown body, or null; write from stdin with `ticket add --description -` or `ticket set --description -`, read it back whole with `ticket show --id` |
| `assignee` | `human`, an agent's name, or null for unowned |
| `origin` | who filed it — written at add, never changed |
| `lane` | the **cycle lane**, or null for the default area |
| `done` | derived from `status == "done"`, kept on the wire for older readers — read `status`, not this |
| `blocked` | derived: true while any ticket it waits on (`blockedBy`) is not `done`; a block lifts with no write to the dependent (ADR 0035) |
| `blockedBy` | what it waits on: one `{id, ref, title, status}` per edge, in the order set; an edge to a done ticket stays as history |
| `blocks` | who waits on it: the far end of the same edges, in stored order |
| `blockedReason` | the body's `## Blocked on` section, read rather than copied; `show` carries it, the list summary does not (body text stays out of a list) |
| `link` | a bikeshed note path, or null; a human sets it in the app |
| `createdAt`, `completedAt` | ISO-8601 UTC; `completedAt` is null unless done |

### The ref — `TAN-42`

A ticket in a declared lane also carries a **ref**: the lane's key and a per-lane number, rendered `KEY-N` — `TAN-42`, never zero-padded. It is the handle a human writes in chat, and every verb that takes `--id` (or `--ticket`) accepts it beside the full UUID: `ticket show --id TAN-42` and `ticket show --id tan-42` both resolve. A UUID *prefix* is not a handle — the ref is. Numbers are issued per lane, one-based, and never handed out twice; a move to another lane takes that lane's next number and does not carry the old one, and the default area has neither a number nor a ref. An unknown ref fails with the lane's real range (`Lane "Probe" (PRB) has issued PRB-1 to PRB-2`), not the open list. Numbering an existing board is the store's own migration: every lane's tickets are numbered by `createdAt`, once.

`status` is the truth, and a change is free-form — any status to any other, backwards included — because the board records what is true, not a process.

`list` returns these objects as **summaries**: every field except `description`, which becomes `descriptionLength` (its character count, `0` when there is none), and `blockedReason`, which is body text like the description. `blocked`, `blockedBy` and `blocks` ride the summary, so the draw is one read. `show`, `add`, `set`, `move`, `block`, `unblock` and `complete` return the whole object.

### `ticket list [--filter open|done|all] [--status <v[,v]>] [--assignee <name|self>] [--lane <name>] [--text <string>] [--archived] [--blocked]`

The tickets in view, in stored order (the order the human arranged; new tickets land on top). Every filter is optional and they combine:

- `--filter` — `open` (default: not done, not archived), `done`, or `all` (every ticket, the archive excluded). The CLI's filters never carry the archive.
- `--status` — comma-separated values, e.g. `--status ready,in-progress`; a repeated flag keeps only its last value.
- `--assignee` — an exact name, `self` for your own (`$TANDEM_AGENT`), or `""` for the unowned.
- `--lane` — a cycle lane name, or `""` for the default area.
- `--text` — a case- and accent-insensitive substring of the title **or the description**.
- `--archived` — no value. Alone it is the archive; with a filter, the archive joins it.
- `--blocked`, no value. A plain AND axis: alone it is the tickets waiting on an open blocker, and with `--status ready` it is the blocked ready ones. A row whose only blocker is `done` is not blocked.

An empty result is a success: `{"tickets":[]}`.

A board read should not carry every ticket's body, so the rows are summaries — `--text` still matches descriptions (the filtering happens before the summary), and a row that looks interesting is read whole with `show`.

### `ticket show --id <id>`

One ticket, **whole** — the only read that carries the description, and the only lookup that takes an id (`--text` matches title and description, never the id). An unknown id is a domain failure whose error carries the current open list.

### `ticket add --title <string> [--status <value>] [--assignee <name|self>] [--lane <name>] [--origin <name|self>] [--description <string|->]`

Title required, trimmed, empty rejected; duplicates allowed. A title alone lands `backlog`, unowned, on top of the default area. `--description -` reads the body from stdin, so a body'd ticket is **one write**, not an add-then-set round trip; naming the flag means the body must carry text (`""` and an empty pipe are both exit `1`, `Description body is empty…`), and omitting it files a ticket with no body. `--assignee self` claims it — `add` never assigns implicitly, because agents routinely file tickets *for* the human (`--assignee human`). A lane must already exist. `--origin` stamps who filed it, never changed after: pass `--origin self` when you file — an omitted flag means `human`, the app's own stamp, and your proposal filed under the human's name is the exact mix-up the field ends. When the host exports no `TANDEM_AGENT`, `self` has nothing to resolve to and the add fails: pass your own agent name instead, e.g. `--origin general`.

### `ticket set --id <id> [--title …] [--status …] [--assignee …] [--description …]`

Change the fields you name in **one write**; everything unnamed is left alone. `--description -` reads the body from stdin, which is how multi-line markdown lands intact. `--assignee ""` clears the owner, `--description ""` clears the body, `--assignee human` hands the ticket to the human. Moving to `done` stamps `completedAt`; moving out clears it. An empty `--title` is rejected, and an empty or whitespace-only `--description -` body is rejected too — exit `1`, `Description body is empty…`, the description left as it was: an empty pipe is a failed write, not a clear.

### `ticket move --id <id> [--lane <name>] [--index <number>]`

Move between lanes and/or within one. `--index` is 0-based inside the target lane; omit to append. `--lane ""` moves to the default area; the lane must exist.

### `ticket block --id <id> --on <id>` / `ticket unblock --id <id> [--on <id>]`

Record that one ticket **waits on** another, as an edge in the store, so the board can order, draw and refuse a blocked ticket (ADR 0035). `--on` takes the same handle `--id` does, a ref (`TAN-42`) or a UUID; an absent one is the empty handle and fails as the unknown id it is. The same edge twice is a no-op, not an error. The write is refused, with nothing written, when the edge would make a ticket wait on itself, when it would **close a cycle** (the message names the loop: `TAN-5 -> TAN-7 -> TAN-5`), when the blocker is **done** (the block would lift the moment it was written), and when it is **archived** (the block could never be seen to lift). Archiving a blocker later is allowed.

`unblock` with `--on` removes that one edge; without it, every edge the ticket has, so a dependent is released in one call. A missing edge is a no-op. Neither verb touches status, assignee or the ticket's place.

The block is physical: `ticket show` carries `blocked`, `blockedBy`, `blockedReason` and `blocks`; the board card draws `Blocked, TAN-42`; the list row a `blocked` capsule; and the ticket sheet a `Blocked by` and a `Blocks` section. The sprint's batch draw skips a blocked ticket whatever its status, and says so in one line (see `skills/tandem-sprint/SKILL.md` §2). **Why it waits is the body's `## Blocked on` section**, read not copied: write the reason there, and the ids in the edge.

A wait that is not a ticket (a person, a release, a credential) stays prose in `## Blocked on`, and the ticket is parked `on-hold`: the draw never takes an `on-hold` row, and `blockedReason` still shows the section. Do not file a placeholder ticket for it.

### `ticket complete --id <id>`

The short path to `--status done`: idempotent, and an unknown id's error carries the current open list. No `reopen` verb — `ticket set --id … --status backlog` is the reopen.

### `ticket answer --id <id> --status <next> [--assignee <name|self>] --body <string|->`

Record the human's answer and close the hold in **one write**: a `## Answer — <date>` block appended to the description, the status the answer opens, and the next assignee. `--assignee` omitted (or `""`) clears the owner — the question is settled; `self` names this agent. `--body -` reads the body from stdin, and an empty body is refused, leaving the ticket alone. The inbox stops listing a ticket as soon as the block is in its body, whether or not the pair moved — so an answered ticket still wearing `human` on the agents' read is debt: answer it again with the pair you want.

### `ticket archive`

Archive every done ticket in one call: nothing is deleted, they leave the default views (`--archived` still shows them). Returns `{"archived": <n>}`; a second call returns 0.

### `ticket comment add --id <id> [--author <name>] --body <string|->` / `ticket comment list --id <id>`

Append to / list the ticket's **discussion log** — where agents talk after refinement, since the description stays refinement-only. The log is the store's `comments` table, not a ticket field, so it survives any edit of the ticket; an append is one transaction that checks the ticket exists and writes the comment together, so there is no sidecar lock to hold. `--body -` reads from stdin; an empty body is rejected. **`--author` signs the line with the name you write — `self` is not expanded for it** (only `--assignee` and `--origin` expand `self`); write the role out (`skills/tandem-team/SKILL.md`), and never omit it: an unnamed author is a gap in the record. Append-only — no edit, no delete; a correction is a new comment. `comment list` returns comments in append order; an unknown id is an empty list, because the log outlives the store's entries.

## Lanes

A **cycle lane** is the area axis — the list groups by it, the board draws its rows; `--lane ""` (or null `lane`) is the default area:

| command | returns |
| --- | --- |
| `lane list` | `{"lanes": [...]}` |
| `lane add --name <name> [--abbr <key>]` | `{"lanes": [...]}` |
| `lane rename --from <a> --to <b> [--abbr <key>]` | `{"lanes": [...]}` |
| `lane delete --name <name>` | `{"lanes": [...], "moved": <n>}` |

`lanes` is the lane list, in render order — `list`'s answer is the store's declared lane list read directly (empty lanes included, no ticket rows); the write verbs answer the list their write produced. Names are trimmed; empty, duplicate, and unknown names are domain failures. `rename` rewrites the lane list and every ticket in it in one write; `delete` never deletes tickets — members fall back to the default area, `moved` is how many. A lane must exist before `ticket add/move --lane` accepts it.

Each lane has a **key** — the `PRB` of `PRB-1` (see "The ref" above). `lane add --abbr <key>` names it; omitted, the key is the first three alphanumerics of the name (`Tandem` → `TAN`, `code-rank` → `COD`). A key is uppercase letters and digits, unique across lanes case-insensitively, and **frozen once a ticket in the lane holds a number**: `lane rename` keeps it, and `--abbr` that would move a frozen key is refused, naming the refs that would move.

## Sessions

A session is a record, not a ticket (ADR 0027): one record per agent session, a row of the store's `sessions` table in `tandem.db` (ADR 0031) — never a ticket, never on the board. Five verbs, two states (`running`, `ended`):

| command | returns |
| --- | --- |
| `session start [--project <name>] [--agent <name>] [--session <id>] [--brief <string\|->]` | the session record |
| `session link --ticket <id> [--session <id>]` | the session record |
| `session end [--session <id>] [--outcome <value>] [--summary <string\|->] [--tokens-input <n>] [--tokens-output <n>] [--tokens-reasoning <n>] [--tokens-cache-read <n>] [--tokens-cache-write <n>] [--cost <dollars>] [--model <name>]` | the session record |
| `session list [--project <name>] [--status <value>]` | `{"sessions": [...]}` |
| `session show [--session <id>]` | the session record |

- **Defaults** — `--project` is the working directory's last path component; `--agent` is `$TANDEM_AGENT`; `--session` is `$OPENCODE_SESSION_ID`, else a generated token.
- **Two states** — `running` from `start` to `end`; `ended` carries `endedAt`, `outcome` (`succeeded | failed | interrupted | unknown`), and `summary`. A session that dies leaves a `running` record.
- **The ticket link is stored** — `link --ticket` appends the ticket id to the record's `tickets`; the record, not the ticket, holds the link.
- **The note link is derived** — `note capture --session <id>` files the capture under `projects/<project>/.raw/<session>/`, and `show` lists those paths as `notes`. There is no linking verb for notes, and `list` rows omit the field.
- **`--brief` and `--summary` take stdin** like `--description`: `-` reads stdin, and an empty body is a domain failure — omit the flag to leave the field unset. `--brief` takes the literal text or `-`, never a file path (a path is refused: pipe the file with `--brief -`). An unknown `--outcome` or `--status` is a domain failure.
- **Re-`start` on a recorded id** returns the record unchanged with a `note` — nothing is overwritten.
- **The usage is reported at `end`** — the seven optional flags (`--tokens-input`, `--tokens-output`, `--tokens-reasoning`, `--tokens-cache-read`, `--tokens-cache-write`, `--cost`, `--model`) write what the harness measured beside the outcome; a flag that is absent is absent on the record, never an invented zero. When any usage is present, the end posts **one usage comment, signed by the record's agent (the `--agent` the session started with; `usage` when it carries none), on every ticket the session linked** — a header naming the session id and the model, then one line per reported metric with thousands separators; a piece the harness did not report is left out — and stamps once, so a second `end` cannot double-post it. A session with no linked tickets stores the usage and posts nothing. On this host the numbers come from the wrapper: `Scripts/usage-report.sh --outcome <…>` (summary on stdin) reads the session row from opencode's database and calls `session end` with the flags; the row missing is exit 1 and the pass falls back to a plain `session end`.

One session, start to end (`ses_demo2` against a scratch home):

```sh
$ export OPENCODE_SESSION_ID="ses_demo2"
$ tandem-cli session start --project tandem --agent builder --brief "Docs pass: write the session record documents"
{"agent":"builder","brief":"Docs pass: write the session record documents","endedAt":null,"id":"ses_demo2","outcome":null,"project":"tandem","startedAt":"2026-10-23T12:33:55Z","status":"running","summary":null,"tickets":[]}
$ tandem-cli session link --ticket TAN-42
{"agent":"builder","brief":"Docs pass: write the session record documents","endedAt":null,"id":"ses_demo2","outcome":null,"project":"tandem","startedAt":"2026-10-23T12:33:55Z","status":"running","summary":null,"tickets":["9879916B-2F32-4999-AEEC-A963DA951DFF"]}
$ tandem-cli session end --outcome succeeded --summary "Documents landed on the PR" \
    --tokens-input 12345 --tokens-output 4567 --tokens-reasoning 567 \
    --tokens-cache-read 89012 --tokens-cache-write 1234 --cost 0.42 --model z-ai/glm-5.3-flash
{"agent":"builder","brief":"Docs pass: write the session record documents","cost":0.42,"endedAt":"2026-10-23T12:33:55Z","id":"ses_demo2","model":"z-ai/glm-5.3-flash","outcome":"succeeded","project":"tandem","startedAt":"2026-10-23T12:33:55Z","status":"ended","summary":"Documents landed on the PR","tickets":["9879916B-2F32-4999-AEEC-A963DA951DFF"],"tokens":{"cacheRead":89012,"cacheWrite":1234,"input":12345,"output":4567,"reasoning":567}}
```

The end above also posted one usage comment on ticket `9879916B…` — the only notice a linked ticket gets, composed by the CLI, signed by the session's agent (`usage` when the record carries no agent), one line per reported metric:

```sh
$ tandem-cli ticket comment list --id TAN-42
{"comments":[{"at":"2026-10-23T12:33:55Z","author":"builder","body":"Usage — ses_demo2 · z-ai/glm-5.3-flash\ninput 12,345\noutput 4,567\ncache read 89,012\ncache write 1,234\nreasoning 567\ncost $0.42"}]}
```

## Notes

The bikeshed (`~/.tandem/bikeshed/`) is a plain folder of `.md` files, and the file itself is the interface — create, edit, and delete curated notes with your own file tools; the CLI writes no note content. Four CLI verbs act on notes: `rename`, `capture`, `stats`, and `links`.

### `note rename --from <path> --to <path>`

Both paths bikeshed-root-relative, `.md` included. Rewrites every wikilink that resolves to the note — bare, qualified, aliased, `#fragment` — in every note including itself, in one locked transaction. Rename through the CLI only: a raw `mv` breaks every wikilink silently, and nothing repairs them but re-running the rename.

### `note capture --kind <decision|gotcha|fact|thread> [--project <name>] [--session <id>]`

File one **capture** — a small, create-only record of a decision, gotcha or fact whose re-derivation would cost real effort, and which no session record, ticket, ADR or the code already carries. Body from **stdin** (UTF-8; empty allowed). It lands at `projects/<project>/.raw/<session>/<utc>-<kind>-<slug>.md`; the slug comes from the body's first line, same-second same-slug notes get a `-2`, `-3`… suffix. `--project` defaults to the working directory's last path component; `--session` to `$OPENCODE_SESSION_ID`, else a generated token. The saddlebag is **hidden**: captures never appear in search, backlinks, or wikilink resolution, and carry no `[[wikilinks]]` — the consolidation pass promotes them into the curated notes. An unknown `--kind`, or a `--project`/`--session` that is not one folder name, is a domain failure. The `work` kind is retired — the summary at `session end --summary` (ADR 0027) is the work record, and `--kind work` is a domain failure (exit 1) naming it; stored `*-work-*.md` captures still read and archive fine.

A note filename may not contain `/`, `#`, `|`, or a leading dot, and must end `.md` — `#` and `|` are the wikilink grammar's separators, so such a name can never be resolved (ADR 0004). The rule constrains what is *created*; pre-existing violators still show and edit.

### `note stats [--project <name>]`

Whether memory is draining — one JSON object, read-only and lock-free. Per project: `curatedNotes`, `rawPending` / `rawArchived` (captures pending under `.raw/`, consumed under `.raw/.done/`), `oldestPending`, `lastConsolidated` (the newest curated-note mtime); plus `totals`. A high `rawPending` or a stale `lastConsolidated` is the queue not draining — the signal to hand the saddlebag to a consolidation pass (`skills/tandem-consolidate/`).

### `note links`

The link graph — `broken` (wikilinks that resolve to nothing: `note`, `link` as written, 1-based `line`), `orphans` (notes with no incoming link), and `counts`. Read-only and lock-free. Rules: a link resolves case-insensitively by name — bare `[[Name]]` or qualified `[[folder/Note]]`, trailing `.md` optional; `#fragment` and `|alias` change the display, never the target; a duplicated basename is a broken link, not a tie-break; `[[…]]` inside code spans or fenced blocks is literal text; the hidden saddlebag stays out of the graph; orphans exclude the entry points (`Index.md`, and a project's anchor note `projects/<project>/<project>.md`). The app's bikeshed **Health** panel reports the same graph. Nothing is repaired for you — a broken link may be an intention; fix by editing the note or renaming through `note rename`.

## Setup check

`tandem-cli doctor` — where this machine's Tandem state lives and whether it is healthy. One JSON object: `{"ok": true, "checks": [{"name": "...", "ok": true, "detail": "..."}]}`. The checks, in order — `home` (the resolved state directory and why: `TANDEM_HOME` override, shipped app, or dev build), `store` (the `tandem.db` path, its schema version, open/done counts, the rolling backup, and a fork note when a JSON store file sits beside an imported database), `bikeshed`, `comments` (`absent` is a valid early state), `lock` (a write transaction another process is holding), `app` (the installed version, with the quarantine-strip hint), `skills` (whether this skill is found for each agent host). `store`, `bikeshed`, `comments` and `lock` can fail; a missing app or uninstalled host is informational, not failure. Exit `0` healthy, `1` not, `2` usage error.

## Streams and exit codes

- **stdout** — one JSON object on success. `list` → `{"tickets": [...]}`, summary rows (no `description`, its `descriptionLength` instead); `show` → the whole ticket object; `add`, `set`, `move`, `complete` → the ticket object; `archive` → `{"archived": <n>}`; `comment add` → the comment; `comment list` → `{"comments": [...]}`; `lane list`/`add`/`rename` → `{"lanes": [...]}`, `lane delete` → `{"lanes": [...], "moved": <n>}`. A top-level `note` appears only when the call recovered a corrupt **session or sprint** record file, naming the backup path — the ticket store never recovers (ADR 0031).
- **stderr** — short, actionable plain text on failure, naming the failed value and the next step; `complete` and `show` with an unknown id append the current open list in the `list` shape (summaries).
- **exit** — `0` success; `1` domain failure (empty title, unknown id, locked store); `2` usage failure (no arguments, unknown subcommand or flag). A missing flag value is deliberately a domain failure, not usage — `add` with no `--title` is the empty title the domain rejects. Retry only when the text says the store is locked.
