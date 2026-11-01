import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import {
  CATALOGUE_MARKER,
  cataloguePath,
  renderCatalogue,
  templatePath,
  writeCatalogue,
} from '../shots/catalogue'
import { REPO_ROOT } from '../shots/roots'
import { SCENES, type Scene } from '../shots/scenes'

const scenes: Scene[] = [
  { id: 'first', file: 'first.png', description: 'the first scene' },
  { id: 'second', file: 'second.png', description: 'the second scene' },
]

const scratch: string[] = []

function fixtureRepo(lines: string[]): string {
  const root = mkdtempSync(join(tmpdir(), 'tandem-catalogue-'))
  scratch.push(root)
  mkdirSync(join(root, 'docs', 'screenshots'), { recursive: true })
  writeFileSync(templatePath(root), `# Screenshots\n\n${CATALOGUE_MARKER}\n\n| file | what it shows |\n`)
  for (const file of lines) writeFileSync(join(root, 'docs', 'screenshots', file), 'png')
  return root
}

afterEach(() => {
  for (const root of scratch.splice(0)) rmSync(root, { recursive: true, force: true })
})

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

  it('refuses a catalogue for a picture the run did not make', () => {
    const root = fixtureRepo(['first.png'])

    expect(() => writeCatalogue({ repoRoot: root, scenes })).toThrow(/second\.png/)
  })

  it('writes the catalogue into the repository when every picture exists', () => {
    const root = fixtureRepo(['first.png', 'second.png'])

    writeCatalogue({ repoRoot: root, scenes })

    const written = readFileSync(cataloguePath(root), 'utf8')
    expect(written).toContain('| `first.png` | the first scene |')
    expect(written).toContain('| `second.png` | the second scene |')
  })

  it('keeps the committed catalogue the one the template and the manifest render', () => {
    // The run writes the table from `scenes.ts`, so a reworded scene row is stale
    // until the next run: a row that describes a layout the pictures no longer
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
})
