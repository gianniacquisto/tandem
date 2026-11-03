import { isScratchOutput } from './output'
import { writeCatalogue } from './catalogue'
import { REPO_ROOT } from './roots'

/**
 * The catalogue is kept by the same run that takes the pictures. A scene whose
 * PNG is missing throws here, so a green run and a documented picture cannot
 * disagree.
 *
 * A scratch look (`TANDEM_SHOTS_OUT`) writes no catalogue: its pictures live
 * outside the committed folder, so there is no row to keep. The seed is left
 * in place: it is a fixed scratch path shared by runs on one machine
 * (`home.ts`), and the next run replaces it wholesale.
 *
 * The root is a parameter so a test can point the teardown at a fixture;
 * Playwright calls with its own config, which carries no root, so a run keeps
 * the repository's.
 */
export default function globalTeardown(overrides: { repoRoot?: string } = {}): void {
  if (isScratchOutput(process.env)) return
  writeCatalogue({ repoRoot: overrides.repoRoot ?? REPO_ROOT })
}
