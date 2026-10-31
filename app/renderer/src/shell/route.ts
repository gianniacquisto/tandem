import { TABS, type TabId } from './tabs'

/**
 * The shell's routing is the hash and nothing else: the renderer holds no
 * history of its own, and a tab is a link a person can keep. The Tickets face
 * routes its two layouts the same way, so the board is a link too (and a
 * picture a spec can open).
 *
 * A ticket in the query (`#/tickets?ticket=<id>`) is the sheet's subject: the
 * note rail's ticket row is an anchor, so opening the sheet from a note must
 * survive a reload, a new window and a message to somebody else. The sheet is a
 * dialog over the face that carried it, so the tab and the layout stay.
 */
export type View = 'list' | 'board'
export type Route = { tab: TabId; view: View; ticket: string | null }

export function tabHref(tab: TabId): string {
  return `#/${tab}`
}

export function viewHref(tab: TabId, view: View): string {
  return view === 'board' ? `#/${tab}/board` : tabHref(tab)
}

export function parseRoute(hash: string): Route {
  const [path = '', query = ''] = hash.replace(/^#\/?/, '').split('?')
  const [first, second] = path.split('/')
  const match = TABS.find((tab) => tab.id === first)
  const tab = match === undefined ? 'tickets' : match.id
  const view = tab === 'tickets' && second === 'board' ? 'board' : 'list'
  return { tab, view, ticket: ticketParam(query) }
}

/**
 * The `ticket` query parameter, or null when it names nothing. A malformed
 * escape is a hash somebody typed or a stale link: it reads as no ticket, never
 * as a throw, because `parseRoute` runs in the shell's render and one bad
 * character must not blank the window.
 */
function ticketParam(query: string): string | null {
  for (const pair of query.split('&')) {
    const [name, value = ''] = pair.split('=')
    if (name !== 'ticket') continue
    const decoded = decodeURIComponentOrNull(value)?.trim() ?? ''
    return decoded === '' ? null : decoded
  }
  return null
}

function decodeURIComponentOrNull(value: string): string | null {
  try {
    return decodeURIComponent(value)
  } catch {
    return null
  }
}
