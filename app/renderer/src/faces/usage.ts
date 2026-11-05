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

/** The Usage page's rolling window, the wire's own (`UsageWindow`). */
export type UsageWindow = 'last-7-days' | 'last-30-days' | 'this-month' | 'all-time'

export const WINDOWS: readonly UsageWindow[] = ['last-7-days', 'last-30-days', 'this-month', 'all-time']

/** The window the page opens on: the last seven days, per visit. */
export const DEFAULT_WINDOW: UsageWindow = 'last-7-days'

const WINDOW_LABELS: Record<UsageWindow, string> = {
  'last-7-days': 'Last 7 days',
  'last-30-days': 'Last 30 days',
  'this-month': 'This month',
  'all-time': 'All time',
}

export function windowLabel(window: UsageWindow): string {
  return WINDOW_LABELS[window]
}

/** The ticketless row's key, the report's own (`NO_TICKET_ID`). */
export const NO_TICKET = 'no-ticket'

/** The table's two readings: summed totals, or per-reported-record averages. */
export type UsageMode = 'total' | 'average'

export const MODES: readonly UsageMode[] = ['total', 'average']

const MODE_LABELS: Record<UsageMode, string> = {
  total: 'Total',
  average: 'Average',
}

export function modeLabel(mode: UsageMode): string {
  return MODE_LABELS[mode]
}

/** The toggle's accessible name: what the two readings share, in the glossary's words. */
export const MODE_CONTROL_LABEL = 'Totals or per-record averages'

/** One row's numeric cells, formatted for the table in the active mode. */
export type UsageCells = {
  input: string
  output: string
  cacheRead: string
  cacheWrite: string
  cost: string
}

/**
 * The row's cells in one mode: its totals in Total mode, byte-identical to
 * today, and its per-reported-record averages in Average mode. A row that
 * reported nothing draws the absent mark in every numeric cell either way,
 * and the Records column never averages, so it is not one of these cells.
 */
export function rowCells(row: UsageRowView, mode: UsageMode): UsageCells {
  if (mode === 'average' && row.reported > 0) {
    return {
      input: tokens(row.averageTokens.input),
      output: tokens(row.averageTokens.output),
      cacheRead: tokens(row.averageTokens.cacheRead),
      cacheWrite: tokens(row.averageTokens.cacheWrite),
      cost: money(row.averageCost),
    }
  }
  if (row.reported === 0) {
    return {
      input: ABSENT,
      output: ABSENT,
      cacheRead: ABSENT,
      cacheWrite: ABSENT,
      cost: ABSENT,
    }
  }
  return {
    input: tokens(row.tokens.input),
    output: tokens(row.tokens.output),
    cacheRead: tokens(row.tokens.cacheRead),
    cacheWrite: tokens(row.tokens.cacheWrite),
    cost: money(row.cost),
  }
}

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
export function coverage(report: UsageReportView, window?: UsageWindow): string {
  const base = `reported by ${report.reported} of ${report.records} records`
  if (window === undefined || window === 'all-time') return base
  if (window === 'last-7-days') return `${base} in the last 7 days`
  if (window === 'last-30-days') return `${base} in the last 30 days`
  return `${base} in this month`
}

/**
 * The ticket table's arithmetic in plain words: what its rows add to, how that
 * compares to the spend above it, and the spend no ticket can claim. A shared
 * record is counted in each ticket row it carries, so the rows add to more
 * than the spend whenever overlap is positive. Every figure is a report
 * field, and no ticket rows means no line.
 */
