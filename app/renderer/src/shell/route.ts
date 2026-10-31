import { TABS, type TabId } from './tabs'

/**
 * The shell's routing is the hash and nothing else: the renderer holds no
 * history of its own, and a tab is a link a person can keep.
 */
export function tabHref(tab: TabId): string {
  return `#/${tab}`
}

export function parseRoute(hash: string): TabId {
  const id = hash.replace(/^#\/?/, '')
  const match = TABS.find((tab) => tab.id === id)
  return match === undefined ? 'tickets' : match.id
}
