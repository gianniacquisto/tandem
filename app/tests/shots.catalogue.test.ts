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

  it('ships one scene per file, and every file a picture', () => {
    expect(new Set(SCENES.map((scene) => scene.id)).size).toBe(SCENES.length)
    expect(new Set(SCENES.map((scene) => scene.file)).size).toBe(SCENES.length)
    for (const scene of SCENES) expect(scene.file).toMatch(/\.png$/)
  })
})
