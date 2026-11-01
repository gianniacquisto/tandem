# Screenshots

The app, photographed. Every picture here is the running app's own window,
captured by a spec. The Playwright suite photographs the app; every run
uses a fixture home.

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
    TANDEM_SHOTS_HOME=../Fixtures/port/Board npx playwright test tests/shots/tickets.shots.ts

`TANDEM_SHOTS_HOME` is required, and it must resolve under the repository's
`Fixtures/`: a run that names no home is refused, and so is `~/.tandem` or any
path outside `Fixtures/`. The app opens the scratch copy, never the committed
home, so no run writes into the fixture.

The port has two committed homes. `Fixtures/port/Home` is the contract home: the
four session records and two board tickets the shell, Usage and Settings scenes
read. `Fixtures/port/Board` is the 78-ticket board home, and the Tickets scenes
are shot on it. CI runs the first command above; the Tickets pictures in this
folder are the second.

The seed is the named fixture home plus a fixed clock: the page's `Date` is
pinned to `2026-10-30T12:00:00Z` before the first application script runs, and
`TZ` is UTC, so a scene that renders a now-relative time renders one instant.
The two seeded paths are fixed too, because the Settings face prints them: a run
in one checkout writes the same bytes as a run in any other. Two things stay the
machine's own, and no fixture can make them otherwise: the `app` row of the
doctor report names the Tandem installed on the machine, and the PNG's pixel
size follows the display's scale factor.

A face taller than the window is laid out at its own height (the scroll
constraint lifted, the width kept) and shot whole, so a picture of a table is
the whole table; a board is laid out at the width its six status columns need,
so the picture is the whole board rather than a column and a half of it. The
scratch home is seeded again before every test, so a spec that writes (a drag, a
save, the CLI as a second writer) cannot change the next spec's picture or its
assertions. A failing spec fails the run, and the catalogue is rewritten only
when every scene's picture exists, so a picture and its row cannot drift.

CI runs the same command on the Linux runner under a virtual display
(`.github/workflows/app.yml`), so a deliberately broken selector is a red job
rather than a human reading a diff.

The suite asserts a face, not only its contents: every photographed face checks
one computed style against the token it reads from
`app/renderer/src/tokens.css`, because structure alone is not a picture. A
stylesheet that parses into a stylesheet with no matching rules leaves the same
DOM, the same row ids and the same counts on unstyled text.

| file | what it shows |
| --- | --- |
| `electron-shell.png` | the shell at its 760 pt floor: the wordmark, the four kept tabs (Tickets, Bikeshed, Usage, Settings), the Tickets face it opens on, and the store revision read over `rpc` |
| `usage-face.png` | the **Usage** face over the fixture home: the reported spend and its coverage, the cost-per-day bar chart above the picker, the ticket rows and their arithmetic, the Day/Model/Agent/Ticket picker, and a ticket row rolled down to the records behind it, each naming its session, agent, model, local start, cost, tokens and the tickets it carried |
| `bikeshed-face.png` | the **Bikeshed** face over the fixture home: the scaffold tree with its reveal control, the committed CodeMirror bundle editing the fixture project note in its own frame, the Backlinks dock naming the Index note, the Health panel reporting the daily note as an orphan, and a draft a killed save left named as a recovery |
| `settings-face.png` | the **Settings** face over the fixture home: the home, the bikeshed, the three agent hosts, the theme rule, the bundled and installed versions, and the doctor's checks each named with its state and detail |
| `tickets-list.png` | the **Tickets** face in its list layout over the port's 78-ticket board home (`Fixtures/port/Board`): the quick-add, the Open/Done/All scopes with their counts, the status, assignee and lane filters, the search, and the lane-grouped rows with each ticket's ref, title, status, assignee, lane and blocked mark |
| `tickets-board.png` | the same face on the same 78-ticket board home in its board layout: the lanes as rows, each named with its count and holding one cell per status, the six status columns with On hold parked last, and the same rows set on cards in the cell of their lane and status |
| `tickets-focus.png` | the **Tickets** face with a project focus set: the focus control naming the active lane with its show-all escape, and the board drawing that lane row alone |
| `tickets-keyboard.png` | the keyboard-only pass over the 78-ticket board home: a row selected with the arrows, the quick-add reached with the platform modifier and N, the search with the platform modifier and F, and a ticket moved one status over, inside its own lane, with Option and the arrows |
