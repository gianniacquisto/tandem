import { loadMath, type MathLibrary } from './lazy'

export type { MathLibrary } from './lazy'

/**
 * One piece of TeX as HTML, for the Write surface.
 *
 * `throwOnError: true` and `output: "html"` are the editor bundle's settings: a
 * parse failure throws, and the catch keeps the raw delimiters visible as
 * monospace ink under `.tandem-math-error`, exactly as a failed chunk does. The
 * library is a dynamic import behind `loadMath`, so a note without math never
 * loads KaTeX (UI-24).
 */
export type MathResult = {
  html: string
  /** True when KaTeX rejected the TeX and `html` is the raw delimiters. */
  error: boolean
}

export type MathLoad = () => Promise<MathLibrary>

export function renderMathWith(
  katex: MathLibrary,
  tex: string,
  display: boolean,
): MathResult {
  const delimiters = display ? '$$' : '$'
  try {
    return {
      html: katex.renderToString(tex, {
        displayMode: display,
        throwOnError: true,
        output: 'html',
      }),
      error: false,
    }
  } catch {
    return {
      html: escapeHtml(`${delimiters}${tex}${delimiters}`),
      error: true,
    }
  }
}

/**
 * Typeset `tex`, awaiting the KaTeX chunk. Resolves to null when the chunk
 * itself rejected: the note keeps its raw source and a later render retries
 * (the loader cleared its memo). A parse error resolves with `error: true` and
 * the raw delimiters in `html`.
 */
export async function renderMath(
  tex: string,
  display: boolean,
  load: MathLoad = loadMath,
): Promise<MathResult | null> {
  try {
    const katex = await load()
    return renderMathWith(katex, tex, display)
  } catch {
    return null
  }
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#x27;')
}
