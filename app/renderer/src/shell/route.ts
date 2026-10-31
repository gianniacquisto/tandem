import { TABS, type TabId } from './tabs'

/**
 * The shell's routing is the hash and nothing else: the renderer holds no
 * history of its own, and a tab is a link a person can keep. The Tickets face
 * routes its two layouts the same way, so the board is a link too (and a
 * picture a spec can open).
 */
export type View = 'list' | 'board'
export type Route = { tab: TabId; view: View }

export function tabHref(tab: TabId): string {
  return `#/${tab}`
}

export function viewHref(tab: TabId, view: View): string {
  return view === 'board' ? `#/${tab}/board` : tabHref(tab)
}

export function parseRoute(hash: string): Route {
  const [first, second] = hash.replace(/^#\/?/, '').split('/')
  const match = TABS.find((tab) => tab.id === first)
  const tab = match === undefined ? 'tickets' : match.id
  const view = tab === 'tickets' && second === 'board' ? 'board' : 'list'
  return { tab, view }
}
