/**
 * The scenes one run photographs: the id a spec asks for, the file under
 * `docs/screenshots/` it writes, and the line the catalogue carries.
 *
 * This list is the run's whole output, and the catalogue is generated from it,
 * so a picture and its row cannot drift. A face adds its scene here and its
 * spec beside `tests/shots/`; the same run then writes both the PNG and the
 * README row.
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
      'the shell at its 760 pt floor: the wordmark, the four kept tabs (Tickets, Bikeshed, Usage, Settings), the Tickets face it opens on, and the store revision read over `rpc`',
  },
  {
    id: 'usage',
    file: 'usage-face.png',
    description:
      'the **Usage** face over the fixture home: the reported spend and its coverage, the cost-per-day bar chart above the picker, the ticket rows and their arithmetic, the Day/Model/Agent/Ticket picker, and a ticket row rolled down to the records behind it, each naming its session, agent, model, local start, cost, tokens and the tickets it carried',
  },
  {
    id: 'bikeshed',
    file: 'bikeshed-face.png',
    description:
      'the **Bikeshed** face over the fixture home: the scaffold tree with its reveal control, the committed CodeMirror bundle editing the fixture project note in its own frame, the Backlinks dock naming the Index note, the Health panel reporting the daily note as an orphan, and a draft a killed save left named as a recovery',
  },
  {
    id: 'settings',
    file: 'settings-face.png',
    description:
      'the **Settings** face over the fixture home: the home, the bikeshed, the three agent hosts, the theme rule, the bundled and installed versions, the doctor\'s checks each named with its state and detail, and the harness sign-in section',
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
      'the same face on the same 78-ticket board home in its board layout: the lanes as rows, each named with its count and holding one cell per status, the six status columns with On hold parked last, and the same rows set on cards in the cell of their lane and status',
  },
  {
    id: 'tickets-focus',
    file: 'tickets-focus.png',
    description:
      'the **Tickets** face with a project focus set: the focus control naming the active lane with its show-all escape, and the board drawing that lane row alone',
  },
  {
    id: 'tickets-keyboard',
    file: 'tickets-keyboard.png',
    description:
      'the keyboard-only pass over the 78-ticket board home: a row selected with the arrows, the quick-add reached with the platform modifier and N, the search with the platform modifier and F, and a ticket moved one status over, inside its own lane, with Option and the arrows',
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
