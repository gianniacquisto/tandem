import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import { committedDir, OUT_VAR, outputDirPath, screenshotPath, shotsOutputDir } from '../shots/output'

const scratch: string[] = []

function scratchDir(): string {
  const dir = mkdtempSync(join(tmpdir(), 'tandem-shots-out-'))
  scratch.push(dir)
  return dir
}

/** A repository checkout: the app, and the committed folder beside it. */
function fixtureRepo(): string {
  const root = mkdtempSync(join(tmpdir(), 'tandem-shots-repo-'))
  scratch.push(root)
  mkdirSync(join(root, 'docs', 'screenshots'), { recursive: true })
  return root
}

afterEach(() => {
  delete process.env[OUT_VAR]
  for (const dir of scratch.splice(0)) rmSync(dir, { recursive: true, force: true })
})

describe('the run output', () => {
  it('defaults to the app checkout scratch directory, never the committed folder', () => {
    const root = fixtureRepo()

    const dir = shotsOutputDir({}, root)

    expect(dir).toBe(resolve(root, 'app', 'shots-out'))
    expect(dir).not.toBe(committedDir(root))
    expect(existsSync(dir)).toBe(true)
    expect(screenshotPath({}, root, { id: 'shell', file: 'electron-shell.png', description: '' })).toBe(
      join(dir, 'electron-shell.png'),
    )
  })

  it('keeps one scratch directory for every run, so the two home commands share it', () => {
    const root = fixtureRepo()

    expect(outputDirPath({}, root)).toBe(outputDirPath({}, root))
    expect(outputDirPath({}, root)).toBe(resolve(root, 'app', 'shots-out'))
    expect(existsSync(outputDirPath({}, root))).toBe(false)
  })

  it('resolves a named override, creates it, and never falls back to the committed folder', () => {
    const root = fixtureRepo()
    const out = join(scratchDir(), 'look', 'one')
    const env = { [OUT_VAR]: out }

    const dir = shotsOutputDir(env, root)

    expect(dir).toBe(resolve(out))
    expect(dir).not.toBe(committedDir(root))
    expect(existsSync(dir)).toBe(true)
    expect(screenshotPath(env, root, { id: 'shell', file: 'electron-shell.png', description: '' })).toBe(
      join(resolve(out), 'electron-shell.png'),
    )
  })

  it('refuses a blank override instead of writing somewhere it did not name', () => {
    const root = fixtureRepo()

    expect(() => shotsOutputDir({ [OUT_VAR]: '   ' }, root)).toThrow(/TANDEM_SHOTS_OUT/)
    expect(() => outputDirPath({ [OUT_VAR]: '   ' }, root)).toThrow(/TANDEM_SHOTS_OUT/)
  })

  it('refuses an override that names a file, and writes nothing beside it', () => {
    const root = fixtureRepo()
    const file = join(scratchDir(), 'a-file.png')
    writeFileSync(file, 'png')

    expect(() => shotsOutputDir({ [OUT_VAR]: file }, root)).toThrow(/directory/)
    expect(existsSync(`${file}.staging`)).toBe(false)
  })
})
