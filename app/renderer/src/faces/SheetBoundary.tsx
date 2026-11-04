import { Component, type ReactNode } from 'react'

export type SheetBoundaryProps = {
  /** The sheet's subject: closing the error closes the sheet that threw. */
  onDismiss: () => void
  children?: ReactNode
}

type SheetBoundaryState = {
  error: Error | null
}

/**
 * The sheet's failure isolation (TAN-312): one failing ticket draws an inline
 * error with a dismiss, never a blank window. Without a boundary, any render
 * throw unmounts the React root, and the app goes blank with no error and no
 * way back but a restart.
 *
 * The boundary sits around the sheet only, so the board, the lanes and the
 * revision behind it stay up while the one ticket is down. The app remounts
 * the boundary per ticket (`key`), so opening another ticket starts clean.
 */
export class SheetBoundary extends Component<SheetBoundaryProps, SheetBoundaryState> {
  constructor(props: SheetBoundaryProps) {
    super(props)
    this.state = { error: null }
  }

  static getDerivedStateFromError(error: Error): SheetBoundaryState {
    return { error }
  }

  override render(): ReactNode {
    if (this.state.error !== null) {
      const detail =
        this.state.error.message === '' ? null : (
          <span className="sheet__error-detail">{this.state.error.message}</span>
        )
      return (
        <div className="sheet-backdrop">
          <div className="sheet" role="dialog" aria-modal="true" aria-label="Ticket">
            <header className="sheet__header">
              <span className="sheet__handle">Ticket</span>
              <button
                type="button"
                className="sheet__close"
                aria-label="Close the ticket"
                onClick={this.props.onDismiss}
              >
                ×
              </button>
            </header>
            <p className="sheet__error" role="alert">
              The ticket could not be drawn. {detail}
            </p>
            <div className="sheet__actions">
              <button type="button" className="sheet__button" onClick={this.props.onDismiss}>
                Dismiss
              </button>
            </div>
          </div>
        </div>
      )
    }
    return this.props.children
  }
}
