import { isScratchOutput } from './output'
import { writeCatalogue } from './catalogue'
import { releaseHeldSeedLock } from './globalSetup'
import { REPO_ROOT } from './roots'

export interface ShotsTeardownOverrides {
  repoRoot?: string
}

/**
 * The catalogue is kept by the same run that takes the pictures. A scene whose
 * PNG is missing throws here, so a green run and a documented picture cannot
 * disagree.
 *
 * A scratch look (`TANDEM_SHOTS_OUT`) writes no catalogue: its pictures live
 * outside the committed folder, so there is no row to keep. The seed is left
 * in place: it is the fixed scratch path a run reads, a red run's seed is the
 * evidence of what the app read, and the next run replaces it wholesale.
 *
 * The seed lock is released last, in a `finally`, so a run whose tests failed
 * and a scratch look that returns early both free the seed for the next run.
 * The lock is not the seed: only the lock goes.
 *
 * The root is a parameter so a test can point the teardown at a fixture;
 * Playwright calls with its own config, which carries no root, so a run keeps
 * the repository's.
 */
export default function globalTeardown(overrides: ShotsTeardownOverrides = {}): void {
  try {
    if (isScratchOutput(process.env)) return
    writeCatalogue({ repoRoot: overrides.repoRoot ?? REPO_ROOT })
  } finally {
    releaseHeldSeedLock()
  }
}
