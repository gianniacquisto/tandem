/**
 * The scenes one run photographs: the id a spec asks for, the file under
 * `docs/screenshots/` it writes, and the line the catalogue carries.
 *
 * This list is the run's whole output, and the catalogue is generated from it,
 * so a picture and its row cannot drift. A face adds its scene here and its
 * spec beside `tests/shots/`; the same run then writes both the PNG and the
 * README row.
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
]

/** The named scene, or a failure naming the ones the manifest does hold. */
export function scene(id: string): Scene {
  const found = SCENES.find((entry) => entry.id === id)
  if (found === undefined) {
    throw new Error(`No scene is named ${id}: the manifest holds ${SCENES.map((entry) => entry.id).join(', ')}.`)
  }
  return found
}
