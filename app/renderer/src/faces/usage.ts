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

/** A chartable series: a token piece, the token total, or the spend. */
export type UsageSeries = 'totalTokens' | 'input' | 'output' | 'cacheRead' | 'cacheWrite' | 'cost'

const SERIES_LABELS: Record<UsageSeries, string> = {
  totalTokens: 'Total tokens',
  input: 'Input',
  output: 'Output',
  cacheRead: 'Cache read',
  cacheWrite: 'Cache write',
  cost: 'Cost',
}

export function seriesLabel(series: UsageSeries): string {
  return SERIES_LABELS[series]
}

/**
 * The four charts, split by scale: input and output share one order of
 * magnitude, cache read and write another, the token total sums both, and the
 * cost is its own unit. The records chart and the reasoning bar are gone.
 */
export type ChartKind = 'tokens' | 'cache' | 'total' | 'cost'

export const CHART_KINDS: readonly ChartKind[] = ['tokens', 'cache', 'total', 'cost']

export const CHART_TITLES: Record<ChartKind, string> = {
  tokens: 'Tokens',
  cache: 'Cache',
  total: 'Total tokens',
  cost: 'Cost',
}

/** The tokens chart groups input and output, in this order. */
export const TOKEN_PIECES: readonly UsageSeries[] = ['input', 'output']

/** The cache chart groups cache read and cache write, in this order. */
export const CACHE_PIECES: readonly UsageSeries[] = ['cacheRead', 'cacheWrite']

/** Each grouped chart opens on all its pieces; a picker can turn one off. */
export const DEFAULT_TOKEN_SERIES: readonly UsageSeries[] = [...TOKEN_PIECES]
export const DEFAULT_CACHE_SERIES: readonly UsageSeries[] = [...CACHE_PIECES]

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
    case 'cost':
      return row.cost
  }
}

/** One axis tick: the gridline at Half or Max of the chart's own scale. */
export type ChartTick = {
  value: number
  /** The formatted value: tokens through `tokens()`, cost through `money()`. */
  label: string
  /** The gridline's height above the baseline, as a percentage of the track. */
  heightPercent: number
}

/** One drawn bar: a reported value and its share of the chart's scale. */
export type ChartBar = {
  series: UsageSeries
  value: number
  heightPercent: number
}

/** One chart column: a day, or an All-time week of days. */
export type ChartColumn = {
  label: string
  /** The day rows behind the column, oldest first. */
  rows: UsageRowView[]
  /** The column's value on the chart's unit: the tallest grouped bar, or the one bar. */
  amount: number
  /** The drawn bars in the chart's own piece order; an absent value draws no bar. */
  bars: ChartBar[]
}

/** One chart: four always-visible charts, each with its own scale and ticks. */
export type UsageChart = {
  kind: ChartKind
  /** True when the columns are Monday-started weeks rather than days. */
  weekly: boolean
  max: number
  ticks: ChartTick[]
  columns: ChartColumn[]
}

export type UsageChartOptions = {
  /** A stacked chart's drawn pieces; the single-series charts ignore it. */
  series?: readonly UsageSeries[]
  /** Bucket the day rows into Monday-started weeks, All time only. */
  weekly?: boolean
}

const DAY_MS = 86_400_000

/**
 * The Monday of the week a `YYYY-MM-DD` label falls in, its ISO date. Plain
 * calendar arithmetic pinned to UTC, so a local zone can never shift a day
 * across a week boundary.
 */
export function weekStart(label: string): string {
  const [year = 0, month = 1, day = 1] = label.split('-').map(Number)
  const date = new Date(Date.UTC(year, month - 1, day))
  const sinceMonday = (date.getUTCDay() + 6) % 7
  return formatIsoDate(new Date(date.getTime() - sinceMonday * DAY_MS))
}

