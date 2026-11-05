import { Fragment, useState } from 'react'

import type { UsageReportView } from '../wire/UsageReportView'
import type { UsageRowView } from '../wire/UsageRowView'
import type { UsageSessionView } from '../wire/UsageSessionView'
import { Face } from './Face'
import {
  ABSENT,
  DEFAULT_GRAPH_SERIES,
  DEFAULT_WINDOW,
  DIMENSIONS,
  GRAPH_SERIES,
  MODES,
  MODE_CONTROL_LABEL,
  WINDOWS,
  coverage,
  defaultSort,
  dimensionLabel,
  dimensionRows,
  graphBars,
  graphScales,
  modeLabel,
  money,
  recordNoun,
  rowCells,
  selectedRow,
  seriesLabel,
  sortRows,
  startedAt,
  ticketArithmetic,
  ticketLabel,
  tokenLine,
  tokens,
  toggleSort,
  windowLabel,
  type UsageDimension,
  type UsageMode,
  type UsageSeries,
  type UsageSort,
  type UsageSortColumn,
  type UsageWindow,
} from './usage'

export type UsageFaceProps = {
  /** The one report the face draws (`usage.report`), or null before the first read. */
  report: UsageReportView | null
  /** The rolling window the report was read through; the shell owns it. */
  window?: UsageWindow
  /** The window pick; the shell re-reads the report through it. */
  onWindowChange?: (window: UsageWindow) => void
  /** Opens a ticket's sheet; a ticket entry is inert until the sheet exists (PRT-17). */
  onOpenTicket?: (id: string) => void
}

/**
 * The Usage face: the spend the agents report on their records, read two ways,
 * rolled up into one table per dimension and rolled down into the records behind
 * a row.
 *
 * Every number is a field of the one report it is handed, formatted (`usage.ts`);
 * the face sums nothing and scales nothing: the heights it draws come from the
 * chart helpers beside the formatters. A record that reported no cost
 * draws the absent mark, never `$0.00`, which is why the header states its
 * coverage. The window narrows the report before it arrives, so header, tables
 * and graph never disagree; sort reorders for display and never rescales.
 */
