# Screenshots

The app, photographed. Every picture here is the running app's own window,
captured by a spec. `Scripts/shot.sh` photographs the Swift app and the
Playwright suite photographs the port; the two never share a home.

## The port's suite

`app/tests/shots/*.shots.ts` is the rule: no session hand-builds a rig, and a
picture is one scene of one spec. A run checks the named home, copies it to a
scratch seed at `/tmp/tandem-shots/Home`, builds the fixture user home beside
it (`/tmp/tandem-shots/user`, the `HOME` a Settings picture reads its agent
hosts from), launches the built app at its 760 pt width, asserts each scene,
writes one PNG per scene into this folder, and rewrites the table below from
`app/shots/scenes.ts`:

    cd app
    npm ci
    npm run binary
    TANDEM_SHOTS_HOME=../Fixtures/port/Home npm run shots

`TANDEM_SHOTS_HOME` is required, and it must resolve under the repository's
`Fixtures/`: a run that names no home is refused, and so is `~/.tandem` or any
path outside `Fixtures/`. The app opens the scratch copy, never the committed
home, so no run writes into the fixture.

The seed is the fixture home plus a fixed clock: the page's `Date` is pinned to
`2026-10-30T12:00:00Z` before the first application script runs, and `TZ` is
UTC, so a scene that renders a now-relative time renders one instant. The two
seeded paths are fixed too, because the Settings face prints them: a run in one
checkout writes the same bytes as a run in any other. Two things stay the
machine's own, and no fixture can make them otherwise: the `app` row of the
doctor report names the Tandem installed on the machine, and the PNG's pixel
size follows the display's scale factor.

A face taller than the window is laid out at its own height (the scroll
constraint lifted, the width kept) and shot whole, so a picture of a table is
the whole table. A failing spec fails the run, and the catalogue is rewritten
only when every scene's picture exists, so a picture and its row cannot drift.

CI runs the same command on the Linux runner under a virtual display
(`.github/workflows/app.yml`), so a deliberately broken selector is a red job
rather than a human reading a diff.

| file | what it shows |
| --- | --- |
| `electron-shell.png` | the shell at its 760 pt floor: the wordmark, the four kept tabs (Tickets, Bikeshed, Usage, Settings) and the store revision read over `rpc` |
| `usage-face.png` | the **Usage** face over the fixture home: the reported spend and its coverage, the ticket rows and their arithmetic, the Day/Model/Agent/Ticket picker, and a ticket row rolled down to the records behind it, each naming its session, agent, model, local start, cost, tokens and the tickets it carried |
| `bikeshed-face.png` | the **Bikeshed** face over the fixture home: the scaffold tree with its reveal control, the committed CodeMirror bundle editing the fixture project note in its own frame, the Backlinks dock naming the Index note, the Health panel reporting the daily note as an orphan, and a draft a killed save left named as a recovery |
| `settings-face.png` | the **Settings** face over the fixture home: the home, the bikeshed, the three agent hosts, the theme rule, the bundled and installed versions, and the doctor's checks each named with its state and detail |

## The Swift app, until the cutover

The Swift app still ships until `v1.0.0` deletes it (`PRT-20`, `docs/port.md`),
so its pictures are still taken by `Scripts/shot.sh` (with the Objective-C rig
it compiles and injects, `Scripts/shot.m`), and this table is hand-kept:

    TANDEM_HOME=Fixtures/screenshots/Home Scripts/shot.sh list
    # pick a window id from the list, then:
    TANDEM_HOME=Fixtures/screenshots/Home Scripts/shot.sh <window-id> docs/screenshots/<file>.png

`list` shows the running app's windows (id, size, title). The script refuses to
run when `TANDEM_HOME` is unset or does not resolve to a home under the repo's
own fixtures (`Fixtures/`), so never `~/.tandem`, never `~/.tandem-dev`, and it
refuses to photograph an app whose own environment is not pointed at that same
fixture home. A refusal exits non-zero and writes nothing.

| file | what it shows |
| --- | --- |
| `tickets-tab.png` | the main window's **Tickets** tab: the quick-add, the Open/Done/All/Archived counts, the search field and the lane-grouped list |
| `tickets-search-empty.png` | the same tab with a search that matches nothing: "No ticket title matches your search." |
| `board.png` | the **Board** view: Status columns across, cycle lanes down, a ticket drawn where its Status and lane meet |
| `ticket-sheet.png` | the ticket sheet: the title, the Status and lane, the description rendered from its markdown (headings, bullets, a code block), the ticket's comment log, and the composer the human writes in |
| `ticket-sheet-source.png` | the same sheet with `Edit source` on: the stored markdown in the monospaced field, the toggle reading `Preview` |
| `bikeshed-send-to-tickets.png` | the Bikeshed face with a note open: the title bar's **Send to tickets** button |
| `bikeshed.png` | the Bikeshed face with a note open: the notes tree, the editor, the backlinks and the Health panel |
| `sessions-tab.png` | the main window's **Sessions** tab: the session records grouped by project, a `running` one and an `ended · succeeded` one |
| `usage-tab.png` | the main window's **Usage** tab: the reported spend and the records' coverage, the ticket table's own arithmetic, the Day/Model/Agent/Ticket picker, and one ticket row rolled down to the records behind it |
| `session-sheet.png` | a session record's sheet: the brief, the notes the session filed, the tickets it carried, and the times |
| `session-sheet-ticket.png` | the **ticket sheet**, opened from a record's ticket: the record's own ticket link |
| `session-note-open.png` | the **Bikeshed** face with the session's capture note open: the record's own note link |

Every file is shot against a scratch home (`TANDEM_HOME`) with fixture tickets
and a fixture bikeshed, never a human's real list or real notes.
