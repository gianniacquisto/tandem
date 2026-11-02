import { Fragment, useState } from 'react'

import type { UsageReportView } from '../wire/UsageReportView'
import type { UsageRowView } from '../wire/UsageRowView'
import type { UsageSessionView } from '../wire/UsageSessionView'
import { Face } from './Face'
import {
  ABSENT,
  DIMENSIONS,
  MODES,
  MODE_CONTROL_LABEL,
  coverage,
  dayBars,
  dimensionLabel,
  dimensionRows,
  modeLabel,
  money,
  recordNoun,
  rowCells,
  selectedRow,
  startedAt,
  ticketArithmetic,
  ticketLabel,
  tokenLine,
  type UsageDimension,
  type UsageMode,
} from './usage'

export type UsageFaceProps = {
  /** The one report the face draws (`usage.report`), or null before the first read. */
  report: UsageReportView | null
  /** Opens a ticket's sheet; a ticket entry is inert until the sheet exists (PRT-17). */
  onOpenTicket?: (id: string) => void
}

/**
 * The Usage face: the spend the agents report on their records, read two ways,
 * rolled up into one table per dimension and rolled down into the records behind
 * a row.
 *
 * Every number is a field of the one report it is handed, formatted (`usage.ts`);
 * the face sums nothing and scales nothing: the one height it draws comes from
 * the chart helper beside the formatters. A record that reported no cost
 * draws the absent mark, never `$0.00`, which is why the header states its
 * coverage.
 */
export function UsageFace({ report, onOpenTicket }: UsageFaceProps) {
  const [dimension, setDimension] = useState<UsageDimension>('day')
  const [mode, setMode] = useState<UsageMode>('total')
  const [selected, setSelected] = useState<string | null>(null)

  if (report === null) {
    return (
      <Face wide title="Usage" blurb="The reported spend, by day, model, agent and ticket.">
        <p className="face__fact">Waiting for the store.</p>
      </Face>
    )
  }

  const arithmetic = ticketArithmetic(report)
  const bars = dayBars(report.day)
  const open = selectedRow(report, dimension, selected)

  function choose(next: UsageDimension) {
    // A row's id only means something inside its own table.
    setDimension(next)
    setSelected(null)
  }

  function select(row: UsageRowView) {
    setSelected(row.id === selected ? null : row.id)
  }

  return (
    <Face wide title="Usage" blurb="The reported spend, by day, model, agent and ticket.">
      <div className="usage">
        <div className="usage__header">
          <span className="usage__coverage">{coverage(report)}</span>
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
            {bars.length === 0 ? null : (
              <div className="usage__chart" role="group" aria-label="Cost per day">
                {bars.map(({ row, heightPercent }) => {
                  // The bar names what the Day table names: the absent mark for
                  // a day no record reported, never a zero dressed up.
                  const name =
                    row.reported === 0 ? `${row.label}: ${ABSENT}` : `${row.label}: ${money(row.cost)}`
                  return (
                    <div key={row.id} className="usage__bar-cell">
                      <div className="usage__bar-track">
                        <div
                          className="usage__bar"
                          role="img"
                          aria-label={name}
                          title={name}
                          style={{ height: `${heightPercent}%` }}
                        />
                      </div>
                      <span className="usage__bar-label" aria-hidden="true">
                        {row.label}
                      </span>
                    </div>
                  )
                })}
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
                  <th scope="col">{dimensionLabel(dimension).toUpperCase()}</th>
                  <th scope="col">Records</th>
                  <th scope="col">In</th>
                  <th scope="col">Out</th>
                  <th scope="col">Cache read</th>
                  <th scope="col">Cache write</th>
                  <th scope="col">Cost</th>
                  <th scope="col">
                    <span className="usage__sr-only">Share</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {dimensionRows(report, dimension).map((row) => {
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
                        <td className="usage__count">{cells.input}</td>
                        <td className="usage__count">{cells.output}</td>
                        <td className="usage__count">{cells.cacheRead}</td>
                        <td className="usage__count">{cells.cacheWrite}</td>
                        <td className="usage__cost">{cells.cost}</td>
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
