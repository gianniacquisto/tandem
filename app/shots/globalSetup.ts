import { seedHome, seedPath, seedUserHome, seedUserPath, shotsSource } from './home'
import { REPO_ROOT } from './roots'

/**
 * The run's seed, before any window opens: the named fixture home is checked
 * against the rule and copied to the scratch path the specs read, and the
 * fixture user home is created beside it. A refusal here fails the whole run
 * with no picture written.
 */
export default function globalSetup(): void {
  const source = shotsSource(process.env, REPO_ROOT)
  seedHome(source, seedPath())
  seedUserHome(seedUserPath())
}
