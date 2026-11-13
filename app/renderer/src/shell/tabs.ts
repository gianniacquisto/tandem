/** The two tabs the port keeps (`docs/port.md`, What is kept). */
export const TABS = [
  { id: 'tickets', label: 'Tickets' },
  { id: 'bikeshed', label: 'Bikeshed' },
] as const

export type TabId = (typeof TABS)[number]['id']
