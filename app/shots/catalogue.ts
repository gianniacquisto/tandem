import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { SCENES, type Scene } from './scenes'

/**
 * The catalogue under `docs/screenshots/README.md`, kept by the run that makes
 * the pictures: the prose lives in the template, the table is generated from
 * the scene manifest, and a scene whose file is missing fails the run instead
 * of leaving a row over nothing.
 */
export const CATALOGUE_MARKER = '<!-- scenes -->'

export function templatePath(repoRoot: string): string {
  return join(repoRoot, 'docs', 'screenshots', 'README.template.md')
}

export function cataloguePath(repoRoot: string): string {
  return join(repoRoot, 'docs', 'screenshots', 'README.md')
}

/** The template with its marker replaced by one row per scene, in order. */
export function renderCatalogue(template: string, scenes: readonly Scene[]): string {
  if (!template.includes(CATALOGUE_MARKER)) {
    throw new Error(
      `The screenshot template has no ${CATALOGUE_MARKER} marker, so the scene table would vanish. Put the marker where the table belongs.`,
    )
  }

  const table = [
    '| file | what it shows |',
    '| --- | --- |',
    ...scenes.map((entry) => `| \`${entry.file}\` | ${entry.description} |`),
  ].join('\n')

  return template.replace(CATALOGUE_MARKER, table)
}

/** Write the catalogue, refusing one that would document a picture nobody made. */
export function writeCatalogue(options: { repoRoot: string; scenes?: readonly Scene[] }): void {
  const scenes = options.scenes ?? SCENES
  const screenshots = join(options.repoRoot, 'docs', 'screenshots')
  const missing = scenes.filter((entry) => !existsSync(join(screenshots, entry.file)))

  if (missing.length > 0) {
    throw new Error(
      `The catalogue names ${missing.map((entry) => entry.file).join(', ')} and this run wrote no such picture: a row over nothing is worse than no row.`,
    )
  }

  const template = readFileSync(templatePath(options.repoRoot), 'utf8')
  writeFileSync(cataloguePath(options.repoRoot), renderCatalogue(template, scenes))
}
