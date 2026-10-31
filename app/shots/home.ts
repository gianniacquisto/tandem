import { cpSync, mkdirSync, renameSync, rmSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'

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

/**
 * Where a run seeds, and the reason it is not under the checkout: the seed's
 * path is printed by the Settings face, so a picture taken in one checkout must
 * carry the same path as a picture taken in any other. A fixed scratch root is
 * the only spelling that gives that, and it is clearly scratch.
 *
 * It is refreshed by every run (see [`seedHome`]) and deliberately never
 * removed: two runs on one machine share it, and a run that deleted it would
 * break the other's next read. A stale seed cannot outlive a run, because the
 * next run replaces it wholesale.
 */
export function seedRoot(): string {
  if (process.platform === 'win32') return resolve(process.env['TEMP'] ?? '.', 'tandem-shots')
  return '/tmp/tandem-shots'
}

/** The scratch copy of the fixture home a run's app reads. */
export function seedPath(): string {
  return join(seedRoot(), 'Home')
}

/**
 * The scratch fixture user home: the `HOME` a scene's Settings face reads, so
 * the host roots it prints are the seed's and never a human's
 * `~/.config/opencode`, `~/.claude` or `~/.agents`.
 */
export function seedUserPath(): string {
  return join(seedRoot(), 'user')
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
  return replace(target, (staging) => {
    cpSync(source, staging, { recursive: true })
  })
}

/**
 * The fixture user home: the three host roots the setup report reads, created
 * empty. They are directories and nothing more, because a picture of Settings
 * has to show a host's path without showing a host's contents.
 */
export function seedUserHome(target: string): string {
  return replace(target, (staging) => {
    mkdirSync(join(staging, '.config', 'opencode'), { recursive: true })
    mkdirSync(join(staging, '.claude'), { recursive: true })
    mkdirSync(join(staging, '.agents'), { recursive: true })
  })
}

/**
 * A seed is built beside its path and moved into place, so a reader never sees
 * half a copy: the only gap is the remove and the rename around it, and both
 * are one directory entry each.
 */
function replace(target: string, build: (staging: string) => void): string {
  const staging = `${target}.staging`
  rmSync(staging, { recursive: true, force: true })
  mkdirSync(dirname(target), { recursive: true })
  build(staging)
  rmSync(target, { recursive: true, force: true })
  renameSync(staging, target)
  return target
}
