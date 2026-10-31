import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import { findPackagedApp } from '../scripts/packaged.mjs'
import { resolveBinary } from '../src/main/binary'

const cleanups: Array<() => void> = []

afterEach(() => {
  while (cleanups.length > 0) cleanups.pop()?.()
})

/** A `dist/` holding exactly the files named, each one empty. */
function fakeDist(files: string[]): string {
  const root = mkdtempSync(join(tmpdir(), 'tandem-dist-'))
  cleanups.push(() => rmSync(root, { recursive: true, force: true }))
  for (const file of files) {
    const path = join(root, file)
    mkdirSync(dirname(path), { recursive: true })
    writeFileSync(path, '')
  }
  return root
}

/** What `binary.ts` resolves beside a packaged app, from the found layout. */
function resolvedFor(dist: string, platform: NodeJS.Platform): string {
  const found = findPackagedApp(dist, platform)
  return resolveBinary({
    env: {},
    packaged: true,
    platform,
    appPath: found.root,
    resourcesPath: found.resources,
  })
}

describe('the packaged app', () => {
  it('finds the macOS bundle and the binary beside its resources', () => {
    const dist = fakeDist([
      'mac-arm64/Tandem.app/Contents/MacOS/Tandem',
      'mac-arm64/Tandem.app/Contents/Resources/tandem',
    ])
    const found = findPackagedApp(dist, 'darwin')

    expect(found.executable).toBe(join(dist, 'mac-arm64/Tandem.app/Contents/MacOS/Tandem'))
    expect(resolvedFor(dist, 'darwin')).toBe(
      join(dist, 'mac-arm64/Tandem.app/Contents/Resources/tandem'),
    )
  })

  it('finds the Linux unpacked app and the binary beside its resources', () => {
    const dist = fakeDist(['linux-unpacked/tandem-app', 'linux-unpacked/resources/tandem'])
    const found = findPackagedApp(dist, 'linux')

    expect(found.executable).toBe(join(dist, 'linux-unpacked/tandem-app'))
    expect(resolvedFor(dist, 'linux')).toBe(join(dist, 'linux-unpacked/resources/tandem'))
  })

  it('finds the Windows unpacked app and the suffixed binary', () => {
    const dist = fakeDist(['win-unpacked/Tandem.exe', 'win-unpacked/resources/tandem.exe'])
    const found = findPackagedApp(dist, 'win32')

    expect(found.executable).toBe(join(dist, 'win-unpacked/Tandem.exe'))
    expect(resolvedFor(dist, 'win32')).toBe(join(dist, 'win-unpacked/resources/tandem.exe'))
  })

  it('refuses a dist whose app carries no binary', () => {
    const dist = fakeDist(['linux-unpacked/tandem-app'])
    expect(() => findPackagedApp(dist, 'linux')).toThrow(/tandem/)
  })

  it('refuses a dist with no packaged app', () => {
    const dist = fakeDist([])
    expect(() => findPackagedApp(dist, 'darwin')).toThrow(/packaged app/)
  })
})
