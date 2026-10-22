# Screenshots

The app, photographed. Each file is the running app's own window, captured in
process (`NSApp.windows` → the key window → `cacheDisplay(in:to:)` → PNG) —
`screencapture` has no Screen Recording permission on this Mac, so the app
photographs itself instead.

| file | what it shows |
| --- | --- |
| `tickets-tab.png` | the main window's **Tickets** tab: the quick-add, the Open/Done/All/Archived counts, the search field and the lane-grouped list |
| `tickets-search-empty.png` | the same tab with a search that matches nothing — "No ticket title matches your search." |
| `board.png` | the **Board** view: Status columns across, cycle lanes down, a ticket drawn where its Status and lane meet |
| `ticket-sheet.png` | the ticket sheet: the title, the Status and lane, the description, the ticket's comment log, and the composer the human writes in |
| `bikeshed-send-to-tickets.png` | the Bikeshed face with a note open: the title bar's **Send to tickets** button |
| `bikeshed.png` | the Bikeshed face with a note open: the notes tree, the editor, the backlinks and the Health panel |
| `sessions-tab.png` | the main window's **Sessions** tab: the session records grouped by project, a `running` one and an `ended · succeeded` one |
| `session-sheet.png` | a session record's sheet: the brief, the notes the session filed, the tickets it carried, and the times |
| `session-sheet-ticket.png` | the **ticket sheet**, opened from a record's ticket — the record's own ticket link |
| `session-note-open.png` | the **Bikeshed** face with the session's capture note open — the record's own note link |

Every file is shot against a scratch home (`TANDEM_HOME`) with fixture tickets and
a fixture bikeshed — never a human's real list or real notes.

The store behind them is a scratch home (`TANDEM_HOME`) with fixture tickets and
two fixture session records, not a human's real list.
