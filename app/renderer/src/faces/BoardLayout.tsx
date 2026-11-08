import type { LaneRow, StatusChoice } from '../shell/board'
import type { WorkflowStage } from '../wire/WorkflowStage'
import type { TicketSummary } from '../wire/TicketSummary'
import { BoardCell } from './BoardCell'

export type BoardProject = {
  name: string
  lanes: string[]
}

export type BoardLayoutProps = {
  rows: LaneRow[]
  /** The active workflow's stages, in order: the columns. */
  statuses: ReadonlyArray<StatusChoice>
  /** The same stages, so a card's custom label draws. */
  stages?: readonly WorkflowStage[] | null
  /** Every project with its lanes, in render order: the board's grouping. */
  projects?: BoardProject[]
  selected: string | null
  onSelect: (id: string) => void
  onToggle: (ticket: TicketSummary) => void
  onOpen?: (ticket: TicketSummary) => void
  onDropOn: (draggedId: string, anchorId: string, after: boolean) => void
  onDropCell: (draggedId: string, lane: string | null, status: string) => void
}

/**
 * The board grid: lanes are rows and the six statuses are columns, reading
 * left to right in the declared workflow order, so every lane runs Backlog to
 * Done the same way the status menu and the filters do.
 *
 * Lanes group under project headers: projects in list order, their lanes
 * beneath in lane order, the default area first and ungrouped, empty projects
 * not drawn. Headers are a quiet group label, smaller than the lane title with
 * no full-bleed band; lane heads are unchanged.
 *
 * A drop on a card reorders inside its cell; a drop on the cell itself files
 * the ticket there. The cell's own state change goes through `ticket.set` and
 * its lane change through `ticket.move`, both verbs the list already uses, so
 * the two layouts cannot drift apart.
 */
export function BoardLayout({
  rows,
  statuses,
  stages,
  projects = [],
  selected,
  onSelect,
  onToggle,
  onOpen,
  onDropOn,
  onDropCell,
}: BoardLayoutProps) {
  const groups = groupRows(rows, projects)
  return (
    <div className="board">
      <div className="board__lane board__lane--head">
        <span className="board__title board__title--head" aria-hidden="true" />
        {statuses.map((status) => (
          <h2 key={status.value} className="board__status" data-status={status.value}>
            {status.label}
          </h2>
        ))}
      </div>
      {groups.map((group) => (
        <GroupRows
          key={group.project ?? 'uncategorized'}
          group={group}
          stages={stages}
          selected={selected}
          onSelect={onSelect}
          onToggle={onToggle}
          onOpen={onOpen}
          onDropOn={onDropOn}
          onDropCell={onDropCell}
        />
      ))}
    </div>
  )
}

type RowGroup = {
  /** Null is the default area: first on the board, ungrouped, no header. */
  project: string | null
  rows: LaneRow[]
}

function groupRows(rows: LaneRow[], projects: BoardProject[]): RowGroup[] {
  if (projects.length === 0) {
    return rows.length === 0 ? [] : [{ project: null, rows }]
  }
  const byLane = new Map<string | null, LaneRow>()
  for (const row of rows) {
    byLane.set(row.key, row)
  }
  const groups: RowGroup[] = []
  const grouped = new Set<string | null>()
  // The default area first, ungrouped, no header.
  const uncategorized = rows.filter((row) => row.key === null)
  if (uncategorized.length > 0) {
    groups.push({ project: null, rows: uncategorized })
    grouped.add(null)
  }
  for (const project of projects) {
    const lanes = project.lanes
      .map((name) => byLane.get(name))
      .filter((row): row is LaneRow => row !== undefined)
    if (lanes.length === 0) continue
    for (const row of lanes) grouped.add(row.key)
    groups.push({ project: project.name, rows: lanes })
  }
  // Lanes no project names (a hand edit, or a stale listing): after the
  // grouped rows, ungrouped, so no ticket is hidden.
  const leftover = rows.filter((row) => !grouped.has(row.key))
  if (leftover.length > 0) {
    groups.push({ project: null, rows: leftover })
  }
  return groups
}

function GroupRows({
  group,
  stages,
  selected,
  onSelect,
  onToggle,
  onOpen,
  onDropOn,
  onDropCell,
}: {
  group: RowGroup
} & Pick<
  BoardLayoutProps,
  'stages' | 'selected' | 'onSelect' | 'onToggle' | 'onOpen' | 'onDropOn' | 'onDropCell'
>) {
  const count = group.rows.reduce(
    (total, row) => total + row.cells.reduce((sum, cell) => sum + cell.tickets.length, 0),
    0,
  )
  return (
    <>
      {group.project === null ? null : (
        <div className="board__project" data-project={group.project}>
          <h2 className="board__project-title">
            <span>{group.project}</span>
            <span className="board__project-count">{count}</span>
          </h2>
        </div>
      )}
      {group.rows.map((row) => (
        <div
          className="board__lane"
          key={row.key ?? 'uncategorized'}
          data-lane={row.key ?? ''}
        >
          <h2 className="board__title">
            <span>{row.title}</span>
            <span className="board__count">
              {row.cells.reduce((count, cell) => count + cell.tickets.length, 0)}
            </span>
          </h2>
          {row.cells.map((cell) => (
            <BoardCell
              key={cell.status}
              lane={row}
              cell={cell}
              stages={stages}
              selected={selected}
              onSelect={onSelect}
              onToggle={onToggle}
              onOpen={onOpen}
              onDropOn={onDropOn}
              onDropCell={onDropCell}
            />
          ))}
        </div>
      ))}
    </>
  )
}
