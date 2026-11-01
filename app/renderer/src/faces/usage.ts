import type { UsageReportView } from '../wire/UsageReportView'
import type { UsageRowView } from '../wire/UsageRowView'
import type { UsageSessionView } from '../wire/UsageSessionView'

/**
 * The report is the derivation (`rust/core/src/domain/usage.rs`) and every number
 * drawn here is one of its fields, formatted. The two quantities this file
 * derives are a row meter's width, which the report carries (`barPercent`), and
 * a day bar's height, which the report cannot carry: the chart reads the day
 * table the face is already handed, so the scale is the table's own max and a
 * height cannot drift from the rows beside it.
 */

/** The four row sets of the report, one table at a time. */
export type UsageDimension = 'day' | 'model' | 'agent' | 'ticket'

export const DIMENSIONS: readonly UsageDimension[] = ['day', 'model', 'agent', 'ticket']

const LABELS: Record<UsageDimension, string> = {
  day: 'Day',
  model: 'Model',
  agent: 'Agent',
  ticket: 'Ticket',
}

/** The ticketless row's key, the report's own (`NO_TICKET_ID`). */
export const NO_TICKET = 'no-ticket'

/** The mark a piece no record reported draws, never a zero dressed up. */
export const ABSENT = '\u2014'

export function dimensionLabel(dimension: UsageDimension): string {
  return LABELS[dimension]
}

/** The report's rows for one dimension, in the report's own order. */
export function dimensionRows(report: UsageReportView, dimension: UsageDimension): UsageRowView[] {
  switch (dimension) {
    case 'day':
      return report.day
    case 'model':
      return report.model
    case 'agent':
      return report.agent
    case 'ticket':
      return report.ticket
  }
}

/**
 * The row a reload still holds open, or nothing: a row's id only means something
 * inside its own table, and a revision can drop the row a human had selected.
 */
export function selectedRow(
  report: UsageReportView,
  dimension: UsageDimension,
  id: string | null,
): UsageRowView | null {
  if (id === null) return null
  return dimensionRows(report, dimension).find((row) => row.id === id) ?? null
}

/** The header's coverage: the records that reported usage, over the records read. */
export function coverage(report: UsageReportView): string {
  return `reported by ${report.reported} of ${report.records} records`
}

/**
 * The ticket table's arithmetic in one line: what its rows total, the overlap
 * that total carries, and the spend no ticket can claim. Every figure is a report
 * field, and no ticket rows means no line.
 */
export function ticketArithmetic(report: UsageReportView): string | null {
  if (report.ticket.length === 0) return null

  const parts = [`Ticket rows total ${money(report.ticketTotal)}`]
  if (report.overlap > 0) {
    parts.push(`${money(report.overlap)} of that is overlap: a shared record's cost per extra ticket`)
  }
  const loose = report.ticket.find((row) => row.id === NO_TICKET)
  if (loose !== undefined) {
    parts.push(
      loose.reported === 0
        ? `${recordNoun(loose.records)} carried none`
        : `${recordNoun(loose.records)} carried none, ${money(loose.cost)}`,
    )
  }
  return parts.join(' \u00b7 ')
}

/** A dollar figure with two decimals, the shape the usage comment carries. */
export function money(cost: number): string {
  return `$${cost.toFixed(2)}`
}

/** One column of the cost-per-day chart: its row and its height. */
export type DayBar = {
  row: UsageRowView
  /**
   * The bar's height as a percentage of the chart, scaled to the largest day
   * cost in the table being drawn, the same rule the row meters use.
   */
  heightPercent: number
}

/**
 * The day rows as chart columns, oldest first: the report's day rows arrive
 * newest first, so the chart reverses them. A day that reported nothing keeps
 * its slot with zero height, and a table whose days all cost nothing draws
 * zero heights rather than dividing by zero.
 */
export function dayBars(days: UsageRowView[]): DayBar[] {
  const oldestFirst = [...days].reverse()
  const largest = oldestFirst.reduce((top, row) => Math.max(top, row.cost), 0)
  return oldestFirst.map((row) => ({
    row,
    heightPercent: largest > 0 ? (row.cost / largest) * 100 : 0,
  }))
}

/** A token count with thousands separators; a piece no record reported draws the mark. */
export function tokens(count: number | null): string {
  if (count === null) return ABSENT
  return new Intl.NumberFormat('en-US').format(count)
}

/** A record count with its noun: "1 record", "8 records". */
export function recordNoun(count: number): string {
  return count === 1 ? '1 record' : `${count} records`
}

/**
 * A record's start in the reader's zone: month, day, hour and minute, never the
 * seconds. The zone is a parameter so a test does not follow the machine.
 */
export function startedAt(iso: string, timeZone?: string): string {
  const style: Intl.DateTimeFormatOptions = {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  }
  if (timeZone !== undefined) style.timeZone = timeZone
  return new Intl.DateTimeFormat('en-US', style).format(new Date(iso))
}

/** The tokens a record reported, each piece by its own word, or that it reported none. */
export function tokenLine(session: UsageSessionView): string {
  const reported = session.tokens
  if (reported === null) return 'no tokens reported'

  const parts: string[] = []
  if (reported.input !== null) parts.push(`${tokens(reported.input)} in`)
  if (reported.output !== null) parts.push(`${tokens(reported.output)} out`)
  if (reported.cacheRead !== null) parts.push(`${tokens(reported.cacheRead)} cache read`)
  if (reported.cacheWrite !== null) parts.push(`${tokens(reported.cacheWrite)} cache write`)
  if (reported.reasoning !== null) parts.push(`${tokens(reported.reasoning)} reasoning`)
  return parts.length === 0 ? 'no tokens reported' : parts.join(' \u00b7 ')
}

/**
 * What a carried ticket's entry reads: the ticket row's own label (the ref and
 * title), or the id alone when the board no longer holds it, so the link is
 * visible rather than silently dropped.
 */
export function ticketLabel(report: UsageReportView, id: string): string {
  return report.ticket.find((row) => row.id === id)?.label ?? id
}
