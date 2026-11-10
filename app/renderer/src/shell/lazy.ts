/**
 * The one place a heavy syntax library enters the renderer.
 *
 * mermaid (core plus the diagram type a note draws) and KaTeX (plus its
 * stylesheet and faces) are features, so they ride the Vite graph as dynamic
 * imports: a note that opens without a fence or math never asks for either
 * chunk. Each import memoizes, and a rejected import clears the memo so the
 * next render retries instead of the note being stuck with the raw source.
 *
 * The theme helpers live here too because mermaid's initialization is the
 * loader's job: imported once, initialized once per theme, re-initialized when
 * the window flips day/night.
 */

const DAY = 'day'
const NIGHT = 'night'

export type MermaidTheme = typeof DAY | typeof NIGHT

/** What the render path needs from mermaid; the package's own default export. */
export type MermaidLibrary = (typeof import('mermaid'))['default']

/** What the render path needs from KaTeX; the package's own default export. */
export type MathLibrary = (typeof import('katex'))['default']

export type MermaidLoad = () => Promise<MermaidLibrary>
export type MathLoad = () => Promise<MathLibrary>

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

let mermaidLibrary: Promise<MermaidLibrary> | undefined
let configuredTheme: MermaidTheme | null = null

async function importMermaid(): Promise<MermaidLibrary> {
  return (await import('mermaid')).default
}

/**
 * The mermaid library, imported once and initialized once per theme. A rejected
 * import clears the memo so the next render retries; it never poisons the note.
 */
export async function loadMermaid(
  theme: MermaidTheme,
  load: MermaidLoad = importMermaid,
): Promise<MermaidLibrary> {
  mermaidLibrary ??= load().catch((error: unknown) => {
    mermaidLibrary = undefined
    throw error
  })

  const library = await mermaidLibrary
  if (configuredTheme !== theme) {
    library.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      theme: 'base',
      themeVariables: themeVariables(),
    })
    configuredTheme = theme
  }
  return library
}

let mathLibrary: Promise<MathLibrary> | undefined

async function importMath(): Promise<MathLibrary> {
  // The stylesheet is a static import of this chunk, so Vite emits it beside
  // the module and loads it before the module runs (UI-24's KaTeX CSS).
  return (await import('./katex')).default
}

/**
 * KaTeX and its stylesheet, imported once. A rejected import clears the memo so
 * the next render retries.
 */
export async function loadMath(load: MathLoad = importMath): Promise<MathLibrary> {
  mathLibrary ??= load().catch((error: unknown) => {
    mathLibrary = undefined
    throw error
  })
  return mathLibrary
}
