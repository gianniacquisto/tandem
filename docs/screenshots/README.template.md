# Screenshots

The app, photographed. Every picture here is the running app's own window,
captured by a spec. `Scripts/shot.sh` photographs the Swift app and the
Playwright suite photographs the port; the two never share a home.

## The port's suite

`app/tests/shots/*.shots.ts` is the rule: no session hand-builds a rig, and a
picture is one scene of one spec. A run checks the home, copies it to a
scratch seed, launches the built app at its 760 pt floor with `TZ=UTC`,
asserts each scene, writes one PNG per scene into this folder, and rewrites the
table below from `app/shots/scenes.ts`:

    cd app
    npm ci
    npm run binary
    TANDEM_SHOTS_HOME=../Fixtures/port/Home npm run shots

`TANDEM_SHOTS_HOME` is required, and it must resolve under the repository's
`Fixtures/`: a run that names no home is refused, and so is `~/.tandem` or any
path outside `Fixtures/`. The app opens a scratch copy, never the committed
home, so no run writes into the fixture. A failing spec fails the run, and the
catalogue is rewritten only when every scene's picture exists, so a picture
and its row cannot drift.

CI runs the same command on the Linux runner under a virtual display
(`.github/workflows/app.yml`), so a deliberately broken selector is a red job
rather than a human reading a diff.

<!-- scenes -->

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
