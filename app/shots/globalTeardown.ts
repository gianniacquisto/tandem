import { writeCatalogue } from './catalogue'
import { REPO_ROOT } from './roots'

/**
 * The catalogue is kept by the same run that takes the pictures. A scene whose
 * PNG is missing throws here, so a green run and a documented picture cannot
 * disagree.
 *
 * The seed is left in place: it is a fixed scratch path shared by runs on one
 * machine (`home.ts`), and the next run replaces it wholesale.
 */
export default function globalTeardown(): void {
  writeCatalogue({ repoRoot: REPO_ROOT })
}
