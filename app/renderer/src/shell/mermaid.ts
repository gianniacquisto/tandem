import mermaid from 'mermaid'

/**
 * The mermaid render path, moved from the retired editor bundle
 * (`Editor/src/mermaid.js`) with its shape intact.
 *
 * The fence is the whole interface: a ```mermaid code block draws its diagram.
 * Rendering is async and view-only: a failed parse leaves the raw source visible
 * under `.tandem-mermaid-error`, so the note never loses its bytes. The buffer
 * is never touched; reveal-to-edit shows the fence's source while the caret is
 * inside it and the diagram when it is not.
 *
 * mermaid runs with `securityLevel: "strict"`, and this module adds no
 * fetch/XHR path of its own. UI-24 owns splitting mermaid out of the renderer
 * chunk; here it is a static import.
 */

const DAY = 'day'
const NIGHT = 'night'

export type MermaidTheme = typeof DAY | typeof NIGHT

/**
 * The page's palette (the tokens in `tokens.css`), with the day values as the
 * fallbacks for a context that has no computed style (Node, the tests).
 */
export function palette(doc: Document | undefined = globalThis.document) {
  const styles =
    doc !== undefined && typeof globalThis.getComputedStyle === 'function'
      ? globalThis.getComputedStyle(doc.documentElement)
      : null
  const read = (name: string, fallback: string) => {
    const value = styles?.getPropertyValue?.(name)?.trim()
    return value ? value : fallback
  }
  return {
    paper: read('--paper', '#f1e9d7'),
    parchment: read('--parchment', '#e7deca'),
    sand: read('--sand', '#d6c9ae'),
    ink: read('--ink', '#211d18'),
    inkDim: read('--ink-dim', '#877d6b'),
    accent: read('--accent', '#6e7d6e'),
    accentDeep: read('--accent-deep', '#55604f'),
    ember: read('--ember', '#a9745c'),
  }
}

/**
 * The app's palette as mermaid's own theme variables, so a diagram is drawn in
 * the note's colours rather than mermaid's default theme. `theme: "base"` is
 * the empty theme these variables fill in.
 */
export function themeVariables(doc?: Document): Record<string, string> {
  const p = palette(doc)
  return {
    background: p.paper,
    primaryColor: p.parchment,
    primaryTextColor: p.ink,
    primaryBorderColor: p.sand,
    secondaryColor: p.sand,
    tertiaryColor: p.parchment,
    lineColor: p.inkDim,
    textColor: p.ink,
    mainBkg: p.parchment,
    nodeBorder: p.sand,
    clusterBkg: p.paper,
    clusterBorder: p.sand,
    edgeLabelBackground: p.paper,
    titleColor: p.ink,
    secondaryBorderColor: p.accent,
    tertiaryBorderColor: p.accentDeep,
    noteBkgColor: p.parchment,
    noteBorderColor: p.ember,
    noteTextColor: p.ink,
    errorBkgColor: p.parchment,
    errorTextColor: p.ember,
  }
}

/**
 * The theme the window is in. The renderer paints dark from `tokens.css`'
 * `prefers-color-scheme` rule, so the system scheme change is the flip.
 */
export function currentTheme(): MermaidTheme {
  if (typeof globalThis.matchMedia === 'function') {
    return globalThis.matchMedia('(prefers-color-scheme: dark)').matches ? NIGHT : DAY
  }
  return DAY
}

let configuredTheme: MermaidTheme | null = null

function configure(theme: MermaidTheme): void {
  if (configuredTheme === theme) return
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: 'strict',
    theme: 'base',
    themeVariables: themeVariables(),
  })
  configuredTheme = theme
}

let diagramId = 0

async function renderWithMermaid(source: string, theme: MermaidTheme): Promise<string> {
  configure(theme)
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
 * Draw `source` into `element`. Resolves to the SVG string, or null when the
 * render rejected (mermaid's parse error): the element then shows the raw
 * source as monospace ink under `.tandem-mermaid-error`. An injected `render`
 * is the test seam; `cache` is a Map keyed by theme + source.
 */
export async function renderDiagram(
  element: MermaidElement,
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
      element.classList.add('tandem-mermaid-error')
      element.textContent = source
      return null
    }
    cache.set(key, svg)
  }
  element.classList.remove('tandem-mermaid-error')
  element.innerHTML = svg
  return svg
}
