import type { ReactNode } from 'react'

export type EmptyStateProps = {
  title: string
  subtitle: string
  /** The first-run state gets the room a quiet one does not. */
  hero?: boolean
  children?: ReactNode
}

/** The app's empty state: one line of title, one of direction, in the chrome's voice. */
export function EmptyState({ title, subtitle, hero = false, children }: EmptyStateProps) {
  return (
    <div className={hero ? 'empty empty--hero' : 'empty'}>
      <p className="empty__title">{title}</p>
      <p className="empty__subtitle">{subtitle}</p>
      {children}
    </div>
  )
}
