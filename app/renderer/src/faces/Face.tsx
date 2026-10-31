import type { ReactNode } from 'react'

export type FaceProps = {
  title: string
  blurb: string
  /** A face whose table needs the window's width rather than a column's. */
  wide?: boolean
  children?: ReactNode
}

/** The one face frame: a title, one line of purpose, and the face's body. */
export function Face({ title, blurb, wide = false, children }: FaceProps) {
  return (
    <section className={wide ? 'face face--wide' : 'face'}>
      <h1 className="face__title">{title}</h1>
      <p className="face__blurb">{blurb}</p>
      {children}
    </section>
  )
}
