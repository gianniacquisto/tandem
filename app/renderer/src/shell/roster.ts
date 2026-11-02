/**
 * The roster draft the Settings face edits, beside `shell/setup`'s
 * `checkChanges`: the small list helpers, so the face holds no parsing of its
 * own. The store cleans again on the write (`domain::Settings`), so these are
 * the draft's manners, not its validation: a blank never joins, a repeat
 * never doubles, and the receipt redraws whatever the store kept.
 */

/**
 * Trim names, drop empties, dedupe preserving order: the draft's version of
 * the cleaning the write performs, for the face's own comparisons.
 */
export function cleanRoster(names: readonly string[]): string[] {
  const seen = new Set<string>()
  const cleaned: string[] = []
  for (const name of names) {
    const trimmed = name.trim()
    if (trimmed === '' || seen.has(trimmed)) continue
    seen.add(trimmed)
    cleaned.push(trimmed)
  }
  return cleaned
}

/**
 * One more name on the draft, trimmed: a blank or a repeat leaves the draft
 * as it was, so an Add press with nothing typed is a no-op and never an
 * empty row.
 */
export function addRosterName(roster: readonly string[], name: string): string[] {
  const trimmed = name.trim()
  if (trimmed === '') return [...roster]
  if (roster.some((entry) => entry.trim() === trimmed)) return [...roster]
  return [...roster, trimmed]
}

/** The draft without the row at `index`; an index past either end is a no-op. */
export function removeRosterName(roster: readonly string[], index: number): string[] {
  if (index < 0 || index >= roster.length) return [...roster]
  return roster.filter((_, at) => at !== index)
}
