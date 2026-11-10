import { readFileSync } from 'node:fs'
import { join } from 'node:path'

/**
 * Vite's build manifest, read the way the lazy-chunk guard and the Playwright
 * request log both need it: one chunk per module, with its static `imports` and
 * its `dynamicImports` named by manifest key.
 */
export type ManifestChunk = {
  file: string
  src?: string
  name?: string
  isEntry?: boolean
  imports?: string[]
  dynamicImports?: string[]
  css?: string[]
}

export type RendererManifest = Record<string, ManifestChunk>

export function readRendererManifest(appRoot: string): RendererManifest {
  const path = join(appRoot, 'out', 'renderer', '.vite', 'manifest.json')
  return JSON.parse(readFileSync(path, 'utf8')) as RendererManifest
}

export function entryKey(manifest: RendererManifest): string {
  const entry = Object.entries(manifest).find(([, chunk]) => chunk.isEntry === true)
  if (entry === undefined) throw new Error('the renderer manifest names no entry chunk')
  return entry[0]
}

/** The entry's static import closure, the chunks a first visit loads eagerly. */
export function staticClosure(
  manifest: RendererManifest,
  key: string,
  seen = new Set<string>(),
): Array<[string, ManifestChunk]> {
  const chunk = manifest[key]
  if (chunk === undefined || seen.has(key)) return []
  seen.add(key)
  return [
    [key, chunk],
    ...(chunk.imports ?? []).flatMap((next) => staticClosure(manifest, next, seen)),
  ]
}

export function isMermaidChunk(chunk: ManifestChunk): boolean {
  return /mermaid/i.test(chunk.file) || /mermaid/i.test(chunk.src ?? '') || /mermaid/i.test(chunk.name ?? '')
}

export function isKatexChunk(chunk: ManifestChunk): boolean {
  return /katex/i.test(chunk.file) || /katex/i.test(chunk.src ?? '') || /katex/i.test(chunk.name ?? '')
}

export function isLazyChunk(chunk: ManifestChunk): boolean {
  return isMermaidChunk(chunk) || isKatexChunk(chunk)
}

/** The files a note without a fence or math must never ask for. */
export function lazyChunkFiles(manifest: RendererManifest): string[] {
  return Object.values(manifest)
    .filter(isLazyChunk)
    .map((chunk) => chunk.file)
}

/**
 * A string that only exists inside each library's own code, never in the
 * renderer that calls it. The manifest alone cannot see a library that is both
 * statically and dynamically imported, because Rollup inlines it and leaves no
 * chunk edge; these signatures catch that in the entry's bytes.
 */
export const KATEX_SIGNATURE = 'katex-error'
export const MERMAID_SIGNATURE = 'mermaidAPI'

/** The library a source appears to have inlined, or null when it inlines none. */
export function inlinedLibrary(source: string): 'mermaid' | 'KaTeX' | null {
  if (source.includes(KATEX_SIGNATURE)) return 'KaTeX'
  if (source.includes(MERMAID_SIGNATURE)) return 'mermaid'
  return null
}
