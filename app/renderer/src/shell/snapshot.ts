import type { ProjectListReply } from '../wire/ProjectListReply'
import type { TicketListReply } from '../wire/TicketListReply'
import type { TicketSummary } from '../wire/TicketSummary'
import type { UsageReportView } from '../wire/UsageReportView'
import type { Workflow } from '../wire/Workflow'

/**
 * One project and its lanes, in render order: the mapping the board joins
 * onto its lane rows instead of growing the snapshot.
 */
export type SnapshotProject = {
  name: string
  lanes: string[]
}

/**
 * What the shell knows about the store: the last listing's revision, its rows,
 * the lanes they are grouped into, the usage report the Usage face draws, and
 * whether the last read landed.
 *
 * One listing is the whole store (`filter: all, archived: true`); the face
 * narrows it locally, so the stored order the drag arithmetic needs is always
 * the same array the rows came from. The revision is the listing's own; a change
 * event only marks the snapshot stale until the re-read lands.
 */
export type SnapshotState = {
  status: 'connecting' | 'ready' | 'failed'
  revision: number | null
  tickets: TicketSummary[]
  lanes: string[]
  /** Every project with its lanes, in render order: the board's grouping. */
  projects: SnapshotProject[]
  usage: UsageReportView | null
  /** The store's roster, the names both assignee surfaces offer. */
  roster: string[]
  /** The active project: the lane the faces draw, or null for every lane. */
  activeProject: string | null
  /**
   * The active workflow: the board's columns, the list's status filter and the
   * sheet's status menu all read its stages. Null until the first read lands, and
   * the faces fall back to the default six in that window.
   */
  workflow: Workflow | null
  error: string | null
  /** A conflict or refusal the human has not dismissed yet. */
  notice: string | null
  /** An event named a store this listing has not caught up to. */
  stale: boolean
}

export type SnapshotAction =
  | { type: 'loaded'; reply: TicketListReply; lanes: string[]; projects: SnapshotProject[] }
  | { type: 'usage'; report: UsageReportView }
  | { type: 'settings'; roster: string[]; activeProject: string | null }
  | { type: 'workflow'; workflow: Workflow }
  | { type: 'failed'; message: string }
  | { type: 'stale' }
  | { type: 'notice'; message: string }
  | { type: 'dismiss' }

export const initialSnapshot: SnapshotState = {
  status: 'connecting',
  revision: null,
  tickets: [],
  lanes: [],
  projects: [],
  usage: null,
  roster: [],
  activeProject: null,
  workflow: null,
  error: null,
  notice: null,
  stale: false,
}

export function snapshotReducer(state: SnapshotState, action: SnapshotAction): SnapshotState {
  if (action.type === 'loaded') {
    return {
      ...state,
      status: 'ready',
      revision: action.reply.revision,
      tickets: action.reply.tickets,
      lanes: action.lanes,
      projects: action.projects,
      error: null,
      stale: false,
    }
  }
  if (action.type === 'usage') {
    return { ...state, usage: action.report }
  }
  if (action.type === 'settings') {
    return { ...state, roster: action.roster, activeProject: action.activeProject }
  }
  if (action.type === 'workflow') {
    return { ...state, workflow: action.workflow }
  }
  if (action.type === 'stale') {
    return { ...state, stale: true }
  }
  if (action.type === 'notice') {
    return { ...state, notice: action.message }
  }
  if (action.type === 'dismiss') {
    return { ...state, notice: null }
  }

  return { ...state, status: 'failed', stale: false, error: action.message }
}
