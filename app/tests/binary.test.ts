import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

import { resolveBinary } from '../src/main/binary'

const base = { packaged: false, resourcesPath: '/resources', appPath: '/repo/app' }

/** The host's own join: a Windows runner normalizes `/repo` with backslashes. */
const path = (...parts: string[]) => join(...parts)

describe('finding the tandem binary', () => {
  it('lets TANDEM_BIN win, so a checkout can name any build', () => {
    expect(
      resolveBinary({ ...base, platform: 'darwin', env: { TANDEM_BIN: '/opt/tandem' } }),
    ).toBe('/opt/tandem')
  })

  it('reads the bundled copy in a packaged app, with the platform suffix', () => {
    expect(resolveBinary({ ...base, packaged: true, platform: 'darwin', env: {} })).toBe(
      path('/resources', 'tandem'),
    )
    expect(resolveBinary({ ...base, packaged: true, platform: 'win32', env: {} })).toBe(
      path('/resources', 'tandem.exe'),
    )
  })

  it('reads the workspace build in development', () => {
    expect(resolveBinary({ ...base, platform: 'darwin', env: {} })).toBe(
      path('/repo/app', '..', 'rust', 'target', 'debug', 'tandem'),
    )
    expect(resolveBinary({ ...base, platform: 'win32', env: {} })).toBe(
      path('/repo/app', '..', 'rust', 'target', 'debug', 'tandem.exe'),
    )
  })
})
