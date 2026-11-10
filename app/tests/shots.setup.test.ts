import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { hostname, tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import { CATALOGUE_MARKER, cataloguePath, templatePath } from '../shots/catalogue'
import globalSetup from '../shots/globalSetup'
import globalTeardown from '../shots/globalTeardown'
import { HOME_VAR } from '../shots/home'
import { OUT_VAR } from '../shots/output'
import { SCENES } from '../shots/scenes'

const scratch: string[] = []
const saved = { home: process.env[HOME_VAR], out: process.env[OUT_VAR] }

function scratchDir(prefix: string): string {
  const dir = mkdtempSync(join(tmpdir(), prefix))
  scratch.push(dir)
  return dir
}

/** A repository with the one fixture home a run is allowed to read. */
function fixtureRepo(): string {
  const repo = scratchDir('tandem-shots-setup-repo-')
  const home = join(repo, 'Fixtures', 'port', 'Home')
  mkdirSync(join(home, 'bikeshed'), { recursive: true })
  writeFileSync(join(home, 'README.md'), 'fixture\n')
  return repo
}

/** The same repo, plus every committed picture and the template a green teardown writes. */
function shotRepo(): string {
  const repo = fixtureRepo()
  const screenshots = join(repo, 'docs', 'screenshots')
  mkdirSync(screenshots, { recursive: true })
  writeFileSync(templatePath(repo), `# Screenshots\n\n${CATALOGUE_MARKER}\n`)
  for (const scene of SCENES) writeFileSync(join(screenshots, scene.file), 'png')
  return repo
}

function homeOf(repo: string): string {
  return join(repo, 'Fixtures', 'port', 'Home')
}

afterEach(() => {
  if (saved.home === undefined) delete process.env[HOME_VAR]
  else process.env[HOME_VAR] = saved.home
  if (saved.out === undefined) delete process.env[OUT_VAR]
  else process.env[OUT_VAR] = saved.out
  for (const dir of scratch.splice(0)) rmSync(dir, { recursive: true, force: true })
})

describe('the shots run hooks', () => {
  it('the run takes the lock before it seeds', async () => {
    const repo = fixtureRepo()
    const seed = scratchDir('tandem-shots-setup-seed-')
    process.env[HOME_VAR] = homeOf(repo)
    mkdirSync(join(seed, '.lock'), { recursive: true })
    writeFileSync(
      join(seed, '.lock', 'owner.json'),
      `${JSON.stringify({ pid: process.pid, host: hostname(), started: '2026-01-01T00:00:00.000Z', token: 'held' })}\n`,
    )

    await expect(globalSetup({ repoRoot: repo, seedRoot: seed, waitBoundMs: 60 })).rejects.toThrow(
      new RegExp(`pid ${process.pid}`),
    )

    expect(existsSync(join(seed, 'Home'))).toBe(false)
  })

  it('the run releases the lock on the scratch-look path', async () => {
    const repo = fixtureRepo()
    const seed = scratchDir('tandem-shots-setup-seed-')
    process.env[HOME_VAR] = homeOf(repo)

    await globalSetup({ repoRoot: repo, seedRoot: seed, waitBoundMs: 1000 })
    expect(existsSync(join(seed, '.lock'))).toBe(true)

    process.env[OUT_VAR] = scratchDir('tandem-shots-setup-look-')
    globalTeardown({ repoRoot: repo })

    expect(existsSync(join(seed, '.lock'))).toBe(false)
  })

  it('the run releases the lock on a green teardown', async () => {
    const repo = shotRepo()
    const seed = scratchDir('tandem-shots-setup-seed-')
    process.env[HOME_VAR] = homeOf(repo)
    delete process.env[OUT_VAR]

    await globalSetup({ repoRoot: repo, seedRoot: seed, waitBoundMs: 1000 })
    globalTeardown({ repoRoot: repo })

    expect(existsSync(join(seed, '.lock'))).toBe(false)
    expect(readFileSync(cataloguePath(repo), 'utf8')).toContain('`electron-shell.png`')
  })
})
