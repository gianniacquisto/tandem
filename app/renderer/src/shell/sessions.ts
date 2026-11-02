import type { Session } from '../wire/Session'

/**
 * The ticket sheet's Sessions section, in pure functions: the kept link, the
 * deep-linked row, the record header, and the long-output fold. The component
 * (`faces/SessionsSection`) draws these; the tests pin them here, where Node
 * runs without a window.
 */

/** A tool output past this many lines renders collapsed, behind an expand control. */
export const LONG_OUTPUT_LINES = 50

/**
 * The kept link that reopens the sheet on the pass it names: the ticket's
 * subject plus the Tandem record id, so a message to somebody else carries the
 * row, not only the ticket.
 */
export function sessionHref(ticketId: string, sessionId: string): string {
  return `#/tickets?ticket=${encodeURIComponent(ticketId)}&session=${encodeURIComponent(sessionId)}`
}

/**
 * The sheet's own hash with the expanded row swapped: the tab and the layout
 * that carried the sheet stay, and only the session query moves. Expanding
 * names the row; collapsing drops it, leaving the ticket's subject behind.
 */
export function sheetSessionHref(
  currentHash: string,
  ticketId: string,
  sessionId: string | null,
): string {
  const withoutHash = currentHash.replace(/^#\/?/, '')
  const [path = ''] = withoutHash.split('?')
  const ticket = encodeURIComponent(ticketId)
  const query =
    sessionId === null
      ? `ticket=${ticket}`
      : `ticket=${ticket}&session=${encodeURIComponent(sessionId)}`
  return `#/${path === '' ? 'tickets' : path}?${query}`
}

/**
 * Which row the sheet opens expanded. A sid the ticket links expands that row;
 * no sid expands nothing, and a sid the ticket does not link is ignored with a
 * notice, never a throw and never an empty expansion.
 */
export function resolveInitialExpanded(
  linked: string[],
  sid: string | null,
): { expanded: string | null; notice: string | null } {
  if (sid === null) return { expanded: null, notice: null }
  if (linked.includes(sid)) return { expanded: sid, notice: null }
  return { expanded: null, notice: `No session ${sid} is linked to this ticket.` }
}

/** Why one row's transcript is missing, on the row, without breaking the section. */
export function unavailableNotice(reason: string): string {
  return `transcript unavailable: ${reason}`
}

/** The lines a text carries: one for empty, otherwise one per newline plus the last. */
export function lineCount(text: string): number {
  if (text === '') return 0
  return text.split('\n').length
}

/** Whether the output folds: past fifty lines, the control expands it. */
export function isLongOutput(text: string): boolean {
  return lineCount(text) > LONG_OUTPUT_LINES
}

/** The folded output: its first fifty lines, unfolded by the expand control. */
export function outputPreview(text: string): string {
  return text.split('\n').slice(0, LONG_OUTPUT_LINES).join('\n')
}

/**
 * The record header's usage half: the model, the cost, and the token counts the
 * record stamped, or null when the harness reported nothing at all (the usage
 * read's own rule: absent is absent, never a zero dressed up as a count).
 */
export function usageLine(record: Session): string | null {
  const parts: string[] = []
  if (record.model !== null && record.model.trim() !== '') parts.push(record.model)
  if (record.cost !== null) parts.push(`$${record.cost.toFixed(2)}`)
  const input = record.tokens?.input ?? null
  const output = record.tokens?.output ?? null
  if (input !== null || output !== null) {
    parts.push(`${input ?? 0} in / ${output ?? 0} out`)
  }
  return parts.length > 0 ? parts.join(' · ') : null
}
