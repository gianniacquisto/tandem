/**
 * The scenes one run photographs: the id a spec asks for, the file
 * `npm run shots:publish` copies into `docs/screenshots/`, and the line the
 * catalogue carries.
 *
 * This list is the run's whole output, and the catalogue is generated from it,
 * so a picture and its row cannot drift. A face adds its scene here and its
 * spec beside `tests/shots/`; the run writes the PNG into its own output
 * directory, and publish copies it and renders its row.
 *
 * A kept surface whose ticket has not landed is not listed: a picture of a face
 * frame is not a picture of the face. The list grows as the faces do.
 */
export type Scene = {
  id: string
  file: string
  description: string
}

export const SCENES: readonly Scene[] = [
  {
    id: 'shell',
    file: 'electron-shell.png',
    description:
      'the shell at its 760 pt floor: the wordmark, the four tabs (Tickets, Bikeshed, Usage, Settings), the Tickets face it opens on, and the store revision read over `rpc`',
  },
  {
    id: 'usage',
    file: 'usage-face.png',
    description:
      'the **Usage** face over the fixture home: five weeks of reported spend and its coverage, four charts each against its own scale and with a swatched legend that draws each bar\'s exact colour (Tokens grouping input and output as two bars per column, Cache grouping cache read and cache write, the two grouped charts sharing one colour pair, Total tokens on its own, and Cost), Half and Max gridline ticks naming each scale, All time read as Monday-started weekly columns, the ticket rows and their arithmetic, the Day/Model/Agent/Ticket picker beside the Total/Average per-record toggle, and a ticket row rolled down to the records behind it, each naming its session, agent, model, local start, cost, tokens and the tickets it carried',
  },
  {
    id: 'bikeshed',
    file: 'bikeshed-face.png',
    description:
      'the **Bikeshed** face over the fixture home: two full-bleed columns, the parchment tree at a fixed 260 with its focus picker, reveal control and Health panel, and the paper document at a centred 740 whose sticky breadcrumb names the staged inline note and holds the note size control, the pinned formatting toolbar above the block surface with its pressed Heading action and its eleven line-drawn glyphs, the block surface drawing a bold run, a link, a wikilink chip and a display-math block rendered by KaTeX, and the document footer after it naming the draft a killed save left as a recovery',
  },
  {
    id: 'settings',
    file: 'settings-face.png',
    description:
      'the **Settings** face over the fixture home: the home, the bikeshed, the three agent hosts, the theme rule, the bundled and installed versions, and the doctor\'s checks each named with its state and detail',
  },
  {
    id: 'tickets-list',
    file: 'tickets-list.png',
    description:
      'the **Tickets** face in its list layout over the port\'s 78-ticket board home (`Fixtures/port/Board`): the quick-add, the Open/Done/All scopes with their counts, the status, assignee and lane filters, the search, and the lane-grouped rows with each ticket\'s ref, title, status, assignee, lane and blocked mark',
  },
  {
    id: 'tickets-board',
    file: 'tickets-board.png',
    description:
      'the same face on the same 78-ticket board home in its board layout: one column per workflow stage in stage order from Backlog to Done, each head naming the stage and the number of cards in it, each card the lane chip, the title and the ref, assignee and blocked footer, and each column\'s foot carrying its add',
  },
  {
    id: 'tickets-focus',
    file: 'tickets-focus.png',
    description:
      'the **Tickets** face with a project focus set: the focus control naming the active project with its show-all escape, and the board drawing that project\'s cards across the columns',
  },
  {
    id: 'tickets-keyboard',
    file: 'tickets-keyboard.png',
    description:
      'the keyboard-only pass over the 78-ticket board home: a card selected with the arrows, the first column\'s add reached with the platform modifier and N, the search with the platform modifier and F, and a ticket moved one stage over with Option and the arrows',
  },
  {
    id: 'ticket-sheet-passes',
    file: 'ticket-sheet-passes.png',
    description:
      'the ticket sheet over the fixture home with its pass-request buttons: refine enabled on a new backlog ticket, build and review disabled with their reasons in place, each a request the next dispatch picks up',
  },
]

/** The named scene, or a failure naming the ones the manifest does hold. */
export function scene(id: string): Scene {
  const found = SCENES.find((entry) => entry.id === id)
  if (found === undefined) {
    throw new Error(`No scene is named ${id}: the manifest holds ${SCENES.map((entry) => entry.id).join(', ')}.`)
  }
  return found
}
