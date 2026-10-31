# Screenshots

The app, photographed. Each file is the running app's own window, captured in
process (`NSApp.windows` → the key window → `cacheDisplay(in:to:)` → PNG) —
`screencapture` has no Screen Recording permission on this Mac, so the app
photographs itself instead.

The tool is `Scripts/shot.sh` (with the small Objective-C rig it compiles and
injects, `Scripts/shot.m`), and the script is the rule. No session hand-builds
a rig; a picture is one command each:

    TANDEM_HOME=Fixtures/screenshots/Home Scripts/shot.sh list
    # pick a window id from the list, then:
    TANDEM_HOME=Fixtures/screenshots/Home Scripts/shot.sh <window-id> docs/screenshots/<file>.png

`list` shows the running app's windows (id, size, title). The script refuses to
run when `TANDEM_HOME` is unset or does not resolve to a home under the repo's
own fixtures (`Fixtures/`) — never `~/.tandem`, never `~/.tandem-dev` — and it
refuses to photograph an app whose own environment is not pointed at that same
fixture home. A refusal exits non-zero and writes nothing.

`electron-shell.png` is the one exception: the port's Electron shell is not the
Swift app, so `Scripts/shot.sh` cannot drive it. That picture is the shell's own
in-process rig, `TANDEM_SHOT=<path.png> npm run dev` in `app/` against
`Fixtures/port/Home`, spelled out in `app/README.md` and held to the same rule:
a fixture home, the app's own capture, a refusal writes nothing.

| file | what it shows |
| --- | --- |
| `electron-shell.png` | the port's Electron shell at its 760 pt floor: the wordmark, the four kept tabs (Tickets, Bikeshed, Usage, Settings) and the store revision read over `rpc` |
| `tickets-tab.png` | the main window's **Tickets** tab: the quick-add, the Open/Done/All/Archived counts, the search field and the lane-grouped list |
| `tickets-search-empty.png` | the same tab with a search that matches nothing — "No ticket title matches your search." |
| `board.png` | the **Board** view: Status columns across, cycle lanes down, a ticket drawn where its Status and lane meet |
| `ticket-sheet.png` | the ticket sheet: the title, the Status and lane, the description rendered from its markdown (headings, bullets, a code block), the ticket's comment log, and the composer the human writes in |
| `ticket-sheet-source.png` | the same sheet with `Edit source` on: the stored markdown in the monospaced field, the toggle reading `Preview` |
| `bikeshed-send-to-tickets.png` | the Bikeshed face with a note open: the title bar's **Send to tickets** button |
| `bikeshed.png` | the Bikeshed face with a note open: the notes tree, the editor, the backlinks and the Health panel |
| `sessions-tab.png` | the main window's **Sessions** tab: the session records grouped by project, a `running` one and an `ended · succeeded` one |
| `usage-tab.png` | the main window's **Usage** tab: the reported spend and the records' coverage, the ticket table's own arithmetic, the Day/Model/Agent/Ticket picker, and one ticket row rolled down to the records behind it |
| `session-sheet.png` | a session record's sheet: the brief, the notes the session filed, the tickets it carried, and the times |
| `session-sheet-ticket.png` | the **ticket sheet**, opened from a record's ticket — the record's own ticket link |
| `session-note-open.png` | the **Bikeshed** face with the session's capture note open — the record's own note link |

Every file is shot against a scratch home (`TANDEM_HOME`) with fixture tickets and
a fixture bikeshed — never a human's real list or real notes.

The store behind them is a scratch home (`TANDEM_HOME`) with fixture tickets and
four fixture session records, three of them carrying reported usage, not a human's
real list.