function formatIsoDate(date: Date): string {
  const year = date.getUTCFullYear()
  const month = String(date.getUTCMonth() + 1).padStart(2, '0')
  const day = String(date.getUTCDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

type ChartBucket = {
  label: string
  rows: UsageRowView[]
}

/**
 * The chart's columns: the day rows oldest first, or All time's weeks that
 * start Monday and carry the week's first day as their label. Bucketing is
 * chart-side only; the Day table keeps the report's day rows.
 */
function chartBuckets(days: UsageRowView[], weekly: boolean): ChartBucket[] {
  const oldestFirst = [...days].reverse()
  if (!weekly) return oldestFirst.map((row) => ({ label: row.label, rows: [row] }))

  const groups = new Map<string, UsageRowView[]>()
  for (const row of oldestFirst) {
    const start = weekStart(row.label)
    const bucket = groups.get(start)
    if (bucket === undefined) groups.set(start, [row])
    else bucket.push(row)
  }
  return [...groups.entries()].map(([label, rows]) => ({ label, rows }))
}

/**
 * The chart's ticks: Half and Max of its own maximum, formatted in its unit.
 * A zero maximum draws no tick rather than dividing by zero.
 */
function chartTicks(max: number, kind: ChartKind): ChartTick[] {
  if (max <= 0) return []
  return [
    { value: max / 2, label: tickLabel(max / 2, kind), heightPercent: 50 },
    { value: max, label: tickLabel(max, kind), heightPercent: 100 },
  ]
}

function tickLabel(value: number, kind: ChartKind): string {
  return kind === 'cost' ? money(value) : tokens(value)
}

function percent(value: number, max: number): number {
  return max > 0 ? (value / max) * 100 : 0
}

/** A series summed over a column's rows; absent stays absent unless a piece reports. */
function sumSeries(rows: UsageRowView[], series: UsageSeries): number | null {
  let total: number | null = null
  for (const row of rows) {
    const value = seriesValue(row, series)
    if (value === null) continue
    total = (total ?? 0) + value
  }
  return total
}

/**
 * The column's value on the chart's own scale: the tallest of the grouped
 * chart's bars, the token total, or the spend. The pieces are read from the
 * chart's full set, never from the picker's state, so turning a bar off never
 * rescales the axis, and an absent piece contributes no bar rather than zero.
 */
function scaleAmount(rows: UsageRowView[], kind: ChartKind): number {
  switch (kind) {
    case 'tokens':
      return Math.max(sumSeries(rows, 'input') ?? 0, sumSeries(rows, 'output') ?? 0)
    case 'cache':
      return Math.max(sumSeries(rows, 'cacheRead') ?? 0, sumSeries(rows, 'cacheWrite') ?? 0)
    case 'total':
      return sumSeries(rows, 'totalTokens') ?? 0
    case 'cost':
      return rows.reduce((sum, row) => sum + row.cost, 0)
  }
}

/**
 * The column's drawable bars: a grouped chart's active pieces, or the one bar
 * of the totals and cost charts. A piece no row reported draws no bar, never a
 * zero bar.
 */
function chartBars(
  rows: UsageRowView[],
  kind: ChartKind,
  series: readonly UsageSeries[],
): Array<{ series: UsageSeries; value: number }> {
  if (kind === 'tokens' || kind === 'cache') {
    // The chart's own piece order holds however the picker was toggled.
    const pieces = kind === 'tokens' ? TOKEN_PIECES : CACHE_PIECES
    // Each piece is a whole day's value for that series, drawn beside the others.
    return pieces.flatMap((name) => {
      if (!series.includes(name)) return []
      const value = sumSeries(rows, name)
      return value === null ? [] : [{ series: name, value }]
    })
  }
  const name: UsageSeries = kind === 'total' ? 'totalTokens' : 'cost'
  return [{ series: name, value: scaleAmount(rows, kind) }]
}

/**
 * One chart over the day table: the four charts draw where they are read, each
 * against its own maximum, so the token pieces and the cache pieces live on
 * scales an order of magnitude apart and neither borrows the other's. The
 * grouped charts scale to the tallest bar actually drawn, never to a column's
 * sum of pieces.
 */
export function usageChart(
  days: UsageRowView[],
  kind: ChartKind,
  options: UsageChartOptions = {},
): UsageChart {
  const weekly = options.weekly ?? false
  const series =
    kind === 'tokens'
      ? (options.series ?? DEFAULT_TOKEN_SERIES)
      : kind === 'cache'
        ? (options.series ?? DEFAULT_CACHE_SERIES)
        : []
  const buckets = chartBuckets(days, weekly)
  const amounts = buckets.map((bucket) => scaleAmount(bucket.rows, kind))
  const max = amounts.reduce((top, amount) => Math.max(top, amount), 0)
  const ticks = chartTicks(max, kind)
  const columns = buckets.map((bucket, index) => ({
    label: bucket.label,
    rows: bucket.rows,
    amount: amounts[index] ?? 0,
    bars: chartBars(bucket.rows, kind, series).map((bar) => ({
      ...bar,
      heightPercent: percent(bar.value, max),
    })),
  }))
  return { kind, weekly, max, ticks, columns }
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
