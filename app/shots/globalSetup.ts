import { seedApp, seedHome, seedPath, seedUserHome, seedUserPath, shotsSource } from './home'
import { REPO_ROOT } from './roots'

/**
 * The run's seed, before any window opens: the named fixture home is checked
 * against the rule and copied to the scratch path the specs read, the fixture
 * user home is created beside it, and the fixture app is written beside both so
 * the Settings face reads a pinned version rather than the host's.
 *
 * A refusal here fails the whole run with no picture written.
 */
export default function globalSetup(): void {
  const source = shotsSource(process.env, REPO_ROOT)
  seedHome(source, seedPath())
  seedUserHome(seedUserPath())
  seedApp()
}
