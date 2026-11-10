import { loadMermaid, type MermaidTheme } from './lazy'

export { currentTheme, palette, themeVariables, type MermaidTheme } from './lazy'

/**
 * The mermaid render path, moved from the retired editor bundle
 * (`Editor/src/mermaid.js`) with its shape intact.
 *
 * The fence is the whole interface: a ```mermaid code block draws its diagram.
 * Rendering is async and view-only: a failed parse or a failed chunk leaves the
 * raw source visible under `.tandem-mermaid-error`, so the note never loses its
 * bytes. The buffer is never touched; reveal-to-edit shows the fence's source
 * while the caret is inside it and the diagram when it is not.
 *
 * mermaid runs with `securityLevel: "strict"`, and this module adds no
 * fetch/XHR path of its own. The library itself is a dynamic import behind
 * `loadMermaid`, so a note without a fence never loads it (UI-24).
 */

let diagramId = 0

async function renderWithMermaid(source: string, theme: MermaidTheme): Promise<string> {
  const mermaid = await loadMermaid(theme)
  const { svg } = await mermaid.render(`tandem-mermaid-${++diagramId}`, source)
  return svg
}

/** What the render path needs from an element, so a fake can stand in for one. */
export type MermaidElement = {
  classList: {
    add(name: string): void
    remove(name: string): void
  }
  textContent: string
  innerHTML: string
}

export type RenderOptions = {
  render?: (source: string, theme: MermaidTheme) => Promise<string>
  cache?: Map<string, string>
}

/** Source plus theme -> SVG, module-wide: a caret move or theme flip reuses it. */
export const diagramCache = new Map<string, string>()

/**
 * Draw `source` into an SVG string, or null when the render rejected (a parse
 * error or a rejected chunk): the caller keeps the raw source under
 * `.tandem-mermaid-error`. An injected `render` is the test seam; `cache` is a
 * Map keyed by theme + source. A rejection caches nothing, so the next render
 * retries (the loader has already cleared its own memo).
 */
export async function renderDiagramSvg(
  source: string,
  theme: MermaidTheme,
  { render = renderWithMermaid, cache = diagramCache }: RenderOptions = {},
): Promise<string | null> {
  const key = `${theme}\u0000${source}`
  let svg = cache.get(key)
  if (svg === undefined) {
    try {
      svg = await render(source, theme)
    } catch {
      return null
    }
    cache.set(key, svg)
  }
  return svg
}

/**
 * Draw `source` into `element`. Resolves to the SVG string, or null when the
 * render rejected: the element then shows the raw source as monospace ink under
 * `.tandem-mermaid-error`. The element-writing wrapper keeps the original test
 * seam (`mermaid.test.ts`); the component path uses `renderDiagramSvg` and
 * React state instead.
 */
export async function renderDiagram(
  element: MermaidElement,
  source: string,
  theme: MermaidTheme,
  options: RenderOptions = {},
): Promise<string | null> {
  const svg = await renderDiagramSvg(source, theme, options)
  if (svg === null) {
    element.classList.add('tandem-mermaid-error')
    element.textContent = source
    return null
  }
  element.classList.remove('tandem-mermaid-error')
  element.innerHTML = svg
  return svg
}
