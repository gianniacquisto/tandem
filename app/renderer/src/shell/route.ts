import { TABS, type TabId } from './tabs'

/**
 * The shell's routing is the hash and nothing else: the renderer holds no
 * history of its own, and a tab is a link a person can keep. The Tickets face
 * is one view (the board), so a tab is one path and a second segment from an
 * older link is ignored rather than opening a layout the app no longer has.
 *
 * A ticket in the query (`#/tickets?ticket=<id>`) is the sheet's subject: the
 * note rail's ticket row is an anchor, so opening the sheet from a note must
 * survive a reload, a new window and a message to somebody else. The sheet is
 * a dialog over the face that carried it, so the tab stays.
 */
export type Route = { tab: TabId; ticket: string | null }

export function tabHref(tab: TabId): string {
  return `#/${tab}`
}

/**
 * Where a sheet closes to: the tab that carried it, without the ticket query.
 * A kept link's query is the sheet's, not the face's.
 */
export function closeHref(route: Route): string {
  return tabHref(route.tab)
}

export function parseRoute(hash: string): Route {
  const [path = '', query = ''] = hash.replace(/^#\/?/, '').split('?')
  const [first] = path.split('/')
  const match = TABS.find((tab) => tab.id === first)
  const tab = match === undefined ? 'tickets' : match.id
  return { tab, ticket: queryParam(query, 'ticket') }
}

/**
 * One query parameter, or null when it names nothing. A malformed escape is a
 * hash somebody typed or a stale link: it reads as no value, never as a throw,
 * because `parseRoute` runs in the shell's render and one bad character must
 * not blank the window.
 */
function queryParam(query: string, name: string): string | null {
  for (const pair of query.split('&')) {
    const [key, value = ''] = pair.split('=')
    if (key !== name) continue
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
