import { Fragment, useState, type ReactNode } from 'react'

import type { UsageReportView } from '../wire/UsageReportView'
import type { UsageRowView } from '../wire/UsageRowView'
import type { UsageSessionView } from '../wire/UsageSessionView'
import { Face } from './Face'
import {
  ABSENT,
  CACHE_PIECES,
  CHART_TITLES,
  DEFAULT_CACHE_SERIES,
  DEFAULT_TOKEN_SERIES,
  DEFAULT_WINDOW,
  DIMENSIONS,
  MODES,
  MODE_CONTROL_LABEL,
  WINDOWS,
  coverage,
  defaultSort,
  dimensionLabel,
  dimensionRows,
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
  TOKEN_PIECES,
  usageChart,
  windowLabel,
  type UsageChart,
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
  const [tokenSeries, setTokenSeries] = useState<UsageSeries[]>([...DEFAULT_TOKEN_SERIES])
  const [cacheSeries, setCacheSeries] = useState<UsageSeries[]>([...DEFAULT_CACHE_SERIES])
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
  // All time's hundreds of day columns bucket into Monday-started weeks; the
  // other windows draw one column per day row.
  const weekly = activeWindow === 'all-time'
  const tokenChart = usageChart(report.day, 'tokens', { series: tokenSeries, weekly })
  const cacheChart = usageChart(report.day, 'cache', { series: cacheSeries, weekly })
  const totalChart = usageChart(report.day, 'total', { weekly })
  const costChart = usageChart(report.day, 'cost', { weekly })
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

  function toggleTokenSeries(name: UsageSeries) {
    setTokenSeries((current) =>
      current.includes(name) ? current.filter((entry) => entry !== name) : [...current, name],
    )
  }

  function toggleCacheSeries(name: UsageSeries) {
    setCacheSeries((current) =>
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

        {/* The window picker stays even when the narrowed window holds nothing:
            it is the only way back to a window that does. */}
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

        {report.records === 0 ? (
          <div className="usage__empty">
            <p className="usage__empty-title">Nothing reported yet</p>
            <p className="usage__empty-subtitle">
              A session reports what it spent when it ends; its record lands here.
            </p>
          </div>
        ) : (
          <>
            {report.day.length === 0 ? null : (
              <div className="usage__graph">
                <div className="usage__charts">
                  <Chart
                    title={CHART_TITLES.tokens}
                    chart={tokenChart}
                    legend={
                      <SeriesPicker
                        label="Token series"
                        pieces={TOKEN_PIECES}
                        selected={tokenSeries}
                        onToggle={toggleTokenSeries}
                      />
                    }
                  />
                  <Chart
                    title={CHART_TITLES.cache}
                    chart={cacheChart}
                    legend={
                      <SeriesPicker
                        label="Cache series"
                        pieces={CACHE_PIECES}
                        selected={cacheSeries}
                        onToggle={toggleCacheSeries}
                      />
                    }
                  />
                  <Chart
                    title={CHART_TITLES.total}
                    chart={totalChart}
                    legend={<SingleLegend series="totalTokens" />}
                  />
                  <Chart
                    title={CHART_TITLES.cost}
                    chart={costChart}
                    legend={<SingleLegend series="cost" />}
                  />
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

/**
 * The stacked charts' legend and toggle in one: each piece is a button carrying
 * its swatch, so turning a piece off drops its slice and its swatch together.
 */
function SeriesPicker({
  label,
  pieces,
  selected,
  onToggle,
}: {
  label: string
  pieces: readonly UsageSeries[]
  selected: readonly UsageSeries[]
  onToggle: (series: UsageSeries) => void
}) {
  return (
    <div className="usage__legend-items" role="group" aria-label={label}>
      {pieces.map((candidate) => {
        const on = selected.includes(candidate)
        return (
          <button
            key={candidate}
            type="button"
            className="usage__series-option"
            aria-pressed={on}
            onClick={() => onToggle(candidate)}
          >
            {on ? (
              <span
                className={`usage__swatch usage__swatch--${candidate}`}
                aria-hidden="true"
              />
            ) : null}
            {seriesLabel(candidate)}
          </button>
        )
      })}
    </div>
  )
}

/** The single-series charts' legend: one plain swatch, never a toggle. */
function SingleLegend({ series }: { series: UsageSeries }) {
  return (
    <div className="usage__legend-items">
      <span className="usage__legend-item">
        <span className={`usage__swatch usage__swatch--${series}`} aria-hidden="true" />
        {seriesLabel(series)}
      </span>
    </div>
  )
}

/**
 * One small chart: its quiet heading, its gridlines at Half and Max with the
 * formatted tick, its columns oldest first, and its legend below. Bars stack in
 * a column per day (one bar per day for the single-series charts), each slice's
 * height already a share of the chart's own maximum.
 */
function Chart({
  title,
  chart,
  legend,
}: {
  title: string
  chart: UsageChart
  legend: ReactNode
}) {
  return (
    <section
      className="usage__chart"
      aria-label={`${title} per ${chart.weekly ? 'week' : 'day'}`}
    >
      <h3 className="usage__chart-title">{title}</h3>
      <div className="usage__chart-plot">
        <div className="usage__axis" aria-hidden="true">
          {chart.ticks.map((tick) => (
            <span
              key={tick.value}
              className="usage__tick"
              style={{ bottom: `${tick.heightPercent}%` }}
            >
              {tick.label}
            </span>
          ))}
        </div>
        <div className="usage__plot">
          <div className="usage__grid" aria-hidden="true">
            {chart.ticks.map((tick) => (
              <span
                key={tick.value}
                className="usage__gridline"
                style={{ bottom: `${tick.heightPercent}%` }}
              />
            ))}
          </div>
          <div className="usage__columns">
            {chart.columns.map((column) => (
              <div key={column.label} className="usage__bar-cell">
                <div className="usage__bar-track">
                  <div className="usage__bar-stack">
                    {column.bars.map((bar) => {
                      const reading = bar.series === 'cost' ? money(bar.value) : tokens(bar.value)
                      const name = `${column.label} ${seriesLabel(bar.series)}: ${reading}`
                      return (
                        <div
                          key={bar.series}
                          className={`usage__bar usage__bar--${bar.series}`}
                          role="img"
                          aria-label={name}
                          title={name}
                          style={{ height: `${bar.heightPercent}%` }}
                        />
                      )
                    })}
                  </div>
                </div>
                <span className="usage__bar-label" aria-hidden="true">
                  {column.label}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
      <div className="usage__legend">{legend}</div>
    </section>
  )
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
