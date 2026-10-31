import { cpSync, mkdirSync, rmSync } from 'node:fs'
import { dirname, resolve } from 'node:path'

import { isFixtureHome } from '../src/main/shot'

/**
 * The home a shots run reads, and the rule that keeps a human's notes out of a
 * public repository (`docs/screenshots/README.md`).
 *
 * The home is an argument with no default: a run that names none is refused
 * before a window opens. It must resolve under the repository's own
 * `Fixtures/`, so `~/.tandem` and every other real home is refused by the same
 * predicate the shell uses for its own capture.
 */
export const HOME_VAR = 'TANDEM_SHOTS_HOME'

/** A refusal of the home: the run stops before it photographs anything. */
export class ShotsRefused extends Error {
  override name = 'ShotsRefused'
}

/** Where a run seeds its scratch copy of the fixture home (gitignored). */
export function seedPath(repoRoot: string): string {
  return resolve(repoRoot, 'app', 'shots', '.seed', 'Home')
}

/** The source home a run was told to read, or a refusal saying why not. */
export function shotsSource(env: NodeJS.ProcessEnv, repoRoot: string): string {
  const fixtures = resolve(repoRoot, 'Fixtures')
  const named = env[HOME_VAR]

  if (named === undefined || named.trim() === '') {
    throw new ShotsRefused(
      `${HOME_VAR} is not set. Every picture comes from a fixture home (docs/screenshots/README.md): name a home under ${fixtures}, never ~/.tandem.`,
    )
  }

  if (!isFixtureHome(named, fixtures)) {
    throw new ShotsRefused(
      `${HOME_VAR}=${named} is outside ${fixtures}. Refused: a picture comes from the fixture home, never a real home (~/.tandem) and never a path outside this repository's fixtures.`,
    )
  }

  return resolve(named)
}

/**
 * One run, one seed: the committed home is content, so the run copies it and
 * points the app at the copy. A spec that writes (a face's own save, a rename)
 * then leaves the committed fixture untouched, and the next run starts from
 * the same bytes.
 */
export function seedHome(source: string, target: string): string {
  rmSync(target, { recursive: true, force: true })
  mkdirSync(dirname(target), { recursive: true })
  cpSync(source, target, { recursive: true })
  return target
}
