import type { TicketSummary } from '../wire/TicketSummary'

/**
 * The workflow DAG's pure half: which pass a batch ticket sits at, and where
 * the nodes sit. The face draws this; nothing here reads the store, and the
 * mapping keys off the status strings the wire answered, never a derivation of
 * what the sprint means.
 *
 * `refine` shapes backlog into ready; `build` claims in-progress; `review`
 * gates in-review; `retro` holds parked work the close must account for (what
 * slipped and what waits); `done` is landed work. A status the board does not
 * spell parks at the front of the pipe, so no batch ticket falls off the graph.
 */

/** One pass of the sprint loop, a node of the graph. */
export type PassId = 'refine' | 'build' | 'review' | 'retro' | 'done'

export type Pass = {
  id: PassId
  label: string
}

/** The sprint loop in order: the nodes, and the handoffs between them. */
export const PASSES: readonly Pass[] = [
  { id: 'refine', label: 'Refine' },
  { id: 'build', label: 'Build' },
  { id: 'review', label: 'Review' },
  { id: 'retro', label: 'Retro' },
  { id: 'done', label: 'Done' },
]

/** The node a batch ticket sits at, by the status the listing answered. */
export function nodeForStatus(status: string): PassId {
  switch (status) {
    case 'backlog':
    case 'ready':
      return 'refine'
    case 'in-progress':
      return 'build'
    case 'in-review':
      return 'review'
    case 'on-hold':
      return 'retro'
    case 'done':
      return 'done'
    default:
      return 'refine'
  }
}

/** One batch ticket on the graph: the summary when the board still holds it. */
export type BatchPlacement = {
  id: string
  ticket: TicketSummary | null
  pass: PassId
}

/**
 * The sprint's batch in stored order, each id placed at its node's pass. An id
 * the board no longer holds keeps its slot with no summary (ADR 0032: it reads
 * "not on the board", never an error), so the face names it instead of
 * dropping it silently.
 */
export function positionBatch(
  batch: readonly string[],
  tickets: readonly TicketSummary[],
): BatchPlacement[] {
  return batch.map((id) => {
    const ticket = tickets.find((row) => row.id === id) ?? null
    return { id, ticket, pass: nodeForStatus(ticket?.status ?? '') }
  })
}

/** The drawn tallies: the batch count at each node, every node named. */
export function countsByPass(placed: readonly BatchPlacement[]): Record<PassId, number> {
  const counts: Record<PassId, number> = { refine: 0, build: 0, review: 0, retro: 0, done: 0 }
  for (const one of placed) counts[one.pass] += 1
  return counts
}

/** The handoffs: each pass to the next, in loop order. */
export function edgeHandoffs(): Array<[PassId, PassId]> {
  const handoffs: Array<[PassId, PassId]> = []
  for (let index = 1; index < PASSES.length; index++) {
    const from = PASSES[index - 1]?.id
    const to = PASSES[index]?.id
    if (from !== undefined && to !== undefined) handoffs.push([from, to])
  }
  return handoffs
}

/** One node on the canvas: a box in a single row, laid left to right. */
export type NodeFrame = {
  x: number
  y: number
  width: number
  height: number
}

export const NODE_WIDTH = 104
export const NODE_HEIGHT = 56
export const NODE_GAP = 28
export const NODE_TOP = 16

export const DAG_WIDTH = PASSES.length * NODE_WIDTH + (PASSES.length - 1) * NODE_GAP
export const DAG_HEIGHT = NODE_TOP + NODE_HEIGHT + 24

/** The frame of the node at an index in loop order. */
export function nodeFrame(index: number): NodeFrame {
  return { x: index * (NODE_WIDTH + NODE_GAP), y: NODE_TOP, width: NODE_WIDTH, height: NODE_HEIGHT }
}

/** The handoff's connector: the right edge's middle to the next left edge's. */
export function edgePath(from: NodeFrame, to: NodeFrame): string {
  const x1 = from.x + from.width
  const y = from.y + from.height / 2
  const x2 = to.x
  const y2 = to.y + to.height / 2
  return `M ${x1} ${y} L ${x2} ${y2}`
}
