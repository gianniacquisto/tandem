import { describe, expect, it } from 'vitest'

import { resolveBinary } from '../src/main/binary'

const base = { packaged: false, resourcesPath: '/resources', appPath: '/repo/app' }

describe('finding the tandem binary', () => {
  it('lets TANDEM_BIN win, so a checkout can name any build', () => {
    expect(
      resolveBinary({ ...base, platform: 'darwin', env: { TANDEM_BIN: '/opt/tandem' } }),
    ).toBe('/opt/tandem')
  })

  it('reads the bundled copy in a packaged app, with the platform suffix', () => {
    expect(
      resolveBinary({ ...base, packaged: true, platform: 'darwin', env: {} }),
    ).toBe('/resources/tandem')
    expect(
      resolveBinary({ ...base, packaged: true, platform: 'win32', env: {} }),
    ).toBe('/resources/tandem.exe')
  })

  it('reads the workspace build in development', () => {
    expect(resolveBinary({ ...base, platform: 'darwin', env: {} })).toBe(
      '/repo/rust/target/debug/tandem',
    )
    expect(resolveBinary({ ...base, platform: 'win32', env: {} })).toBe(
      '/repo/rust/target/debug/tandem.exe',
    )
  })
})
