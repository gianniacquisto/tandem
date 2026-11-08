import { describe, expect, it } from 'vitest'

import {
  ABSENT,
  coverage,
  dimensionLabel,
  dimensionRows,
  modeLabel,
  money,
  recordNoun,
  rowCells,
  selectedRow,
  startedAt,
  ticketArithmetic,
  DIMENSIONS,
  MODES,
  MODE_CONTROL_LABEL,
  tokenLine,
  tokens,
  WINDOWS,
  DEFAULT_WINDOW,
  windowLabel,
  GRAPH_SERIES,
  TOKEN_PIECES,
  TOKEN_CHART_SERIES,
  DEFAULT_TOKEN_SERIES,
  seriesLabel,
  seriesValue,
  totalTokens,
  usageChart,
  weekStart,
  SORT_COLUMNS,
  defaultSort,
  initialDirection,
  sortRows,
  toggleSort,
} from '../renderer/src/faces/usage'
import type { UsageReportView } from '../renderer/src/wire/UsageReportView'
import type { UsageRowView } from '../renderer/src/wire/UsageRowView'
import type { UsageSessionView } from '../renderer/src/wire/UsageSessionView'

/** The frozen fixture home's numbers, as the core's `usage.report` answers them. */
const NO_TICKET = 'no-ticket'
const TICKET_ONE = '00000000-0000-4000-8000-000000000001'
const TICKET_TWO = '00000000-0000-4000-8000-000000000002'

