export type FocusControlProps = {
  /** The declared projects, in render order. */
  projects: ReadonlyArray<{ name: string }>
  /** The stored focus, or null for every project. */
  active: string | null
  /** Whether the face draws every project without clearing the stored focus. */
  showAll: boolean
  /** Persist a new focus; null clears back to every project. */
  onPick: (project: string | null) => void
  /** Toggle the show-all escape. */
  onToggleShowAll: () => void
}

/**
 * The project focus control: a picker that persists the active project and a
 * show-all escape beside it. The picker is a native select, so it is keyboard
 * reachable without the face owning any keys; the escape is a button with its
 * pressed state, so the focused face can draw every project without clearing
 * the stored value. Both faces draw this one control, so the board and the
 * notes cannot disagree about what the focus is. It lists the board's projects,
 * which is what the stored focus means (UI-33).
 */
export function FocusControl({
  projects,
  active,
  showAll,
  onPick,
  onToggleShowAll,
}: FocusControlProps) {
  const value =
    active !== null && projects.some((project) => project.name === active) ? active : ''
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
        {projects.map((project) => (
          <option key={project.name} value={project.name}>
            {project.name}
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
