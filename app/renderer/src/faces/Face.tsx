import type { ReactNode } from 'react'

export type FaceProps = {
  /** The tab the face draws under, for assistive tech; the tab strip is the title. */
  label: string
  /** At most one short purpose line, where it earns its space. */
  blurb?: string
  /** A face whose body takes the window's width rather than a column's. */
  wide?: boolean
  children?: ReactNode
}

/** The one face frame: a labelled section, an optional purpose line, the body. */
export function Face({ label, blurb, wide = false, children }: FaceProps) {
  return (
    <section className={wide ? 'face face--wide' : 'face'} aria-label={label}>
      {blurb === undefined ? null : <p className="face__blurb">{blurb}</p>}
      {children}
    </section>
  )
}
