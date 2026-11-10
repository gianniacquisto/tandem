import type { Document } from './blocks'

/**
 * The caret as one document offset, and back.
 *
 * A switch between Write and Source carries one offset into the note's text.
 * Source's textarea selection is already that offset; the block surface's
 * selection is a block index and an offset into that block's exact source, so
 * these two pure functions are the translation. They walk UI-17's parts and
 * accumulate each part's source length, which is the only offset source there
 * is: no second copy of the text and no normalisation.
 *
 * No React and no DOM: tested in Node.
 */
export type BlockPoint = {
  /** The index among the document's block parts. */
  block: number
  /** The offset into that block's exact source bytes. */
  offset: number
}

/**
 * The document offset of a block point.
 *
 * Returns null when the document has no such block (only possible for an
 * empty note). The block-relative offset is clamped into the block's source,
 * so a caller cannot point past the bytes it names.
 */
export function documentOffset(document: Document, point: BlockPoint): number | null {
  let at = 0
  let seen = 0
  for (const part of document.parts) {
    if (part.type === 'gap') {
      at += part.source.length
      continue
    }
    if (seen === point.block) {
      const length = part.block.source.length
      return at + Math.max(0, Math.min(length, point.offset))
    }
    seen++
    at += part.block.source.length
  }
  return null
}

/**
 * The block point a document offset names.
 *
 * An offset inside a block yields that block and its block-relative offset; an
 * offset in a gap lands at the next block's start; past the last block lands at
 * the last block's end; before the first lands at the first block's start; an
 * empty document has no block and yields null. An offset exactly at a block's
 * end stays on that block, so an untouched text round-trips every offset.
 */
export function blockPoint(document: Document, offset: number): BlockPoint | null {
  let at = 0
  let index = 0
  let last: { block: number; start: number; end: number } | null = null
  for (const part of document.parts) {
    if (part.type === 'gap') {
      at += part.source.length
      continue
    }
    const start = at
    const length = part.block.source.length
    const end = start + length
    if (offset <= end) {
      return { block: index, offset: Math.max(0, Math.min(length, offset - start)) }
    }
    last = { block: index, start, end }
    index++
    at = end
  }
  return last === null ? null : { block: last.block, offset: last.end - last.start }
}
