import katex from 'katex'

/**
 * One piece of TeX as HTML, for the Write surface.
 *
 * `throwOnError: false` is the bundle's setting: a parse failure draws the raw
 * TeX in place, inside KaTeX's own error span, rather than throwing into a
 * render pass. The catch is the same promise kept if a future KaTeX throws
 * before it can build that span. Source mode never calls this; it shows the
 * raw TeX untouched (UI-21).
 */
export function renderMath(tex: string, display: boolean): string {
  try {
    return katex.renderToString(tex, { displayMode: display, throwOnError: false })
  } catch {
    return escapeHtml(tex)
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
