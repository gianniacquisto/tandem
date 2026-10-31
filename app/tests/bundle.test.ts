import { existsSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

/**
 * The editor bundle is built by `Editor/build.mjs` straight into
 * `app/renderer/public/editor`, so the copy the renderer loads is the one
 * source tree (`docs/port.md`, Where the old code goes). The build is
 * reproducible: `app.yml` runs it and fails on any diff in this directory, so a
 * hand edit cannot survive CI.
 *
 * This test is the shape check the renderer can do alone: the files an iframe
 * loads are there, and the bundle is a real build rather than a placeholder.
 */
const appRoot = fileURLToPath(new URL('..', import.meta.url))
const copy = join(appRoot, 'renderer', 'public', 'editor')

describe('the committed editor bundle', () => {
  it('carries every file the editor frame loads', () => {
    for (const file of ['index.html', 'editor.js', 'style.css', 'LICENSES.md']) {
      expect(existsSync(join(copy, file)), file).toBe(true)
    }
    expect(readdirSync(join(copy, 'fonts')).length).toBeGreaterThan(0)
  })

  it('carries the bundle itself, so the copy is not a placeholder', () => {
    expect(existsSync(join(copy, 'editor.js'))).toBe(true)
    expect(statSync(join(copy, 'editor.js')).size).toBeGreaterThan(500_000)
  })
})
