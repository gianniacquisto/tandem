import {
  EDITOR_FONT_SIZE_MAX,
  EDITOR_FONT_SIZE_MIN,
} from '../shell/fontSize'

/**
 * ADR 0012's `A− size A+`, restored as header chrome for the block editor.
 *
 * The face owns the live size and persists the step, so this control is only
 * the three cells: the two bounds disable their own button, and the readout
 * announces a step without moving the group under the pointer.
 */
export type NoteSizeControlProps = {
  /** The live app-wide note size, in pixels. */
  size: number
  /** One step: -1 for A−, +1 for A+. */
  onStep: (delta: number) => void
}

export function NoteSizeControl({ size, onStep }: NoteSizeControlProps) {
  return (
    <div className="editor__size" role="group" aria-label="Note size">
      <button
        type="button"
        className="editor__size-button"
        aria-label="Decrease note font size"
        disabled={size <= EDITOR_FONT_SIZE_MIN}
        onClick={() => onStep(-1)}
      >
        A{'\u2212'}
      </button>
      <span className="editor__size-readout" aria-live="polite">
        {size}
      </span>
      <button
        type="button"
        className="editor__size-button"
        aria-label="Increase note font size"
        disabled={size >= EDITOR_FONT_SIZE_MAX}
        onClick={() => onStep(1)}
      >
        A+
      </button>
    </div>
  )
}
