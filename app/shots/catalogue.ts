import { join } from 'node:path'

import { SCENES, type Scene } from './scenes'

/**
 * The catalogue under `docs/screenshots/README.md`: the prose lives in the
 * template, the table is generated from the scene manifest, and `publish.ts` is
 * its one writer, so a run writes no catalogue and a picture and its row cannot
 * drift.
 */
export const CATALOGUE_MARKER = '<!-- scenes -->'

export function templatePath(repoRoot: string): string {
  return join(repoRoot, 'docs', 'screenshots', 'README.template.md')
}

export function cataloguePath(repoRoot: string): string {
  return join(repoRoot, 'docs', 'screenshots', 'README.md')
}

/** The template with its marker replaced by one row per scene, in order. */
export function renderCatalogue(template: string, scenes: readonly Scene[] = SCENES): string {
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
