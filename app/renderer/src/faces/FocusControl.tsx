export type FocusControlProps = {
  /** The declared lanes, in render order. */
  lanes: readonly string[]
  /** The stored focus, or null for every lane. */
  active: string | null
  /** Whether the face draws every lane without clearing the stored focus. */
  showAll: boolean
  /** Persist a new focus; null clears back to every lane. */
  onPick: (lane: string | null) => void
  /** Toggle the show-all escape. */
  onToggleShowAll: () => void
}

/**
 * The project focus control: a picker that persists the active project and a
 * show-all escape beside it. The picker is a native select, so it is keyboard
 * reachable without the face owning any keys; the escape is a button with its
 * pressed state, so the focused face can draw every lane without clearing the
 * stored value. Both faces draw this one control, so the board and the notes
 * cannot disagree about what the focus is.
 */
export function FocusControl({ lanes, active, showAll, onPick, onToggleShowAll }: FocusControlProps) {
  const value = active !== null && lanes.includes(active) ? active : ''
  return (
    <div className="focus">
      <label className="focus__label" htmlFor="focus-picker">
        Project
      </label>
      <select
        id="focus-picker"
        className="filter focus__picker"
        aria-label="Project"
        value={value}
        onChange={(event) =>
          onPick(event.target.value === '' ? null : event.target.value)
        }
      >
        <option value="">All projects</option>
        {lanes.map((lane) => (
          <option key={lane} value={lane}>
            {lane}
          </option>
        ))}
      </select>
      {active === null ? null : (
        <button
          type="button"
          className="focus__all"
          aria-pressed={showAll}
          onClick={onToggleShowAll}
        >
          {showAll ? 'Show focus' : 'Show all'}
        </button>
      )}
    </div>
  )
}
