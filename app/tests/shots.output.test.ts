import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import { cataloguePath, CATALOGUE_MARKER } from '../shots/catalogue'
import globalTeardown from '../shots/globalTeardown'
import { OUT_VAR, isScratchOutput, screenshotPath, shotsOutputDir } from '../shots/output'
import { SCENES } from '../shots/scenes'

const scratch: string[] = []

function scratchDir(): string {
  const dir = mkdtempSync(join(tmpdir(), 'tandem-shots-out-'))
  scratch.push(dir)
  return dir
}

function fixtureRepo(files: string[]): string {
  const root = mkdtempSync(join(tmpdir(), 'tandem-shots-teardown-'))
  scratch.push(root)
  mkdirSync(join(root, 'docs', 'screenshots'), { recursive: true })
  writeFileSync(join(root, 'docs', 'screenshots', 'README.template.md'), `# Shots\n\n${CATALOGUE_MARKER}\n`)
  for (const file of files) writeFileSync(join(root, 'docs', 'screenshots', file), 'png')
  return root
}

afterEach(() => {
  delete process.env[OUT_VAR]
  for (const dir of scratch.splice(0)) rmSync(dir, { recursive: true, force: true })
})

describe('the scratch output', () => {
  it('defaults to the committed screenshots folder, and is not scratch', () => {
    expect(isScratchOutput({})).toBe(false)
    expect(shotsOutputDir({}, '/repo')).toBe(resolve('/repo', 'docs', 'screenshots'))
    expect(screenshotPath({}, '/repo', { id: 'shell', file: 'electron-shell.png', description: '' })).toBe(
      resolve('/repo', 'docs', 'screenshots', 'electron-shell.png'),
    )
  })

  it('resolves inside the named dir, creates it, and never defaults to the committed folder', () => {
    const base = scratchDir()
    const out = join(base, 'look', 'one')
    const env = { [OUT_VAR]: out }

    expect(isScratchOutput(env)).toBe(true)

    const dir = shotsOutputDir(env, '/repo')
    expect(dir).toBe(resolve(out))
    expect(dir).not.toBe(resolve('/repo', 'docs', 'screenshots'))
    expect(existsSync(dir)).toBe(true)

    expect(screenshotPath(env, '/repo', { id: 'shell', file: 'electron-shell.png', description: '' })).toBe(
      join(resolve(out), 'electron-shell.png'),
    )
  })

  it('refuses a blank override instead of writing into the committed folder silently', () => {
    expect(() => shotsOutputDir({ [OUT_VAR]: '   ' }, '/repo')).toThrow(/TANDEM_SHOTS_OUT/)
  })

  it('refuses an override that names a file, and writes nothing beside it', () => {
    const base = scratchDir()
    const file = join(base, 'a-file.png')
    writeFileSync(file, 'png')

    expect(() => shotsOutputDir({ [OUT_VAR]: file }, '/repo')).toThrow(/directory/)
    expect(existsSync(`${file}.staging`)).toBe(false)
  })

  it('teardown with the override set writes no catalogue', () => {
    const root = fixtureRepo(['first.png'])
    const env = { [OUT_VAR]: join(root, 'scratch') }
    const dir = shotsOutputDir(env, root)
    writeFileSync(join(dir, 'electron-shell.png'), 'png')

    process.env[OUT_VAR] = join(root, 'scratch')
    globalTeardown({ repoRoot: root })

    expect(existsSync(cataloguePath(root))).toBe(false)
    expect(existsSync(join(dir, 'electron-shell.png'))).toBe(true)
  })

  it('teardown without the override still refuses a catalogue over a missing picture', () => {
    const root = fixtureRepo(['first.png'])

    expect(() => globalTeardown({ repoRoot: root })).toThrow()
    expect(existsSync(cataloguePath(root))).toBe(false)
  })

  it('teardown without the override writes the catalogue when every picture exists', () => {
    const root = mkdtempSync(join(tmpdir(), 'tandem-shots-teardown-'))
    scratch.push(root)
    mkdirSync(join(root, 'docs', 'screenshots'), { recursive: true })
    writeFileSync(join(root, 'docs', 'screenshots', 'README.template.md'), `# Shots\n\n${CATALOGUE_MARKER}\n`)
    for (const entry of SCENES) writeFileSync(join(root, 'docs', 'screenshots', entry.file), 'png')

    globalTeardown({ repoRoot: root })

    const written = readFileSync(cataloguePath(root), 'utf8')
    for (const entry of SCENES) expect(written).toContain(entry.file)
  })
})
