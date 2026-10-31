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
      'the shell at its 760 pt floor: the wordmark, the four kept tabs (Tickets, Bikeshed, Usage, Settings) and the store revision read over `rpc`',
  },
  {
    id: 'usage',
    file: 'usage-face.png',
    description:
      'the **Usage** face over the fixture home: the reported spend and its coverage, the ticket rows and their arithmetic, the Day/Model/Agent/Ticket picker, and a ticket row rolled down to the records behind it, each naming its session, agent, model, local start, cost, tokens and the tickets it carried',
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
      'the **Settings** face over the fixture home: the home, the bikeshed, the three agent hosts, the theme rule, the bundled and installed versions, and the doctor\'s checks each named with its state and detail',
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
