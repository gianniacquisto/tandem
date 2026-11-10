import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import { CATALOGUE_MARKER, cataloguePath, templatePath } from '../shots/catalogue'
import { OUT_VAR } from '../shots/output'
import { publish } from '../shots/publish'
import type { Scene } from '../shots/scenes'

const scenes: Scene[] = [
  { id: 'first', file: 'first.png', description: 'the first scene' },
  { id: 'second', file: 'second.png', description: 'the second scene' },
]

const scratch: string[] = []

function temp(prefix: string): string {
  const dir = mkdtempSync(join(tmpdir(), prefix))
  scratch.push(dir)
  return dir
}

/** A checkout with a committed set and a run's scratch directory beside it. */
function fixtureRepo(run: Record<string, string>): string {
  const root = temp('tandem-publish-')
  mkdirSync(join(root, 'docs', 'screenshots'), { recursive: true })
  writeFileSync(
    templatePath(root),
    `# Screenshots\n\nWords.\n\n${CATALOGUE_MARKER}\n`,
  )
  writeFileSync(join(root, 'docs', 'screenshots', 'first.png'), 'committed')
  const out = join(root, 'app', 'shots-out')
  mkdirSync(out, { recursive: true })
  for (const [file, body] of Object.entries(run)) writeFileSync(join(out, file), body)
  return root
}

afterEach(() => {
  delete process.env[OUT_VAR]
  for (const dir of scratch.splice(0)) rmSync(dir, { recursive: true, force: true })
})

describe('publishing the run', () => {
  it('copies every scene into the committed folder and renders the catalogue from the manifest', () => {
    const root = fixtureRepo({ 'first.png': 'run one', 'second.png': 'run two' })

    const written = publish({ repoRoot: root, env: {}, scenes })

    expect(written).toHaveLength(2)
    expect(readFileSync(join(root, 'docs', 'screenshots', 'first.png'), 'utf8')).toBe('run one')
    expect(readFileSync(join(root, 'docs', 'screenshots', 'second.png'), 'utf8')).toBe('run two')
    const catalogue = readFileSync(cataloguePath(root), 'utf8')
    expect(catalogue).toContain('| `first.png` | the first scene |')
    expect(catalogue).toContain('| `second.png` | the second scene |')
    expect(catalogue).toContain('Words.')
    expect(catalogue).not.toContain(CATALOGUE_MARKER)
  })

  it('copies from a named scratch directory, so a one-spec look publishes the same way', () => {
    const root = fixtureRepo({})
    const look = temp('tandem-publish-look-')
    writeFileSync(join(look, 'first.png'), 'look one')
    writeFileSync(join(look, 'second.png'), 'look two')

    publish({ repoRoot: root, env: { [OUT_VAR]: look }, scenes })

    expect(readFileSync(join(root, 'docs', 'screenshots', 'second.png'), 'utf8')).toBe('look two')
  })

  it('refuses a scene the run did not make, names it, and changes nothing', () => {
    const root = fixtureRepo({ 'first.png': 'run one' })

    expect(() => publish({ repoRoot: root, env: {}, scenes })).toThrow(/second \(second\.png\)/)

    expect(readFileSync(join(root, 'docs', 'screenshots', 'first.png'), 'utf8')).toBe('committed')
    expect(existsSync(join(root, 'docs', 'screenshots', 'second.png'))).toBe(false)
    expect(existsSync(cataloguePath(root))).toBe(false)
  })

  it('refuses a run directory that is not there at all, naming every scene', () => {
    const root = fixtureRepo({})
    rmSync(join(root, 'app', 'shots-out'), { recursive: true, force: true })

    expect(() => publish({ repoRoot: root, env: {}, scenes })).toThrow(/first \(first\.png\), second/)
    expect(readFileSync(join(root, 'docs', 'screenshots', 'first.png'), 'utf8')).toBe('committed')
  })

  it('refuses a template that lost its marker before it copies a single picture', () => {
    const root = fixtureRepo({ 'first.png': 'run one', 'second.png': 'run two' })
    writeFileSync(templatePath(root), '# Screenshots\n')

    expect(() => publish({ repoRoot: root, env: {}, scenes })).toThrow(/marker/)
    expect(readFileSync(join(root, 'docs', 'screenshots', 'first.png'), 'utf8')).toBe('committed')
  })
})
