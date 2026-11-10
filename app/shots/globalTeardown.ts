import { releaseHeldSeedLock } from './globalSetup'

/**
 * The run's exit hook, and the only thing it does: free the seed lock this run
 * took, so the next run does not break it as stale. A run writes no catalogue
 * and nothing under `docs/screenshots`; `npm run shots:publish` is that
 * folder's one writer.
 *
 * The release is idempotent, so a run whose tests failed and a run that took no
 * lock both free the seed for the next run. The lock is not the seed: only the
 * lock goes.
 */
export default function globalTeardown(): void {
  releaseHeldSeedLock()
}
