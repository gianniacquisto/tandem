import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

import { CATALOGUE_MARKER, cataloguePath, renderCatalogue, templatePath } from '../shots/catalogue'
import { REPO_ROOT } from '../shots/roots'
import { SCENES, type Scene } from '../shots/scenes'

const scenes: Scene[] = [
  { id: 'first', file: 'first.png', description: 'the first scene' },
  { id: 'second', file: 'second.png', description: 'the second scene' },
]

const shots = join(REPO_ROOT, 'app', 'shots')

describe('the screenshot catalogue', () => {
  it('renders one row per scene, in the manifest order', () => {
    const template = `# Screenshots\n\nWords.\n\n${CATALOGUE_MARKER}\n\nTail.\n`

    const rendered = renderCatalogue(template, scenes)

    expect(rendered).not.toContain(CATALOGUE_MARKER)
    expect(rendered).toContain('| file | what it shows |')
    expect(rendered).toContain('| `first.png` | the first scene |')
    expect(rendered.indexOf('`first.png`')).toBeLessThan(rendered.indexOf('`second.png`'))
    expect(rendered.indexOf('Words.')).toBeLessThan(rendered.indexOf('`first.png`'))
    expect(rendered.indexOf('`second.png`')).toBeLessThan(rendered.indexOf('Tail.'))
  })

  it('refuses a template that lost its marker, so the table cannot vanish', () => {
    expect(() => renderCatalogue('# Screenshots\n', scenes)).toThrow(/marker/)
  })

  it('keeps the committed catalogue the one the template and the manifest render', () => {
    // Publish writes the table from `scenes.ts`, so a reworded scene row is stale
    // until the next publish: a row that describes a layout the pictures no longer
    // show is the drift this check exists to stop.
    //
    // The endings are compared as one: the table the writer builds is LF, while a
    // Windows checkout hands both files back CRLF, so a raw compare fails there
    // on every row without a word of prose having changed.
    const root = REPO_ROOT
    const lines = (text: string): string => text.replace(/\r\n/g, '\n')

    expect(lines(readFileSync(cataloguePath(root), 'utf8'))).toBe(
      lines(renderCatalogue(readFileSync(templatePath(root), 'utf8'), SCENES)),
    )
  })

  it('ships one scene per file, and every file a picture', () => {
    expect(new Set(SCENES.map((scene) => scene.id)).size).toBe(SCENES.length)
    expect(new Set(SCENES.map((scene) => scene.file)).size).toBe(SCENES.length)
    for (const scene of SCENES) expect(scene.file).toMatch(/\.png$/)
  })

  // The catalogue and the committed set have one writer, and it is the copy: a
  // run writes its scratch pictures and nothing under `docs/screenshots`, so a
  // local run cannot dirty the tree. These two read the harness's own source,
  // because the guard is about what the suite may call, not what one call does.
  it('has publish as the only writer of the catalogue', () => {
    const writers = readdirSync(shots)
      .filter((file) => file.endsWith('.ts') && file !== 'catalogue.ts')
      .filter((file) => readFileSync(join(shots, file), 'utf8').includes('renderCatalogue('))

    expect(writers).toEqual(['publish.ts'])
  })

  it('declares a teardown that releases the seed lock and writes no catalogue', () => {
    const config = readFileSync(join(REPO_ROOT, 'app', 'playwright.config.ts'), 'utf8')
    const teardown = readFileSync(join(shots, 'globalTeardown.ts'), 'utf8')

    expect(config).toContain('globalTeardown')
    expect(teardown).toContain('releaseHeldSeedLock')
    expect(teardown).not.toMatch(/from '\.\/catalogue'/)
  })
})
