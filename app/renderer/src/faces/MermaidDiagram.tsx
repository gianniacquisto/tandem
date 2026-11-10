import { useEffect, useRef, useState } from 'react'

import { currentTheme, renderDiagram, type MermaidTheme } from '../shell/mermaid'

/**
 * One mermaid diagram, drawn into a plain element.
 *
 * The render is async and view-only: `renderDiagram` owns the cache and the
 * error fallback. A system scheme change swaps the theme, and the effect's
 * theme + source key makes mermaid draw the mounted diagram again in the new
 * palette rather than keeping the old colours.
 */
export function MermaidDiagram({ source }: { source: string }) {
  const element = useRef<HTMLDivElement | null>(null)
  const [theme, setTheme] = useState<MermaidTheme>(() => currentTheme())

  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => setTheme(media.matches ? 'night' : 'day')
    media.addEventListener('change', onChange)
    return () => media.removeEventListener('change', onChange)
  }, [])

  useEffect(() => {
    const target = element.current
    if (target === null) return
    void renderDiagram(target, source, theme)
  }, [source, theme])

  return <div ref={element} className="block__mermaid" contentEditable={false} />
}