export function UsageFace({ report, window, onWindowChange, onOpenTicket }: UsageFaceProps) {
  const [dimension, setDimension] = useState<UsageDimension>('day')
  const [mode, setMode] = useState<UsageMode>('total')
  const [selected, setSelected] = useState<string | null>(null)
  const [series, setSeries] = useState<UsageSeries[]>([...DEFAULT_GRAPH_SERIES])
  const [sorts, setSorts] = useState<Record<'model' | 'agent' | 'ticket', UsageSort>>(() => ({
    model: defaultSort('model') ?? { column: 'label', direction: 'asc' },
    agent: defaultSort('agent') ?? { column: 'label', direction: 'asc' },
    ticket: defaultSort('ticket') ?? { column: 'label', direction: 'asc' },
  }))
  const activeWindow: UsageWindow = window ?? DEFAULT_WINDOW

  if (report === null) {
    return (
      <Face wide label="Usage">
        <p className="face__fact">Waiting for the store.</p>
      </Face>
    )
  }

  const arithmetic = ticketArithmetic(report)
  const columns = graphBars(report.day, series)
  const scales = graphScales(report.day, series)
  const open = selectedRow(report, dimension, selected)
  const sort = dimension === 'day' ? null : sorts[dimension]
  const rows =
    dimension === 'day' || sort === null
      ? dimensionRows(report, dimension)
      : sortRows(dimensionRows(report, dimension), sort.column, sort.direction, mode)

  function choose(next: UsageDimension) {
    // A row's id only means something inside its own table.
    setDimension(next)
    setSelected(null)
  }

  function select(row: UsageRowView) {
    setSelected(row.id === selected ? null : row.id)
  }

  function toggleSeries(name: UsageSeries) {
    setSeries((current) =>
      current.includes(name) ? current.filter((entry) => entry !== name) : [...current, name],
    )
  }

  function pickSort(column: UsageSortColumn) {
    if (dimension === 'day' || sort === null) return
    setSorts((current) => ({ ...current, [dimension]: toggleSort(current[dimension], column) }))
  }

  return (
    <Face wide label="Usage">
      <div className="usage">
        <div className="usage__header">
          <span className="usage__coverage">{coverage(report, activeWindow)}</span>
          <span className="usage__spend">{money(report.cost)}</span>
        </div>
        {arithmetic === null ? null : <p className="usage__arithmetic">{arithmetic}</p>}

        {report.records === 0 ? (
          <div className="usage__empty">
            <p className="usage__empty-title">Nothing reported yet</p>
            <p className="usage__empty-subtitle">
              A session reports what it spent when it ends; its record lands here.
            </p>
          </div>
        ) : (
          <>
            <div className="usage__window" role="tablist" aria-label="Usage window">
              {WINDOWS.map((candidate) => (
                <button
                  key={candidate}
                  type="button"
                  role="tab"
                  className="usage__window-segment"
                  aria-selected={candidate === activeWindow}
                  onClick={() => onWindowChange?.(candidate)}
                >
                  {windowLabel(candidate)}
                </button>
              ))}
            </div>
            {columns.length === 0 ? null : (
              <div className="usage__graph">
                <div className="usage__chart" role="group" aria-label="Usage per day">
                  {columns.map(({ row, values, heights }) => (
                    <div key={row.id} className="usage__bar-cell">
                      <div
                        className={`usage__bar-track${series.length > 1 ? ' usage__bar-track--multi' : ''}`}
                      >
                        {series.map((name) => {
                          const value = values[name] ?? null
                          // Absent stays absent: no bar, never a zero dressed up.
                          // A reported zero keeps its bar at zero height.
                          if (value === null) return null
                          const height = heights[name] ?? 0
                          const reading =
                            name === 'cost'
                              ? money(value)
                              : name === 'records'
                                ? tokens(value)
                                : tokens(value)
                          const name_ = `${row.label} ${seriesLabel(name)}: ${reading}`
                          return (
                            <div
                              key={name}
                              className={`usage__bar usage__bar--${name}`}
                              role="img"
                              aria-label={name_}
                              title={name_}
                              style={{ height: `${height}%` }}
                            />
                          )
                        })}
                      </div>
                      <span className="usage__bar-label" aria-hidden="true">
                        {row.label}
                      </span>
                    </div>
                  ))}
                </div>
                <p className="usage__scales">
                  <GraphScales scales={scales} series={series} />
                </p>
                <div className="usage__series" role="group" aria-label="Graph series">
                  {GRAPH_SERIES.map((candidate) => (
                    <button
                      key={candidate}
                      type="button"
                      className="usage__series-option"
                      aria-pressed={series.includes(candidate)}
                      onClick={() => toggleSeries(candidate)}
                    >
                      {seriesLabel(candidate)}
                    </button>
                  ))}
                </div>
              </div>
            )}
            <div className="usage__controls">
              <div className="usage__picker" role="tablist" aria-label="Usage dimensions">
                {DIMENSIONS.map((candidate) => (
                  <button
                    key={candidate}
                    type="button"
                    role="tab"
                    className="usage__segment"
                    aria-selected={candidate === dimension}
                    onClick={() => choose(candidate)}
                  >
                    {dimensionLabel(candidate)}
                  </button>
                ))}
              </div>
              <div className="usage__mode" role="tablist" aria-label={MODE_CONTROL_LABEL}>
                {MODES.map((candidate) => (
                  <button
                    key={candidate}
                    type="button"
                    role="tab"
                    className="usage__mode-segment"
                    aria-selected={candidate === mode}
                    onClick={() => setMode(candidate)}
                  >
                    {modeLabel(candidate)}
                  </button>
                ))}
              </div>
            </div>

            <table className="usage__table">
              <thead>
                <tr>
                  <SortHeader
                    dimension={dimension}
                    sort={sort}
                    column="label"
                    label={dimensionLabel(dimension).toUpperCase()}
                    onPick={pickSort}
                  />
                  <SortHeader dimension={dimension} sort={sort} column="records" label="Records" onPick={pickSort} />
                  <SortHeader dimension={dimension} sort={sort} column="input" label="In" onPick={pickSort} />
                  <SortHeader dimension={dimension} sort={sort} column="output" label="Out" onPick={pickSort} />
                  <SortHeader dimension={dimension} sort={sort} column="cacheRead" label="Cache read" onPick={pickSort} />
                  <SortHeader
                    dimension={dimension}
                    sort={sort}
                    column="cacheWrite"
                    label="Cache write"
                    onPick={pickSort}
                  />
                  <SortHeader dimension={dimension} sort={sort} column="cost" label="Cost" onPick={pickSort} />
                  <th scope="col">
                    <span className="usage__sr-only">Share</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => {
                  // Every numeric cell is the report's own in the active mode:
                  // totals, or per-reported-record averages.
                  const cells = rowCells(row, mode)
                  return (
                    <Fragment key={row.id}>
                      <tr className="usage__row" aria-selected={row.id === selected}>
                        <th scope="row">
                          <button
                            type="button"
                            className="usage__label"
                            aria-expanded={row.id === selected}
                            onClick={() => select(row)}
                            title={`Show the ${recordNoun(row.records)} behind this row`}
                          >
                            {row.label}
                          </button>
                        </th>
                        <td className="usage__count">{row.records}</td>
                        <td className={`usage__count${cells.input === ABSENT ? ' usage__absent' : ''}`}>
                          {cells.input}
                        </td>
                        <td className={`usage__count${cells.output === ABSENT ? ' usage__absent' : ''}`}>
                          {cells.output}
                        </td>
                        <td className={`usage__count${cells.cacheRead === ABSENT ? ' usage__absent' : ''}`}>
                          {cells.cacheRead}
                        </td>
                        <td className={`usage__count${cells.cacheWrite === ABSENT ? ' usage__absent' : ''}`}>
                          {cells.cacheWrite}
                        </td>
                        <td className={`usage__cost${cells.cost === ABSENT ? ' usage__absent' : ''}`}>
                          {cells.cost}
                        </td>
                        <td className="usage__share">
                          {/* The bar is a meter over the report's own percentage: no
                              inline width, no percentage derived here. */}
                          <meter
                            className="usage__meter"
                            min={0}
                            max={100}
                            value={row.barPercent}
                            aria-label={`${row.label}: ${recordNoun(row.reported)} reported`}
                          />
                        </td>
                      </tr>
                      {row.id === selected && open !== null ? (
                        <tr className="usage__roll">
                          <td colSpan={8}>
                            {open.sessions.map((session) => (
                              <Record
                                key={session.id}
                                session={session}
                                ticketName={(id) => ticketLabel(report, id)}
                                onOpenTicket={onOpenTicket}
                              />
                            ))}
                          </td>
                        </tr>
                      ) : null}
                    </Fragment>
                  )
                })}
              </tbody>
            </table>
          </>
        )}
      </div>
    </Face>
  )
}

