# Screenshots

The app, photographed. Every picture here is the running app's own window,
captured by a spec. The Playwright suite photographs the app; every run
uses a fixture home.

## What a run writes, and what only publish writes

A run writes scratch. Its pictures land in `app/shots-out/` (git-ignored), or in
the directory `TANDEM_SHOTS_OUT` names for a one-spec look; its seed lives at
`/tmp/tandem-shots`. A run never writes this folder and never rewrites the table
below: `docs/screenshots/` and `README.md` move only through
`npm run shots:publish`, which copies a run's bytes and never re-renders. So a
local run leaves `git status` clean, and one machine's pixels are committed only
when a human asks for them.

    cd app
    npm ci
    npm run binary
    TANDEM_SHOTS_HOME=../Fixtures/port/Home npm run shots
    TANDEM_SHOTS_HOME=../Fixtures/port/Board npx playwright test tests/shots/tickets.shots.ts tests/shots/focus.shots.ts
    npm run shots:publish

The two home commands write into the same scratch directory, so the publish
after them resolves every scene. Publish refuses a scene the run did not make,
naming it and changing nothing, and refuses a template that lost its table
marker before it copies a single picture.

## The port's suite

`app/tests/shots/*.shots.ts` is the rule: no session hand-builds a rig, and a
picture is one scene of one spec. A run checks the named home, copies it to a
scratch seed at `/tmp/tandem-shots/Home`, builds the fixture user home beside
it (`/tmp/tandem-shots/user`, the `HOME` a Settings picture reads its agent
hosts from) and the fixture app (`/tmp/tandem-shots/app/Tandem.app`, whose
version the doctor's `app` row prints), launches the built app at its 760 pt
width, asserts each scene, and writes one PNG per scene into the run's output.

`TANDEM_SHOTS_HOME` is required, and it must resolve under the repository's
`Fixtures/`: a run that names no home is refused, and so is `~/.tandem` or any
path outside `Fixtures/`. The app opens the scratch copy, never the committed
home, so no run writes into the fixture.

The port has two committed homes. `Fixtures/port/Home` is the contract home: the
five weeks of session records and the two board tickets the shell, Usage and
Settings scenes read. `Fixtures/port/Board` is the 78-ticket board home, and the Tickets scenes
are shot on it. CI runs both commands above in order; the Tickets pictures in
this folder are the second.

## The agent loop

A session iterating on a face looks without touching the committed pictures:
run one spec. The PNG lands under `app/shots-out/` (or under the directory
`TANDEM_SHOTS_OUT` names), and this folder is untouched:

    cd app
    TANDEM_SHOTS_HOME=../Fixtures/port/Home npx playwright test tests/shots/shell.shots.ts

Run a single spec file while iterating; run both home commands and then
`npm run shots:publish` to regenerate every committed picture before opening
the PR:

    TANDEM_SHOTS_HOME=../Fixtures/port/Home npm run shots
    TANDEM_SHOTS_HOME=../Fixtures/port/Board npx playwright test tests/shots/tickets.shots.ts tests/shots/focus.shots.ts
    npm run shots:publish

CI runs both commands in this order on the Linux runner under a virtual display
(`.github/workflows/app.yml`), so a deliberately broken selector is a red job
rather than a human reading a diff.

Three traps, and what the harness does about each. Paint: the clock is pinned
to `2026-10-30T12:00:00Z` with `TZ` at UTC, every scene asserts its face before
it captures, and no step sleeps for a paint. Focus: one worker, one app per
test, and the app is closed after every test, so a second window never steals
the shot. Seed: the named home is copied to the fixed scratch seed
(`/tmp/tandem-shots/Home`) before every test and the app opens the copy, so a
spec that writes cannot change the next spec's picture and no run writes into
the committed fixture.

One seed, one run. Because the seed is one fixed path on the machine, a run
takes an exclusive lock on it (`app/shots/lock.ts`): a second run on the same
machine waits for the lock rather than reading or writing the same seed, and
two runs therefore never share one. A run that dies leaves its lock behind, and
the next run breaks it as stale with a printed notice.

The seed is the named fixture home plus a fixed clock: the page's `Date` is
pinned to `2026-10-30T12:00:00Z` before the first application script runs, and
`TZ` is UTC, so a scene that renders a now-relative time renders one instant.
The seeded paths are fixed too, because the Settings face prints them: a run
in one checkout writes the same bytes as a run in any other.

## What the harness pins

These facts are the harness's, not the machine's, so a picture carries nothing
of the host that made it:

- **the clock**: `2026-10-30T12:00:00.000Z`, with `TZ` at UTC.
- **the colour scheme**: the window emulates `light` before every scene, so a
  dark-mode host photographs the light face; a spec may still flip to dark and
  back inside its own assertions.
- **the pixel scale**: the launch pins the device scale factor at 2, so a
  scene's PNG is twice its CSS width on a 1x and on a 2x display alike. The
  committed widths are that 2x: 1520 for the 760 faces, 2660 for the 1330 board.
- **the seeded paths**: `/tmp/tandem-shots/Home`, `/tmp/tandem-shots/user` and
  `/tmp/tandem-shots/app/Tandem.app`.
- **the doctor's app row**: `TANDEM_APP_PATH` names the seeded fixture app and
  its `Info.plist` names version 1.0.0, so the row reads
  `installed at /tmp/tandem-shots/app/Tandem.app (version 1.0.0)` with no
  quarantine clause. A host's `TANDEM_AGENT_BUNDLE` is deleted from the
  environment, so no installed agent face reaches the picture.

One thing stays the machine's own: the renderer's rasterisation is the
platform's, so a re-render on another platform is not the committed bytes. CI
asserts each scene and never diffs the committed pictures.

A face taller than the window is laid out at its own height (the scroll
constraint lifted, the width kept) and shot whole, so a picture of a table is
the whole table; a board is laid out at the width its six status columns need,
so the picture is the whole board rather than a column and a half of it. The
scratch home is seeded again before every test, so a spec that writes (a drag, a
save, the CLI as a second writer) cannot change the next spec's picture or its
assertions. A failing spec fails the run, and publish refuses a scene it has no
picture for, so a picture and its row cannot drift.

CI runs both home commands on the Linux runner under a virtual display
(`.github/workflows/app.yml`, the agent loop above), so a deliberately broken
selector is a red job rather than a human reading a diff.

The suite asserts a face, not only its contents: every photographed face checks
one computed style against the token it reads from
`app/renderer/src/tokens.css`, because structure alone is not a picture. A
stylesheet that parses into a stylesheet with no matching rules leaves the same
DOM, the same row ids and the same counts on unstyled text.

| file | what it shows |
| --- | --- |
| `electron-shell.png` | the shell at its 760 pt floor: the wordmark, the four tabs (Tickets, Bikeshed, Usage, Settings), the Tickets face it opens on, and the store revision read over `rpc` |
| `usage-face.png` | the **Usage** face over the fixture home: five weeks of reported spend and its coverage, four charts each against its own scale and with a swatched legend that draws each bar's exact colour (Tokens grouping input and output as two bars per column, Cache grouping cache read and cache write, the two grouped charts sharing one colour pair, Total tokens on its own, and Cost), Half and Max gridline ticks naming each scale, All time read as Monday-started weekly columns, the ticket rows and their arithmetic, the Day/Model/Agent/Ticket picker beside the Total/Average per-record toggle, and a ticket row rolled down to the records behind it, each naming its session, agent, model, local start, cost, tokens and the tickets it carried |
| `bikeshed-face.png` | the **Bikeshed** face over the fixture home: two full-bleed columns, the parchment tree at a fixed 260 with its focus picker, reveal control and Health panel, and the paper document at a centred 740 whose sticky breadcrumb names the staged inline note and holds the note size control, the pinned formatting toolbar above the block surface with its pressed Heading action and its eleven line-drawn glyphs, the block surface drawing a bold run, a link, a wikilink chip and a display-math block rendered by KaTeX, and the document footer after it naming the draft a killed save left as a recovery |
| `settings-face.png` | the **Settings** face over the fixture home: the home, the bikeshed, the opencode and claude agent hosts, the theme rule, the bundled and installed versions, and the doctor's checks each named with its state and detail |
| `tickets-list.png` | the **Tickets** face in its list layout over the port's 78-ticket board home (`Fixtures/port/Board`): the quick-add, the Open/Done/All scopes with their counts, the status, assignee and lane filters, the search, and the lane-grouped rows with each ticket's ref, title, status, assignee, lane and blocked mark |
| `tickets-board.png` | the same face on the same 78-ticket board home in its board layout: one column per workflow stage in stage order from Backlog to Done, each head naming the stage and the number of cards in it, each card the lane chip, the title and the ref, assignee and blocked footer, and each column's foot carrying its add |
| `tickets-focus.png` | the **Tickets** face with a project focus set: the focus control naming the active project with its show-all escape, and the board drawing that project's cards across the columns |
| `tickets-keyboard.png` | the keyboard-only pass over the 78-ticket board home: a card selected with the arrows, the first column's add reached with the platform modifier and N, the search with the platform modifier and F, and a ticket moved one stage over with Option and the arrows |
| `tickets-board-sheet.png` | the same face on the same 78-ticket board home with a Ready card's sheet open over the columns: the dialog's sticky band with the title input and its Save in full and the three-fact row under it, the board's sticky column heads and their paper bands drawn beneath the dimmed backdrop, the sheet the top surface of the face |
| `ticket-sheet-passes.png` | the ticket sheet over the fixture home with its pass-request buttons: refine enabled on a new backlog ticket, build and review disabled with their reasons in place, each a request the next dispatch picks up |
