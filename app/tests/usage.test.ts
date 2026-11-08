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
  CACHE_PIECES,
  CHART_KINDS,
  CHART_TITLES,
  DEFAULT_CACHE_SERIES,
  DEFAULT_TOKEN_SERIES,
  TOKEN_PIECES,
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
    const nothing = { input: null, output: null, reasoning: null, cacheRead: null, cacheWrite: null }
    const days = [
      row({
        id: '2026-10-31',
        label: '2026-10-31',
        reported: 0,
        cost: 0,
        records: 0,
        tokens: nothing,
      }),
      row({
        id: '2026-10-30',
        label: '2026-10-30',
        reported: 0,
        cost: 0,
        records: 0,
        tokens: nothing,
      }),
    ]
    for (const kind of ['tokens', 'cache'] as const) {
      const chart = usageChart(days, kind)
      expect(chart.max).toBe(0)
      expect(chart.ticks).toEqual([])
      // No reported piece draws no slice at all.
      expect(chart.columns.map((column) => column.bars)).toEqual([[], []])
    }
    for (const kind of ['total', 'cost'] as const) {
      const chart = usageChart(days, kind)
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

  it('draws four charts split by scale, with total tokens on its own', () => {
    expect(CHART_KINDS).toEqual(['tokens', 'cache', 'total', 'cost'])
    expect(CHART_KINDS.map((kind) => CHART_TITLES[kind])).toEqual([
      'Tokens',
      'Cache',
      'Total tokens',
      'Cost',
    ])
    expect(TOKEN_PIECES).toEqual(['input', 'output'])
    expect(CACHE_PIECES).toEqual(['cacheRead', 'cacheWrite'])
    expect(DEFAULT_TOKEN_SERIES).toEqual(['input', 'output'])
    expect(DEFAULT_CACHE_SERIES).toEqual(['cacheRead', 'cacheWrite'])
    expect(TOKEN_PIECES.map(seriesLabel)).toEqual(['Input', 'Output'])
    expect(CACHE_PIECES.map(seriesLabel)).toEqual(['Cache read', 'Cache write'])

    const day = row({
      tokens: { input: 3000, output: 500, reasoning: 100, cacheRead: 200, cacheWrite: 50 },
    })
    expect(usageChart([day], 'tokens').columns[0]?.bars.map((bar) => bar.series)).toEqual([
      'input',
      'output',
    ])
    expect(usageChart([day], 'cache').columns[0]?.bars.map((bar) => bar.series)).toEqual([
      'cacheRead',
      'cacheWrite',
    ])
    expect(usageChart([day], 'total').columns[0]?.bars.map((bar) => bar.series)).toEqual([
      'totalTokens',
    ])
    expect(usageChart([day], 'total').columns[0]?.bars[0]?.value).toBe(3850)
    expect(usageChart([day], 'cost').columns[0]?.bars.map((bar) => bar.series)).toEqual(['cost'])

    // The records chart and the reasoning slice are gone from every chart.
    const charted = CHART_KINDS.flatMap(
      (kind) => usageChart([day], kind).columns[0]?.bars.map((bar) => bar.series) ?? [],
    )
    expect(charted).not.toContain('records')
    expect(charted).not.toContain('reasoning')
  })

  it('draws input and output as grouped bars on one scale, and cache read and write on another', () => {
    const days = [
      row({
        id: '2026-10-31',
        label: '2026-10-31',
        tokens: {
          input: 2000,
          output: null,
          reasoning: 0,
          cacheRead: 800_000,
          cacheWrite: null,
        },
      }),
      row({
        id: '2026-10-30',
        label: '2026-10-30',
        tokens: {
          input: 6000,
          output: 3000,
          reasoning: 0,
          cacheRead: 1_200_000,
          cacheWrite: 300_000,
        },
      }),
    ]

    const tokens = usageChart(days, 'tokens')
    expect(tokens.columns.map((column) => column.label)).toEqual(['2026-10-30', '2026-10-31'])
    const [older, newer] = tokens.columns
    expect(older?.bars.map((bar) => bar.series)).toEqual(['input', 'output'])
    // Absent stays absent: no output bar on the newer day.
    expect(newer?.bars.map((bar) => bar.series)).toEqual(['input'])
    // The scale is the tallest single bar actually drawn, never the combined
    // input + output of one day.
    expect(tokens.max).toBe(6000)
    expect(tokens.columns.map((column) => column.amount)).toEqual([6000, 2000])
    expect(older?.bars.find((bar) => bar.series === 'input')?.heightPercent).toBe(100)
    expect(older?.bars.find((bar) => bar.series === 'output')?.heightPercent).toBe(50)
    expect(newer?.bars.find((bar) => bar.series === 'input')?.heightPercent).toBe(33.33333333333333)

    const cache = usageChart(days, 'cache')
    expect(cache.columns[0]?.bars.map((bar) => bar.series)).toEqual(['cacheRead', 'cacheWrite'])
    expect(cache.columns[1]?.bars.map((bar) => bar.series)).toEqual(['cacheRead'])
    expect(cache.max).toBe(1_200_000)
    expect(cache.columns[0]?.bars.map((bar) => bar.value)).toEqual([1_200_000, 300_000])
    expect(cache.columns[0]?.bars.map((bar) => bar.heightPercent)).toEqual([100, 25])
    expect(cache.columns[1]?.bars[0]?.heightPercent).toBe(66.66666666666666)
  })

  it('draws one grouped bar per series with a value, none for an absent value, and a zero height for a reported zero', () => {
    const days = [
      row({
        id: '2026-10-31',
        label: '2026-10-31',
        tokens: { input: 2000, output: 0, reasoning: 0, cacheRead: null, cacheWrite: null },
      }),
      row({
        id: '2026-10-30',
        label: '2026-10-30',
        tokens: { input: null, output: 500, reasoning: 0, cacheRead: null, cacheWrite: null },
      }),
    ]

    const chart = usageChart(days, 'tokens')
    // The older day reported only output: its group holds one bar.
    expect(chart.columns[0]?.bars.map((bar) => bar.series)).toEqual(['output'])
    // The newer day reported both pieces: input has a value, output a reported zero.
    expect(chart.columns[1]?.bars.map((bar) => bar.series)).toEqual(['input', 'output'])
    expect(chart.columns[1]?.bars.find((bar) => bar.series === 'output')?.value).toBe(0)
    expect(chart.columns[1]?.bars.find((bar) => bar.series === 'output')?.heightPercent).toBe(0)
    // The scale is the tallest drawn bar, so input's 2,000 is the ceiling.
    expect(chart.max).toBe(2000)
    expect(chart.columns[1]?.bars.find((bar) => bar.series === 'input')?.heightPercent).toBe(100)
    expect(chart.columns[0]?.bars[0]?.heightPercent).toBe(25)
  })

  it('ticks each chart at half and max of its own scale', () => {
    const days = [
      row({
        id: '2026-10-30',
        label: '2026-10-30',
        records: 4,
        cost: 2,
        tokens: { input: 3000, output: 1000, reasoning: 500, cacheRead: 8000, cacheWrite: 2000 },
      }),
    ]

    const tokenChart = usageChart(days, 'tokens')
    // Grouped bars scale to the tallest single piece: input's 3,000.
    expect(tokenChart.max).toBe(3000)
    expect(tokenChart.ticks.map((tick) => tick.label)).toEqual(['1,500', '3,000'])
    expect(tokenChart.ticks.map((tick) => tick.heightPercent)).toEqual([50, 100])

    const cacheChart = usageChart(days, 'cache')
    // Grouped bars scale to the taller of cache read and cache write: 8,000.
    expect(cacheChart.max).toBe(8000)
    expect(cacheChart.ticks.map((tick) => tick.label)).toEqual(['4,000', '8,000'])

    const totalChart = usageChart(days, 'total')
    expect(totalChart.max).toBe(14_500)
    expect(totalChart.ticks.map((tick) => tick.label)).toEqual(['7,250', '14,500'])

    const costChart = usageChart(days, 'cost')
    expect(costChart.max).toBe(2)
    expect(costChart.ticks.map((tick) => tick.label)).toEqual(['$1.00', '$2.00'])

    // A chart whose days report nothing on its scale draws no tick.
    const emptyDays = [
      row({
        id: '2026-10-28',
        label: '2026-10-28',
        reported: 0,
        cost: 0,
        records: 0,
        tokens: { input: null, output: null, reasoning: null, cacheRead: null, cacheWrite: null },
      }),
    ]
    for (const kind of ['tokens', 'cache', 'total', 'cost'] as const) {
      const empty = usageChart(emptyDays, kind)
      expect(empty.max).toBe(0)
      expect(empty.ticks).toEqual([])
    }
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

  it('reads each charted series from the day row, absent never zero', () => {
    const entry = row({
      cost: 1.5,
      tokens: { input: 3000, output: null, reasoning: 100, cacheRead: 200, cacheWrite: null },
    })
    // Total tokens still sums every reported piece, reasoning included.
    expect(totalTokens(entry)).toBe(3300)
    expect(seriesValue(entry, 'totalTokens')).toBe(3300)
    expect(seriesValue(entry, 'input')).toBe(3000)
    expect(seriesValue(entry, 'output')).toBeNull()
    expect(seriesValue(entry, 'cacheRead')).toBe(200)
    expect(seriesValue(entry, 'cacheWrite')).toBeNull()
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

  it('draws each chart against its own scale', () => {
    const days = [
      row({
        id: '2026-10-31',
        label: '2026-10-31',
        cost: 2,
        tokens: { input: 6000, output: 0, reasoning: 0, cacheRead: 900_000, cacheWrite: 0 },
      }),
      row({
        id: '2026-10-30',
        label: '2026-10-30',
        cost: 1,
        tokens: { input: 3000, output: 0, reasoning: 0, cacheRead: 300_000, cacheWrite: 0 },
      }),
    ]
    const tokenChart = usageChart(days, 'tokens')
    expect(tokenChart.max).toBe(6000)
    expect(tokenChart.columns.map((column) => column.bars[0]?.heightPercent)).toEqual([50, 100])
    const cacheChart = usageChart(days, 'cache')
    expect(cacheChart.max).toBe(900_000)
    expect(cacheChart.columns[0]?.bars[0]?.heightPercent).toBeCloseTo(100 / 3)
    expect(cacheChart.columns[1]?.bars[0]?.heightPercent).toBe(100)
    const totalChart = usageChart(days, 'total')
    expect(totalChart.max).toBe(906_000)
    expect(totalChart.columns[0]?.bars[0]?.heightPercent).toBeCloseTo((303_000 / 906_000) * 100)
    expect(totalChart.columns[1]?.bars[0]?.heightPercent).toBe(100)
    const costChart = usageChart(days, 'cost')
    expect(costChart.max).toBe(2)
    expect(costChart.columns.map((column) => column.bars[0]?.heightPercent)).toEqual([50, 100])
  })

  it('keeps zero and absent pieces in their slots', () => {
    const days = [
      row({
        id: '2026-10-31',
        label: '2026-10-31',
        tokens: { input: 2000, output: 0, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
      }),
      row({
        id: '2026-10-30',
        label: '2026-10-30',
        reported: 0,
        tokens: { input: null, output: null, reasoning: null, cacheRead: null, cacheWrite: null },
      }),
      row({
        id: '2026-10-29',
        label: '2026-10-29',
        tokens: { input: 0, output: 0, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
      }),
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