function GraphScales({
  scales,
  series,
}: {
  scales: { tokenMax: number; recordsMax: number; costMax: number }
  series: UsageSeries[]
}) {
  const parts: string[] = []
  if (series.some((name) => name !== 'records' && name !== 'cost')) {
    parts.push(`Tokens per day, max ${tokens(scales.tokenMax)}`)
  }
  if (series.includes('records')) {
    parts.push(`Records per day, max ${tokens(scales.recordsMax)}`)
  }
  if (series.includes('cost')) {
    parts.push(`Cost per day, max ${money(scales.costMax)}`)
  }
  if (parts.length === 0) return <>Select a series to draw the graph.</>
  return <>{parts.join(' · ')}</>
}

function SortHeader({
  dimension,
  sort,
  column,
  label,
  onPick,
}: {
  dimension: UsageDimension
  sort: UsageSort | null
  column: UsageSortColumn
  label: string
  onPick: (column: UsageSortColumn) => void
}) {
  // Day stays newest first and is not sortable, since it feeds the chart.
  if (dimension === 'day' || sort === null) {
    return <th scope="col">{label}</th>
  }
  const active = sort.column === column
  const arrow = active ? (sort.direction === 'asc' ? ' ↑' : ' ↓') : ''
  return (
    <th scope="col" aria-sort={active ? (sort.direction === 'asc' ? 'ascending' : 'descending') : 'none'}>
      <button
        type="button"
        className="usage__sort"
        aria-label={`Sort by ${label}${active ? `, ${sort.direction}ending` : ''}`}
        onClick={() => onPick(column)}
      >
        {`${label}${arrow}`}
      </button>
    </th>
  )
}

type RecordProps = {
  session: UsageSessionView
  ticketName: (id: string) => string
  onOpenTicket?: (id: string) => void
}

/**
 * One record behind a row: its session id, agent, model, local start, cost, the
 * tokens it reported and the tickets it carried, each opening its sheet.
 */
function Record({ session, ticketName, onOpenTicket }: RecordProps) {
  return (
    <div className="usage__record">
      <div className="usage__record-line">
        <code className="usage__session">{session.id}</code>
        <span>{session.agent ?? 'unattributed'}</span>
        <span className="usage__model">{session.model ?? 'unreported'}</span>
        <span className="usage__started">{startedAt(session.startedAt)}</span>
        <span className="usage__cost">
          {session.cost === null ? ABSENT : money(session.cost)}
        </span>
      </div>
      <div className="usage__record-line usage__record-tokens">
        <span>{tokenLine(session)}</span>
        <span className="usage__tickets">
          {session.tickets.map((id) => (
            <button
              key={id}
              type="button"
              className="usage__ticket"
              onClick={() => onOpenTicket?.(id)}
              title={`Open ${ticketName(id)}`}
            >
              {ticketName(id)}
            </button>
          ))}
        </span>
      </div>
    </div>
  )
}
