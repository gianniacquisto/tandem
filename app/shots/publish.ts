import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { cataloguePath, renderCatalogue, templatePath } from './catalogue'
import { ShotsRefused } from './home'
import { committedDir, outputDirPath } from './output'
import { REPO_ROOT } from './roots'
import { SCENES, type Scene } from './scenes'

/**
 * The explicit copy (`docs/screenshots/README.md`): the committed set moves
 * only when a human asks, and only from a run.
 *
 * It copies every manifest picture out of the run's output directory, then
 * writes the catalogue the template and the manifest render. It never
 * re-renders, so the published files are the bytes the run made, and the run
 * itself never touches `docs/screenshots`.
 *
 * Every refusal happens before the first copy: a scene the run did not make is
 * named by id and file, and a template that lost its marker is refused while
 * the committed folder still holds what it held. So the folder is left as it
 * was or written whole, never half.
 */
export function publish(
  options: { repoRoot?: string; env?: NodeJS.ProcessEnv; scenes?: readonly Scene[] } = {},
): string[] {
  const repoRoot = options.repoRoot ?? REPO_ROOT
  const env = options.env ?? process.env
  const scenes = options.scenes ?? SCENES
  const source = outputDirPath(env, repoRoot)

  const missing = scenes.filter((entry) => !existsSync(join(source, entry.file)))
  if (missing.length > 0) {
    throw new ShotsRefused(
      `This run made no picture for ${missing.map((entry) => `${entry.id} (${entry.file})`).join(', ')}: publish copies the run's pictures and never re-renders, so run the home and board commands first. Nothing was written.`,
    )
  }

  const catalogue = renderCatalogue(readFileSync(templatePath(repoRoot), 'utf8'), scenes)
  const target = committedDir(repoRoot)
  mkdirSync(target, { recursive: true })
  for (const entry of scenes) copyFileSync(join(source, entry.file), join(target, entry.file))
  writeFileSync(cataloguePath(repoRoot), catalogue)
  return scenes.map((entry) => join(target, entry.file))
}

/**
 * `npm run shots:publish`'s entry. The guard is the invoked path, so the tests
 * can import `publish` without copying anything: the runner executes this file,
 * and no other caller does.
 */
if (process.argv[1]?.endsWith('publish.ts') === true) {
  try {
    for (const file of publish()) console.log(`copied ${file}`)
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exitCode = 1
  }
}
