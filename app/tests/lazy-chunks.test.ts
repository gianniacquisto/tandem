import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

import {
  entryKey,
  isKatexChunk,
  isMermaidChunk,
  lazyChunkFiles,
  readRendererManifest,
  staticClosure,
} from './lazy-manifest'

/**
 * The permanent guard UI-24's Q3 asks for: the built renderer keeps mermaid and
 * KaTeX off the entry, so a note without a fence or math pays none of their
 * startup bytes. It reads Vite's build manifest, so it sees the graph the app
 * actually ships, not the source imports.
 *
 * Run after `npm run build` (`npm run test:build`; app.yml's build gate). It is
 * excluded from `npm test` because a clean checkout has no build to read.
 */
const appRoot = process.cwd()
const manifest = readRendererManifest(appRoot)
const entry = entryKey(manifest)
const eager = staticClosure(manifest, entry)
const eagerKeys = new Set(eager.map(([key]) => key))
const chunks = Object.entries(manifest).filter(([, chunk]) => chunk.file.endsWith('.js'))
const lazy = chunks.filter(([, chunk]) => isMermaidChunk(chunk) || isKatexChunk(chunk))

describe('the entry chunk', () => {
  it('does not reach mermaid or KaTeX by a static import', () => {
    expect(eager.filter(([, chunk]) => isMermaidChunk(chunk))).toEqual([])
    expect(eager.filter(([, chunk]) => isKatexChunk(chunk))).toEqual([])
  })

  it('builds the page it serves', () => {
    const entryChunk = manifest[entry]
    expect(entryChunk?.isEntry).toBe(true)
    expect(entryChunk?.file).toMatch(/\.js$/)
  })
})

describe('the lazy chunks', () => {
  it('ships a mermaid chunk and a KaTeX chunk', () => {
    expect(chunks.filter(([, chunk]) => isMermaidChunk(chunk)).length).toBeGreaterThan(0)
    expect(chunks.filter(([, chunk]) => isKatexChunk(chunk)).length).toBeGreaterThan(0)
  })

  it('enters only through a dynamic import, never the eager closure', () => {
    const dynamic = new Set(
      Object.values(manifest).flatMap((chunk) => chunk.dynamicImports ?? []),
    )

    // Both roots are dynamic imports of the page: mermaid's core and the KaTeX
    // wrapper that carries its stylesheet.
    const mermaidRoot = chunks.find(([, chunk]) => /mermaid\.core/i.test(chunk.file))
    const katexRoot = chunks.find(([, chunk]) => /katex/i.test(chunk.file))
    expect(mermaidRoot).toBeDefined()
    expect(katexRoot).toBeDefined()
    expect(dynamic.has(mermaidRoot![0])).toBe(true)
    expect(dynamic.has(katexRoot![0])).toBe(true)

    for (const [key, chunk] of lazy) {
      expect(eagerKeys.has(key), `${chunk.file} sits in the eager closure`).toBe(false)
    }
  })

  it('is not preloaded or linked by the built page', () => {
    const html = readFileSync(join(appRoot, 'out', 'renderer', 'index.html'), 'utf8')
    for (const file of lazyChunkFiles(manifest)) {
      expect(html, `index.html names ${file}`).not.toContain(file)
    }
    expect(html).not.toMatch(/katex[^"']*\.css/i)
    expect(html).not.toMatch(/mermaid[^"']*\.js/i)
  })
})
