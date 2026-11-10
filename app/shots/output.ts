import { existsSync, mkdirSync, readFileSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'

import { ShotsRefused } from './home'
import type { Scene } from './scenes'

/**
 * Where a run writes its pictures (`docs/screenshots/README.md`).
 *
 * A run writes scratch, always: `app/shots-out/` in the checkout, or the
 * directory `TANDEM_SHOTS_OUT` names for a one-spec look. The committed folder
 * is written by `npm run shots:publish` alone, which copies a run's bytes and
 * never re-renders, so a local run cannot dirty the tree and one machine's
 * pixels reach `docs/screenshots` only when a human asks for them.
 */
export const OUT_VAR = 'TANDEM_SHOTS_OUT'

/** The committed folder: publish's output, and no run's. */
export function committedDir(repoRoot: string): string {
  return join(resolve(repoRoot), 'docs', 'screenshots')
}

/**
 * The run's own scratch directory in the checkout. Both home commands write
 * here, so a publish after the pair resolves every scene; it is never the
 * committed folder, and `app/.gitignore` keeps it out of the tree.
 */
export function scratchDir(repoRoot: string): string {
  return join(resolve(repoRoot), 'app', 'shots-out')
}

/**
 * Where this run's pictures go, resolved and not created: the override, or the
 * checkout's scratch directory. A relative override resolves against the
 * working directory, like any other output path.
 */
export function outputDirPath(env: NodeJS.ProcessEnv, repoRoot: string): string {
  const named = env[OUT_VAR]
  if (named === undefined) return scratchDir(repoRoot)
  if (named.trim() === '') {
    throw new ShotsRefused(
      `${OUT_VAR} is set but blank. Unset it to write ${scratchDir(repoRoot)}, or name a scratch directory for a one-spec look.`,
    )
  }
  return resolve(named)
}

/** The run's output directory, created if missing. A file there is refused. */
export function shotsOutputDir(env: NodeJS.ProcessEnv, repoRoot: string): string {
  const dir = outputDirPath(env, repoRoot)
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true })
  if (!statSync(dir).isDirectory()) {
    throw new ShotsRefused(`${dir} is not a directory. The output is a directory only, created if missing.`)
  }
  return dir
}

/** The file a scene's PNG is written to under this run's output. */
export function screenshotPath(env: NodeJS.ProcessEnv, repoRoot: string, entry: Scene): string {
  return join(shotsOutputDir(env, repoRoot), entry.file)
}

/**
 * The width a PNG's header names, in pixels: the run's picture at the pinned
 * scale, which is the one fact about a picture a spec can assert from the file
 * rather than from the page.
 */
export function pngWidth(file: string): number {
  const header = readFileSync(file)
  if (header.length < 24 || header.toString('latin1', 12, 16) !== 'IHDR') {
    throw new Error(`${file} is not a PNG: a capture writes one, and its header names the pixel size.`)
  }
  return header.readUInt32BE(16)
}
