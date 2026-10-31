import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

/**
 * The editor bundle moves verbatim (`docs/port.md`, Where the old code goes).
 *
 * The committed `Sources/Tandem/Resources/Editor` is the source of truth; the
 * renderer's copy under `public/editor` is what an iframe loads, and Vite copies
 * `public/` byte for byte, so the bundle is never rebundled. A file that differs
 * by one byte is the test that fails, because a rebundled editor is a different
 * editor.
 */
const appRoot = fileURLToPath(new URL('..', import.meta.url))
const repoRoot = join(appRoot, '..')
const source = join(repoRoot, 'Sources', 'Tandem', 'Resources', 'Editor')
const copy = join(appRoot, 'renderer', 'public', 'editor')

function filesUnder(directory: string, root: string = directory): string[] {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry)
    if (statSync(path).isDirectory()) return filesUnder(path, root)
    return [relative(root, path)]
  })
}

describe('the committed editor bundle', () => {
  it('is copied into the renderer with the same files', () => {
    expect(filesUnder(copy).sort()).toEqual(filesUnder(source).sort())
  })

  it('is byte-identical, file for file', () => {
    for (const file of filesUnder(source)) {
      const expected = readFileSync(join(source, file))
      const actual = readFileSync(join(copy, file))
      expect(actual.equals(expected), `${file} differs from the Swift resource`).toBe(true)
    }
  })

  it('carries the bundle itself, so the copy is not a placeholder', () => {
    expect(existsSync(join(copy, 'editor.js'))).toBe(true)
    expect(statSync(join(copy, 'editor.js')).size).toBeGreaterThan(500_000)
  })
})
