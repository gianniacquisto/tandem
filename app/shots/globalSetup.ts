import { seedApp, seedHome, seedPath, seedRoot, seedUserHome, seedUserPath, shotsSource } from './home'
import { acquireSeedLock, releaseSeedLock, type SeedLock } from './lock'
import { REPO_ROOT } from './roots'

export interface ShotsSetupOverrides {
  repoRoot?: string
  seedRoot?: string
  waitBoundMs?: number
}

/**
 * The lock this run holds, released by [`globalTeardown`]. Playwright runs both
 * hooks in the runner process, so the handle is the same process's; a future
 * Playwright that split them would leave the lock behind and the next run
 * breaks it as stale, which costs a notice and no correctness.
 */
let held: SeedLock | null = null

/** Release the lock this run took; idempotent, and a run that took none frees nothing. */
export function releaseHeldSeedLock(): void {
  releaseSeedLock(held)
  held = null
}

/**
 * The run's seed, before any window opens: the named fixture home is checked
 * against the rule and copied to the scratch path the specs read, the fixture
 * user home is created beside it, and the fixture app is written beside both so
 * the Settings face reads a pinned version rather than the host's.
 *
 * A refusal here fails the whole run with no picture written.
 *
 * The run takes the seed lock before its first write to the root, so a run that
 * waits has written nothing while it waits. The seed root is one fixed path on
 * the machine (`home.ts`), and the lock is what keeps a second run from copying
 * over the first's seed mid-flight.
 */
export default async function globalSetup(overrides: ShotsSetupOverrides = {}): Promise<void> {
  const repoRoot = overrides.repoRoot ?? REPO_ROOT
  const root = overrides.seedRoot ?? seedRoot()
  const source = shotsSource(process.env, repoRoot)

  held = await acquireSeedLock({ root, waitBoundMs: overrides.waitBoundMs })

  seedHome(source, seedPath(root))
  seedUserHome(seedUserPath(root))
  seedApp(root)
}
