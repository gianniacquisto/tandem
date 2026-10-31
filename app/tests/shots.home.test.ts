import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import { HOME_VAR, seedHome, seedPath, seedUserPath, shotsSource } from '../shots/home'

const repoRoot = '/repo'

const scratch: string[] = []

function scratchDir(): string {
  const dir = mkdtempSync(join(tmpdir(), 'tandem-shots-'))
  scratch.push(dir)
  return dir
}

afterEach(() => {
  for (const dir of scratch.splice(0)) rmSync(dir, { recursive: true, force: true })
})

describe('the shots home', () => {
  it('requires the home: unset or blank fails the run', () => {
    expect(() => shotsSource({}, repoRoot)).toThrow(/TANDEM_SHOTS_HOME is not set/)
    expect(() => shotsSource({ [HOME_VAR]: '   ' }, repoRoot)).toThrow(/TANDEM_SHOTS_HOME is not set/)
  })

  it('refuses a real home, a sibling prefix and a home outside the repository', () => {
    expect(() => shotsSource({ [HOME_VAR]: '/Users/someone/.tandem' }, repoRoot)).toThrow(/Refused/)
    expect(() => shotsSource({ [HOME_VAR]: '/repo/Fixtures-other/Home' }, repoRoot)).toThrow(/Refused/)
    expect(() => shotsSource({ [HOME_VAR]: '/Users/someone/tandem-home' }, repoRoot)).toThrow(/Refused/)
  })

  it('accepts the port fixture home and resolves it', () => {
    const port = join(repoRoot, 'Fixtures', 'port', 'Home')

    expect(shotsSource({ [HOME_VAR]: port }, repoRoot)).toBe(resolve(port))
  })

  it('seeds at one fixed scratch root, so a picture carries the same paths in any checkout', () => {
    if (process.platform === 'win32') return

    expect(seedPath()).toBe('/tmp/tandem-shots/Home')
    expect(seedUserPath()).toBe('/tmp/tandem-shots/user')
  })

  it('seeds a scratch copy so no run writes into the committed home', () => {
    const dir = scratchDir()
    const source = join(dir, 'source')
    mkdirSync(join(source, 'bikeshed'), { recursive: true })
    writeFileSync(join(source, 'README.md'), 'fixture\n')
    const target = join(dir, 'seed', 'Home')

    seedHome(source, target)

    expect(readFileSync(join(target, 'README.md'), 'utf8')).toBe('fixture\n')
    writeFileSync(join(target, 'scratch.db'), 'x')
    expect(existsSync(join(source, 'scratch.db'))).toBe(false)

    seedHome(source, target)

    expect(existsSync(join(target, 'scratch.db'))).toBe(false)
    expect(existsSync(`${target}.staging`)).toBe(false)
  })
})