function session(overrides: Partial<UsageSessionView> = {}): UsageSessionView {
  return {
    id: 'ses_a',
    agent: 'builder',
    model: 'deepseek-v4.1-flash',
    startedAt: '2026-10-30T12:00:00Z',
    cost: 1,
    tokens: { input: 1000, output: 500, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
    tickets: [],
    ...overrides,
  }
}

function row(overrides: Partial<UsageRowView> = {}): UsageRowView {
  return {
    id: 'row',
    label: 'row',
    records: 1,
    reported: 1,
    cost: 1,
    barPercent: 100,
    tokens: { input: 1000, output: 500, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
    averageTokens: { input: 500, output: 250, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
    averageCost: 0.5,
    sessions: [session()],
    ...overrides,
  }
}

function report(overrides: Partial<UsageReportView> = {}): UsageReportView {
  return {
    records: 4,
    reported: 3,
    cost: 2.5,
    tokens: { input: 6000, output: 1500, reasoning: 100, cacheRead: 200, cacheWrite: 50 },
    day: [
      row({
        id: '2026-10-31',
        label: '2026-10-31',
        records: 2,
        reported: 1,
        cost: 1.5,
        barPercent: 100,
        sessions: [session({ id: 'ses_b', cost: 1.5 })],
      }),
      row({
        id: '2026-10-30',
        label: '2026-10-30',
        records: 2,
        reported: 2,
        cost: 1,
        barPercent: 66.66666666666666,
        sessions: [session({ id: 'ses_a' })],
      }),
    ],
    model: [],
    agent: [
      row({ id: 'builder', label: 'builder', records: 2, reported: 2, cost: 1.75, barPercent: 100 }),
      row({
        id: 'reviewer',
        label: 'reviewer',
        cost: 0.75,
        barPercent: 42.857142857142854,
      }),
      row({ id: '', label: 'unattributed', records: 1, reported: 0, cost: 0, barPercent: 0 }),
    ],
    ticket: [
      row({
        id: TICKET_ONE,
        label: 'FIX-1: the board',
        records: 2,
        reported: 2,
        cost: 1,
        barPercent: 66.66666666666666,
      }),
      row({
        id: TICKET_TWO,
        label: 'FIX-2: the records',
        records: 2,
        reported: 1,
        cost: 0.75,
        barPercent: 50,
      }),
      row({
        id: NO_TICKET,
        label: 'no ticket',
        records: 1,
        reported: 1,
        cost: 1.5,
        barPercent: 100,
        sessions: [session({ id: 'ses_loose', cost: 1.5, tickets: [] })],
      }),
    ],
    overlap: 0.75,
    ticketTotal: 3.25,
    ...overrides,
  }
}

describe('the usage face over one report', () => {
  it('draws one dimension at a time, in the report\u2019s own order', () => {
    const drawn = report()
    expect(DIMENSIONS.map(dimensionLabel)).toEqual([
      'Day',
      'Model',
      'Agent',
      'Ticket',
    ])
    expect(dimensionRows(drawn, 'day')).toBe(drawn.day)
    expect(dimensionRows(drawn, 'agent').map((entry) => entry.label)).toEqual([
      'builder',
      'reviewer',
      'unattributed',
    ])
    expect(dimensionRows(drawn, 'ticket')).toHaveLength(3)
  })

  it('names the spend, the coverage and the ticket arithmetic from the report', () => {
    const drawn = report()
    expect(money(drawn.cost)).toBe('$2.50')
    expect(coverage(drawn)).toBe('reported by 3 of 4 records')
    expect(recordNoun(1)).toBe('1 record')
    expect(recordNoun(8)).toBe('8 records')
    expect(ticketArithmetic(drawn)).toBe(
      'Ticket rows add to $3.25, $0.75 more than the $2.50 spent: a record on two tickets is counted in both rows. 1 record carried no ticket ($1.50).',
    )
  })

  it('says a ticketless record that reported nothing without a cost', () => {
    const drawn = report({
      cost: 0,
      ticket: [
        row({ id: NO_TICKET, label: 'no ticket', records: 2, reported: 0, cost: 0, barPercent: 0 }),
      ],
      overlap: 0,
      ticketTotal: 0,
    })
    expect(ticketArithmetic(drawn)).toBe(
      'Ticket rows add to $0.00, matching the $0.00 spent: each record is counted once. 2 records carried no ticket and reported nothing.',
    )
  })

  it('states no ticket arithmetic when the report has no ticket rows', () => {
    expect(ticketArithmetic(report({ ticket: [], overlap: 0, ticketTotal: 0 }))).toBeNull()
  })

  it('says shared rows double count without a ticketless row', () => {
    const drawn = report({
      ticket: [
        row({ id: TICKET_ONE, label: 'FIX-1: the board', records: 2, reported: 2, cost: 1, barPercent: 66 }),
        row({ id: TICKET_TWO, label: 'FIX-2: the records', records: 2, reported: 1, cost: 0.75, barPercent: 50 }),
      ],
      overlap: 0.75,
      ticketTotal: 3.25,
    })
    expect(ticketArithmetic(drawn)).toBe(
      'Ticket rows add to $3.25, $0.75 more than the $2.50 spent: a record on two tickets is counted in both rows.',
    )
  })

  it('says a ticketless spend with no overlap in plain words', () => {
    const drawn = report({
      cost: 2.5,
      ticket: [
        row({ id: TICKET_ONE, label: 'FIX-1: the board', records: 1, reported: 1, cost: 1, barPercent: 66 }),
        row({ id: NO_TICKET, label: 'no ticket', records: 1, reported: 1, cost: 1.5, barPercent: 100 }),
      ],
      overlap: 0,
      ticketTotal: 2.5,
    })
    expect(ticketArithmetic(drawn)).toBe(
      'Ticket rows add to $2.50, matching the $2.50 spent: each record is counted once. 1 record carried no ticket ($1.50).',
    )
  })

  it('draws a home whose records predate usage as zeros with a coverage of zero', () => {
    const predates = report({ records: 3, reported: 0, cost: 0, overlap: 0, ticketTotal: 0 })
    expect(money(predates.cost)).toBe('$0.00')
    expect(coverage(predates)).toBe('reported by 0 of 3 records')
  })

  it('formats tokens with grouping, and a piece no record reported as absent', () => {
    expect(tokens(6000)).toBe('6,000')
    expect(tokens(0)).toBe('0')
    expect(tokens(null)).toBe('\u2014')
  })

  it('names a record\u2019s start in the reader\u2019s zone, without seconds', () => {
    expect(startedAt('2026-10-30T12:00:00Z', 'UTC')).toBe('Oct 30, 12:00 PM')
    expect(startedAt('2026-10-31T23:30:00Z', 'UTC')).toBe('Oct 31, 11:30 PM')
  })

  it('spells a record\u2019s tokens each by its own word', () => {
    expect(tokenLine(session())).toBe('1,000 in \u00b7 500 out \u00b7 0 cache read \u00b7 0 cache write \u00b7 0 reasoning')
    expect(tokenLine(session({ tokens: null }))).toBe('no tokens reported')
    expect(tokenLine(session({ tokens: { input: null, output: 20, reasoning: null, cacheRead: null, cacheWrite: null } }))).toBe(
      '20 out',
    )
  })

  it('keeps a selected row through a reload that still holds it', () => {
    const drawn = report()
    expect(selectedRow(drawn, 'day', '2026-10-30')?.label).toBe('2026-10-30')

    const reloaded = report()
    expect(selectedRow(reloaded, 'day', '2026-10-30')?.label).toBe('2026-10-30')
    expect(selectedRow(reloaded, 'day', '2026-10-29')).toBeNull()
    expect(selectedRow(reloaded, 'agent', 'builder')?.label).toBe('builder')
    expect(selectedRow(reloaded, 'model', 'builder')).toBeNull()
  })

  it('scales the day chart to the largest day cost', () => {
    const days = [
      row({ id: '2026-10-31', label: '2026-10-31', cost: 1 }),
      row({ id: '2026-10-30', label: '2026-10-30', cost: 2 }),
    ]
    const chart = usageChart(days, 'cost')
    expect(chart.columns.map((column) => column.label)).toEqual(['2026-10-30', '2026-10-31'])
    expect(chart.columns.map((column) => column.bars[0]?.heightPercent)).toEqual([100, 50])
    expect(chart.max).toBe(2)
  })

  it('draws zero heights when no day reported anything', () => {
    const days = [
      row({ id: '2026-10-31', label: '2026-10-31', reported: 0, cost: 0, records: 0 }),
      row({ id: '2026-10-30', label: '2026-10-30', reported: 0, cost: 0, records: 0 }),
    ]
    for (const unit of ['records', 'cost'] as const) {
      const chart = usageChart(days, unit)
      expect(chart.max).toBe(0)
      expect(chart.ticks).toEqual([])
      expect(chart.columns.map((column) => column.bars[0]?.heightPercent)).toEqual([0, 0])
    }
  })

  it('orders the day chart oldest first', () => {
    const days = [
      row({ id: '2026-11-01', label: '2026-11-01', cost: 3 }),
      row({ id: '2026-10-31', label: '2026-10-31', cost: 3 }),
      row({ id: '2026-10-30', label: '2026-10-30', cost: 3 }),
    ]
    expect(usageChart(days, 'cost').columns.map((column) => column.label)).toEqual([
      '2026-10-30',
      '2026-10-31',
      '2026-11-01',
    ])
  })

  it('keeps a zero-cost day in its slot with zero height', () => {
    const days = [
      row({ id: '2026-10-31', label: '2026-10-31', cost: 2 }),
      row({ id: '2026-10-30', label: '2026-10-30', cost: 0 }),
      row({ id: '2026-10-29', label: '2026-10-29', cost: 1 }),
    ]
    const chart = usageChart(days, 'cost')
    expect(chart.columns.map((column) => column.label)).toEqual([
      '2026-10-29',
      '2026-10-30',
      '2026-10-31',
    ])
    expect(chart.columns.map((column) => column.bars[0]?.heightPercent)).toEqual([50, 0, 100])
  })

  it('reads totals in Total mode, byte-identical to today', () => {
    const entry = row({
      records: 2,
      reported: 2,
      cost: 3,
      tokens: { input: 1000, output: 500, reasoning: 0, cacheRead: 200, cacheWrite: 100 },
      averageTokens: { input: 500, output: 250, reasoning: 0, cacheRead: 100, cacheWrite: 50 },
      averageCost: 1.5,
    })
    expect(MODES.map(modeLabel)).toEqual(['Total', 'Average'])
    expect(rowCells(entry, 'total')).toEqual({
      input: '1,000',
      output: '500',
      cacheRead: '200',
      cacheWrite: '100',
      cost: '$3.00',
    })
  })

  it('reads per-reported-record averages in Average mode', () => {
    const entry = row({
      records: 2,
      reported: 2,
      cost: 3,
      tokens: { input: 1000, output: 500, reasoning: 0, cacheRead: 200, cacheWrite: 100 },
      averageTokens: { input: 500, output: 250, reasoning: 0, cacheRead: 100, cacheWrite: 50 },
      averageCost: 1.5,
    })
    expect(rowCells(entry, 'average')).toEqual({
      input: '500',
      output: '250',
      cacheRead: '100',
      cacheWrite: '50',
      cost: '$1.50',
    })
  })

  it('selects absent for every numeric cell when the row reported nothing', () => {
    const entry = row({
      records: 2,
      reported: 0,
      cost: 0,
      tokens: { input: null, output: null, reasoning: null, cacheRead: null, cacheWrite: null },
      averageTokens: { input: null, output: null, reasoning: null, cacheRead: null, cacheWrite: null },
      averageCost: 0,
    })
    expect(rowCells(entry, 'average')).toEqual({
      input: ABSENT,
      output: ABSENT,
      cacheRead: ABSENT,
      cacheWrite: ABSENT,
      cost: ABSENT,
    })
    expect(rowCells(entry, 'total').cost).toBe(ABSENT)
  })

  it('draws a missing piece with the same absent mark as a missing cost', () => {
    const entry = row({
      records: 1,
      reported: 1,
      cost: 1.5,
      tokens: { input: 3000, output: null, reasoning: null, cacheRead: null, cacheWrite: null },
      averageTokens: { input: 3000, output: null, reasoning: null, cacheRead: null, cacheWrite: null },
      averageCost: 1.5,
    })
    expect(rowCells(entry, 'total')).toEqual({
      input: '3,000',
      output: ABSENT,
      cacheRead: ABSENT,
      cacheWrite: ABSENT,
      cost: '$1.50',
    })
    expect(rowCells(entry, 'average').output).toBe(ABSENT)
    expect(tokens(null)).toBe(ABSENT)
  })

  it('names the toggle for records, never tasks', () => {
    const copy = [...MODES.map(modeLabel), MODE_CONTROL_LABEL].join(' ')
    expect(copy).toContain('record')
    expect(copy).not.toMatch(/task/i)
  })

  it('offers four windows defaulting to the last seven days', () => {
    expect(WINDOWS).toEqual(['last-7-days', 'last-30-days', 'this-month', 'all-time'])
    expect(DEFAULT_WINDOW).toBe('last-7-days')
    expect(WINDOWS.map(windowLabel)).toEqual(['Last 7 days', 'Last 30 days', 'This month', 'All time'])
  })

  it('names the window in the header coverage', () => {
    const drawn = report()
    expect(coverage(drawn)).toBe('reported by 3 of 4 records')
    expect(coverage(drawn, 'last-7-days')).toBe('reported by 3 of 4 records in the last 7 days')
    expect(coverage(drawn, 'last-30-days')).toBe('reported by 3 of 4 records in the last 30 days')
    expect(coverage(drawn, 'this-month')).toBe('reported by 3 of 4 records in this month')
    expect(coverage(drawn, 'all-time')).toBe('reported by 3 of 4 records')
  })

  it('opens the tokens chart on its five pieces stacked with total tokens off', () => {
    expect(GRAPH_SERIES).toEqual([
      'totalTokens',
      'input',
      'output',
      'cacheRead',
      'cacheWrite',
      'reasoning',
      'records',
      'cost',
    ])
    expect(TOKEN_PIECES).toEqual(['input', 'output', 'cacheRead', 'cacheWrite', 'reasoning'])
    expect(TOKEN_CHART_SERIES).toEqual(['totalTokens', ...TOKEN_PIECES])
    expect(DEFAULT_TOKEN_SERIES).toEqual([...TOKEN_PIECES])
    expect(TOKEN_CHART_SERIES.map(seriesLabel)).toEqual([
      'Total tokens',
      'Input',
      'Output',
      'Cache read',
      'Cache write',
      'Reasoning',
    ])

    const day = row({
      tokens: { input: 3000, output: 500, reasoning: 100, cacheRead: 200, cacheWrite: 50 },
    })
    const stacked = usageChart([day], 'tokens')
    expect(stacked.columns[0]?.bars.map((bar) => bar.series)).toEqual([...TOKEN_PIECES])

    const total = usageChart([day], 'tokens', { series: ['totalTokens'] })
    expect(total.columns[0]?.bars.map((bar) => bar.series)).toEqual(['totalTokens'])
    expect(total.columns[0]?.bars[0]?.value).toBe(3850)
    expect(total.columns[0]?.bars[0]?.heightPercent).toBe(100)
  })

  it('stacks the token pieces into one bar per day and a piece with no value draws no slice, never a zero slice', () => {
    const days = [
      row({
        id: '2026-10-31',
        label: '2026-10-31',
        tokens: { input: 3000, output: null, reasoning: 0, cacheRead: 200, cacheWrite: null },
      }),
      row({
        id: '2026-10-30',
        label: '2026-10-30',
        tokens: { input: 1000, output: 500, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
      }),
    ]
    const chart = usageChart(days, 'tokens')
    expect(chart.columns.map((column) => column.label)).toEqual(['2026-10-30', '2026-10-31'])

    const [older, newer] = chart.columns
    expect(older?.bars.map((bar) => bar.series)).toEqual([
      'input',
      'output',
      'cacheRead',
      'cacheWrite',
      'reasoning',
    ])
    // Absent stays absent: no output or cache write slice.
    expect(newer?.bars.map((bar) => bar.series)).toEqual(['input', 'cacheRead', 'reasoning'])
    expect(newer?.bars.find((bar) => bar.series === 'output')).toBeUndefined()

    // A reported zero keeps its zero-height slice.
    const reasoning = older?.bars.find((bar) => bar.series === 'reasoning')
    expect(reasoning?.value).toBe(0)
    expect(reasoning?.heightPercent).toBe(0)

    // Every slice is scaled to the stack's own maximum, the largest total.
    expect(chart.max).toBe(3200)
    expect(newer?.bars.find((bar) => bar.series === 'input')?.heightPercent).toBe(93.75)
    expect(older?.bars.find((bar) => bar.series === 'output')?.heightPercent).toBe(15.625)
  })

  it('ticks each chart at half and max of its own scale', () => {
    const days = [
      row({
        id: '2026-10-30',
        label: '2026-10-30',
        records: 4,
        cost: 2,
        tokens: { input: 3000, output: 0, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
      }),
      row({
        id: '2026-10-29',
        label: '2026-10-29',
        records: 2,
        cost: 1,
        tokens: { input: 1000, output: 0, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
      }),
    ]

    const tokenChart = usageChart(days, 'tokens')
    expect(tokenChart.max).toBe(3000)
    expect(tokenChart.ticks.map((tick) => tick.label)).toEqual(['1,500', '3,000'])
    expect(tokenChart.ticks.map((tick) => tick.heightPercent)).toEqual([50, 100])

    const recordChart = usageChart(days, 'records')
    expect(recordChart.max).toBe(4)
    expect(recordChart.ticks.map((tick) => tick.label)).toEqual(['2', '4'])

    const costChart = usageChart(days, 'cost')
    expect(costChart.max).toBe(2)
    expect(costChart.ticks.map((tick) => tick.label)).toEqual(['$1.00', '$2.00'])

    // A chart whose days report nothing on its scale draws no tick.
    const empty = usageChart(
      [
        row({
          id: '2026-10-28',
          label: '2026-10-28',
          reported: 0,
          cost: 0,
          records: 0,
          tokens: { input: null, output: null, reasoning: null, cacheRead: null, cacheWrite: null },
        }),
      ],
      'cost',
    )
    expect(empty.max).toBe(0)
    expect(empty.ticks).toEqual([])
  })

  it('all-time days bucket into Monday-started weeks, labeled by the week first day', () => {
    // 2026-11-01 is a Sunday; its Monday is 2026-10-26, a different column
    // from the Monday 2026-11-02 that follows it.
    expect(weekStart('2026-11-01')).toBe('2026-10-26')
    expect(weekStart('2026-11-02')).toBe('2026-11-02')
    expect(weekStart('2026-10-31')).toBe('2026-10-26')
    expect(weekStart('2026-10-26')).toBe('2026-10-26')

    const labels = ['2026-11-02', '2026-11-01', '2026-10-31', '2026-10-26']
    const days = labels.map((label, index) =>
      row({ id: label, label, cost: index + 1, records: 1 }),
    )

    const weekly = usageChart(days, 'cost', { weekly: true })
    expect(weekly.columns.map((column) => column.label)).toEqual([
      '2026-10-26',
      '2026-11-02',
    ])
    expect(weekly.columns[0]?.rows.map((entry) => entry.label)).toEqual([
      '2026-10-26',
      '2026-10-31',
      '2026-11-01',
    ])
    expect(weekly.columns[1]?.rows.map((entry) => entry.label)).toEqual(['2026-11-02'])
    expect(weekly.columns[0]?.bars[0]?.value).toBe(2 + 3 + 4)
    expect(weekly.columns[1]?.bars[0]?.value).toBe(1)
    expect(weekly.max).toBe(9)

    // The day windows keep one column per day row, unchanged.
    expect(usageChart(days, 'cost').columns.map((column) => column.label)).toEqual([
      '2026-10-26',
      '2026-10-31',
      '2026-11-01',
      '2026-11-02',
    ])
  })

  it('reads each picker series from the day row, absent never zero', () => {
    const entry = row({
      cost: 1.5,
      tokens: { input: 3000, output: null, reasoning: 100, cacheRead: 200, cacheWrite: null },
    })
    expect(totalTokens(entry)).toBe(3300)
    expect(seriesValue(entry, 'totalTokens')).toBe(3300)
    expect(seriesValue(entry, 'input')).toBe(3000)
    expect(seriesValue(entry, 'output')).toBeNull()
    expect(seriesValue(entry, 'cacheRead')).toBe(200)
    expect(seriesValue(entry, 'cacheWrite')).toBeNull()
    expect(seriesValue(entry, 'reasoning')).toBe(100)
    expect(seriesValue(entry, 'records')).toBe(1)
    expect(seriesValue(entry, 'cost')).toBe(1.5)
  })

  it('reads total tokens as absent when the day reported no pieces', () => {
    const entry = row({
      reported: 0,
      tokens: { input: null, output: null, reasoning: null, cacheRead: null, cacheWrite: null },
    })
    expect(totalTokens(entry)).toBeNull()
    expect(seriesValue(entry, 'totalTokens')).toBeNull()
  })

  it('draws each unit against its own scale', () => {
    const days = [
      row({ id: '2026-10-31', label: '2026-10-31', cost: 2, records: 10, tokens: { input: 6000, output: 0, reasoning: 0, cacheRead: 0, cacheWrite: 0 } }),
      row({ id: '2026-10-30', label: '2026-10-30', cost: 1, records: 5, tokens: { input: 3000, output: 0, reasoning: 0, cacheRead: 0, cacheWrite: 0 } }),
    ]
    const tokenChart = usageChart(days, 'tokens')
    expect(tokenChart.max).toBe(6000)
    expect(tokenChart.columns.map((column) => column.bars[0]?.heightPercent)).toEqual([50, 100])
    const recordChart = usageChart(days, 'records')
    expect(recordChart.max).toBe(10)
    expect(recordChart.columns.map((column) => column.bars[0]?.heightPercent)).toEqual([50, 100])
    const costChart = usageChart(days, 'cost')
    expect(costChart.max).toBe(2)
    expect(costChart.columns.map((column) => column.bars[0]?.heightPercent)).toEqual([50, 100])
  })

  it('keeps zero and absent token pieces in their slots', () => {
    const days = [
      row({ id: '2026-10-31', label: '2026-10-31', tokens: { input: 2000, output: 0, reasoning: 0, cacheRead: 0, cacheWrite: 0 } }),
      row({ id: '2026-10-30', label: '2026-10-30', reported: 0, tokens: { input: null, output: null, reasoning: null, cacheRead: null, cacheWrite: null } }),
      row({ id: '2026-10-29', label: '2026-10-29', tokens: { input: 0, output: 0, reasoning: 0, cacheRead: 0, cacheWrite: 0 } }),
    ]
    const chart = usageChart(days, 'tokens')
    expect(chart.columns.map((column) => column.label)).toEqual([
      '2026-10-29',
      '2026-10-30',
      '2026-10-31',
    ])
    // The day that reported nothing draws no slice at all.
    expect(chart.columns[1]?.bars).toEqual([])
    // A reported zero keeps its slice at zero height.
    expect(chart.columns[0]?.bars.find((bar) => bar.series === 'input')?.value).toBe(0)
    expect(chart.columns[0]?.bars.find((bar) => bar.series === 'input')?.heightPercent).toBe(0)
    expect(chart.columns[2]?.bars.find((bar) => bar.series === 'input')?.heightPercent).toBe(100)
  })

  it('sorts non-day tables by label A-Z with day exempt', () => {
    expect(SORT_COLUMNS).toEqual(['label', 'records', 'input', 'output', 'cacheRead', 'cacheWrite', 'cost'])
    expect(defaultSort('day')).toBeNull()
    expect(defaultSort('model')).toEqual({ column: 'label', direction: 'asc' })
    expect(defaultSort('agent')).toEqual({ column: 'label', direction: 'asc' })
    expect(defaultSort('ticket')).toEqual({ column: 'label', direction: 'asc' })
    const rows = [
      row({ id: 'b', label: 'builder', cost: 1 }),
      row({ id: 'a', label: 'agent', cost: 2 }),
    ]
    expect(sortRows(rows, 'label', 'asc', 'total').map((entry) => entry.label)).toEqual([
      'agent',
      'builder',
    ])
  })

  it('sorts numerics descending first with label tiebreak in both modes', () => {
    expect(initialDirection('label')).toBe('asc')
    expect(initialDirection('cost')).toBe('desc')
    expect(initialDirection('input')).toBe('desc')
    const rows = [
      row({
        id: 'b',
        label: 'builder',
        records: 2,
        reported: 2,
        cost: 2,
        tokens: { input: 2000, output: 0, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
        averageTokens: { input: 1000, output: 0, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
        averageCost: 1,
      }),
      row({
        id: 'a',
        label: 'agent',
        records: 1,
        reported: 1,
        cost: 2,
        tokens: { input: 1000, output: 0, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
        averageTokens: { input: 1000, output: 0, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
        averageCost: 2,
      }),
    ]
    expect(sortRows(rows, 'cost', 'desc', 'total').map((entry) => entry.label)).toEqual([
      'agent',
      'builder',
    ])
    expect(sortRows(rows, 'input', 'desc', 'total').map((entry) => entry.label)).toEqual([
      'builder',
      'agent',
    ])
    expect(sortRows(rows, 'input', 'desc', 'average').map((entry) => entry.label)).toEqual([
      'agent',
      'builder',
    ])
    expect(sortRows(rows, 'cost', 'desc', 'average').map((entry) => entry.label)).toEqual([
      'agent',
      'builder',
    ])
    expect(toggleSort({ column: 'cost', direction: 'desc' }, 'cost')).toEqual({
      column: 'cost',
      direction: 'asc',
    })
    expect(toggleSort({ column: 'cost', direction: 'desc' }, 'label')).toEqual({
      column: 'label',
      direction: 'asc',
    })
  })

  it('keeps bars cost-scaled under any sort', () => {
    const rows = [
      row({ id: 'b', label: 'builder', cost: 1, barPercent: 50 }),
      row({ id: 'a', label: 'agent', cost: 2, barPercent: 100 }),
    ]
    const sorted = sortRows(rows, 'label', 'asc', 'total')
    expect(sorted.map((entry) => entry.label)).toEqual(['agent', 'builder'])
    expect(sorted.map((entry) => entry.barPercent)).toEqual([100, 50])
  })
})