export function ticketArithmetic(report: UsageReportView): string | null {
  if (report.ticket.length === 0) return null

  const head =
    report.overlap > 0
      ? `Ticket rows add to ${money(report.ticketTotal)}, ${money(report.overlap)} more than the ${money(report.cost)} spent: a record on two tickets is counted in both rows.`
      : `Ticket rows add to ${money(report.ticketTotal)}, matching the ${money(report.cost)} spent: each record is counted once.`

  const loose = report.ticket.find((row) => row.id === NO_TICKET)
  if (loose === undefined) return head
  if (loose.reported === 0) {
    return `${head} ${recordNoun(loose.records)} carried no ticket and reported nothing.`
  }
  return `${head} ${recordNoun(loose.records)} carried no ticket (${money(loose.cost)}).`
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

/** The graph's series: the day's token pieces, its record count and its cost. */
export type UsageSeries =
  | 'totalTokens'
  | 'input'
  | 'output'
  | 'cacheRead'
  | 'cacheWrite'
  | 'reasoning'
  | 'records'
  | 'cost'

/** Every series the picker offers, token-first with cost last as the opt-in. */
export const GRAPH_SERIES: readonly UsageSeries[] = [
  'totalTokens',
  'input',
  'output',
  'cacheRead',
  'cacheWrite',
  'reasoning',
  'records',
  'cost',
]

/** The graph's opening series: total tokens per day, cost kept secondary. */
export const DEFAULT_GRAPH_SERIES: readonly UsageSeries[] = ['totalTokens']

const SERIES_LABELS: Record<UsageSeries, string> = {
  totalTokens: 'Total tokens',
  input: 'Input',
  output: 'Output',
  cacheRead: 'Cache read',
  cacheWrite: 'Cache write',
  reasoning: 'Reasoning',
  records: 'Records',
  cost: 'Cost',
}

export function seriesLabel(series: UsageSeries): string {
  return SERIES_LABELS[series]
}

/**
 * The day's total tokens: the sum of the reported pieces. Absent when the day
 * reported no piece, never zero dressed up: a reported zero still sums.
 */
export function totalTokens(row: UsageRowView): number | null {
  const pieces = [
    row.tokens.input,
    row.tokens.output,
    row.tokens.reasoning,
    row.tokens.cacheRead,
    row.tokens.cacheWrite,
  ]
  if (pieces.every((piece) => piece === null)) return null
  return pieces.reduce<number>((sum, piece) => sum + (piece ?? 0), 0)
}

/** One series' value for one day: absent stays absent, never zero. */
export function seriesValue(row: UsageRowView, series: UsageSeries): number | null {
  switch (series) {
    case 'totalTokens':
      return totalTokens(row)
    case 'input':
      return row.tokens.input
    case 'output':
      return row.tokens.output
    case 'cacheRead':
      return row.tokens.cacheRead
    case 'cacheWrite':
      return row.tokens.cacheWrite
    case 'reasoning':
      return row.tokens.reasoning
    case 'records':
      return row.records
    case 'cost':
      return row.cost
  }
}

/** The graph's three scales: token pieces share one, records and cost keep their own. */
export type GraphScales = {
  tokenMax: number
  recordsMax: number
  costMax: number
}

const TOKEN_SERIES: readonly UsageSeries[] = [
  'totalTokens',
  'input',
  'output',
  'cacheRead',
  'cacheWrite',
  'reasoning',
]

/**
 * The largest value per scale over the days in view: token series share the
 * token scale so one bar never hides another token bar, while records and
 * cost draw against their own labeled scales. Absent values never set a scale.
 */
export function graphScales(
  days: UsageRowView[],
  series: readonly UsageSeries[],
): GraphScales {
  let tokenMax = 0
  let recordsMax = 0
  let costMax = 0
  for (const row of days) {
    for (const name of series) {
      const value = seriesValue(row, name)
      if (value === null) continue
      if (name === 'cost') costMax = Math.max(costMax, value)
      else if (name === 'records') recordsMax = Math.max(recordsMax, value)
      else tokenMax = Math.max(tokenMax, value)
    }
  }
  return { tokenMax, recordsMax, costMax }
}

/** One series' bar height against its scale: absent and empty scales draw zero. */
export function seriesHeight(value: number | null, max: number): number {
  if (value === null || max <= 0) return 0
  return (value / max) * 100
}

/** One day column of the combined graph: its row and per-series values and heights. */
export type GraphColumn = {
  row: UsageRowView
  values: Partial<Record<UsageSeries, number | null>>
  heights: Partial<Record<UsageSeries, number>>
}

/**
 * The day rows as combined graph columns, oldest first to match the day table.
 * A day that reported nothing keeps its slot with no values; absent stays
 * absent, never zero, while its height is zero so the slot stays legible.
 */
export function graphBars(
  days: UsageRowView[],
  series: readonly UsageSeries[],
): GraphColumn[] {
  const oldestFirst = [...days].reverse()
  const scales = graphScales(oldestFirst, series)
  return oldestFirst.map((row) => {
    const values: Partial<Record<UsageSeries, number | null>> = {}
    const heights: Partial<Record<UsageSeries, number>> = {}
    for (const name of series) {
      const value = seriesValue(row, name)
      values[name] = value
      const max = name === 'cost' ? scales.costMax : name === 'records' ? scales.recordsMax : scales.tokenMax
      heights[name] = seriesHeight(value, max)
    }
    return { row, values, heights }
  })
}

/** The sortable table columns, in header order: label plus every numeric the face draws. */
export type UsageSortColumn =
  | 'label'
  | 'records'
  | 'input'
  | 'output'
  | 'cacheRead'
  | 'cacheWrite'
  | 'cost'

export const SORT_COLUMNS: readonly UsageSortColumn[] = [
  'label',
  'records',
  'input',
  'output',
  'cacheRead',
  'cacheWrite',
  'cost',
]

export type UsageSortDirection = 'asc' | 'desc'

export type UsageSort = {
  column: UsageSortColumn
  direction: UsageSortDirection
}

/** The opening direction: label ascending first, numerics descending first. */
export function initialDirection(column: UsageSortColumn): UsageSortDirection {
  return column === 'label' ? 'asc' : 'desc'
}

/**
 * The table's opening sort: model, agent and ticket open label A-Z; day stays
 * newest-first chronological and is not sortable, since it feeds the chart.
 */
export function defaultSort(dimension: UsageDimension): UsageSort | null {
  if (dimension === 'day') return null
  return { column: 'label', direction: 'asc' }
}

/** The next sort after a header pick: the same column toggles, a new one opens. */
export function toggleSort(current: UsageSort, column: UsageSortColumn): UsageSort {
  if (current.column === column) {
    return { column, direction: current.direction === 'asc' ? 'desc' : 'asc' }
  }
  return { column, direction: initialDirection(column) }
}

/**
 * One row's value for a sortable column in the active mode, on the displayed
 * values: totals, or per-reported-record averages. Records never averages.
 * Absent draws last: a piece no record reported, and a cost on a row that
 * reported nothing, sort as null rather than zero.
 */
function sortValue(
  row: UsageRowView,
  column: UsageSortColumn,
  mode: UsageMode,
): string | number | null {
  switch (column) {
    case 'label':
      return row.label
    case 'records':
      return row.records
    case 'input':
      return mode === 'average' ? row.averageTokens.input : row.tokens.input
    case 'output':
      return mode === 'average' ? row.averageTokens.output : row.tokens.output
    case 'cacheRead':
      return mode === 'average' ? row.averageTokens.cacheRead : row.tokens.cacheRead
    case 'cacheWrite':
      return mode === 'average' ? row.averageTokens.cacheWrite : row.tokens.cacheWrite
    case 'cost':
      if (row.reported === 0) return null
      return mode === 'average' ? row.averageCost : row.cost
  }
}

function compareLabel(a: string, b: string): number {
  if (a < b) return -1
  if (a > b) return 1
  return 0
}

/**
 * The rows for display: a sorted copy, never a rescale. Ties break by label
 * ascending so two reads agree, and bars keep the report's own cost share
 * under any order: sort reorders, it never rescales.
 */
export function sortRows(
  rows: UsageRowView[],
  column: UsageSortColumn,
  direction: UsageSortDirection,
  mode: UsageMode,
): UsageRowView[] {
  return [...rows].sort((a, b) => {
    if (column === 'label') {
      const order = compareLabel(a.label, b.label)
      return direction === 'asc' ? order : -order
    }
    const first = sortValue(a, column, mode)
    const second = sortValue(b, column, mode)
    if (first === null && second === null) return compareLabel(a.label, b.label)
    if (first === null) return 1
    if (second === null) return -1
    const order =
      (first as number) < (second as number) ? -1 : (first as number) > (second as number) ? 1 : 0
    if (order !== 0) return direction === 'asc' ? order : -order
    return compareLabel(a.label, b.label)
  })
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
