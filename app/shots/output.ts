import { existsSync, mkdirSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'

import { ShotsRefused } from './home'
import type { Scene } from './scenes'

/**
 * The scratch look a session takes mid-change (`docs/screenshots/README.md`).
 *
 * When set, a run's PNGs go to the named directory and the catalogue is left
 * alone, so a look writes nothing under `docs/screenshots` and no catalogue
 * row. When unset, a run writes into the committed folder and keeps the
 * catalogue, exactly as before. The override names a directory only, created
 * if missing; a blank override and a path that names a file are refused
 * before any window opens.
 */
export const OUT_VAR = 'TANDEM_SHOTS_OUT'

/** The committed folder a run writes into when no override names another. */
export function committedDir(repoRoot: string): string {
  return join(resolve(repoRoot), 'docs', 'screenshots')
}

/** Whether this run is a scratch look: pictures, never the catalogue. */
export function isScratchOutput(env: NodeJS.ProcessEnv): boolean {
  const named = env[OUT_VAR]
  return named !== undefined && named.trim() !== ''
}

/**
 * Where a run's PNGs go: the override, or the committed folder. A relative
 * override resolves against the working directory, like any other output path.
 */
export function shotsOutputDir(env: NodeJS.ProcessEnv, repoRoot: string): string {
  const named = env[OUT_VAR]
  if (named === undefined) return committedDir(repoRoot)
  if (named.trim() === '') {
    throw new ShotsRefused(
      `${OUT_VAR} is set but blank. Unset it to write the committed pictures, or name a scratch directory for a look that writes nothing under docs/screenshots.`,
    )
  }

  const dir = resolve(named)
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true })
  if (!statSync(dir).isDirectory()) {
    throw new ShotsRefused(`${OUT_VAR}=${named} is not a directory. The override is a directory only, created if missing.`)
  }
  return dir
}

/** The file a scene's PNG is written to under this run's output. */
export function screenshotPath(env: NodeJS.ProcessEnv, repoRoot: string, entry: Scene): string {
  return join(shotsOutputDir(env, repoRoot), entry.file)
}
