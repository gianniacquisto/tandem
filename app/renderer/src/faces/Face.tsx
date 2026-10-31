import type { ReactNode } from 'react'

export type FaceProps = {
  title: string
  blurb: string
  children?: ReactNode
}

/** The one face frame: a title, one line of purpose, and the face's body. */
export function Face({ title, blurb, children }: FaceProps) {
  return (
    <section className="face">
      <h1 className="face__title">{title}</h1>
      <p className="face__blurb">{blurb}</p>
      {children}
    </section>
  )
}
