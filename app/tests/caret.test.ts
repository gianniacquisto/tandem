import { describe, expect, it } from 'vitest'

import { parseDocument, type Document } from '../renderer/src/shell/blocks'
import { blockPoint, documentOffset } from '../renderer/src/shell/caret'
import { fromDocument, positionAtSource, sourceOffset } from '../renderer/src/shell/editor-ops'

const NOTE = '# Title\n\nFirst paragraph.\n\n- one\n- two\n'

/** Each block part's start, end and index, read from the string, not the code. */
function starts(document: Document) {
  let at = 0
  let index = 0
  const out: { start: number; end: number; index: number }[] = []
  for (const part of document.parts) {
    if (part.type === 'gap') {
      at += part.source.length
      continue
    }
    out.push({ start: at, end: at + part.block.source.length, index })
    at += part.block.source.length
    index++
  }
  return out
}

describe('a document offset as a block point', () => {
  const document = parseDocument(NOTE)
  const blocks = starts(document)

  it('a write caret lands at the same offset after an untouched round trip', () => {
    for (const offset of [0, 3, 7, 9, 15, NOTE.indexOf('one'), NOTE.length - 1]) {
      const point = blockPoint(document, offset)
      expect(point).not.toBeNull()
      expect(documentOffset(document, point!)).toBe(offset)
    }
  })

  it('an offset inside a block part yields that block and its block-relative offset', () => {
    const paragraph = blocks[1]!
    expect(blockPoint(document, NOTE.indexOf('First'))).toEqual({
      block: paragraph.index,
      offset: NOTE.indexOf('First') - paragraph.start,
    })
  })

  it('an offset in a gap lands at the next block part start', () => {
    // The blank line between the heading and the paragraph: its interior.
    const gap = NOTE.indexOf('\n\n') + 1
    expect(gap).toBeLessThan(blocks[1]!.start)
    expect(blockPoint(document, gap)).toEqual({ block: blocks[1]!.index, offset: 0 })
  })

  it('an offset past the last block lands at the last block end', () => {
    const last = blocks[blocks.length - 1]!
    expect(blockPoint(document, NOTE.length + 50)).toEqual({
      block: last.index,
      offset: last.end - last.start,
    })
  })

  it('an offset before the first block lands at the first block start', () => {
    const leading = parseDocument('\n\n# Title\n')
    const first = starts(leading)[0]!
    expect(first.start).toBeGreaterThan(0)
    expect(blockPoint(leading, 0)).toEqual({ block: first.index, offset: 0 })
  })

  it('an empty document yields no selection', () => {
    expect(blockPoint(parseDocument(''), 0)).toBeNull()
  })

  it('the reverse direction returns the part start plus the block-relative offset', () => {
    const paragraph = blocks[1]!
    expect(documentOffset(document, { block: paragraph.index, offset: 3 })).toBe(
      paragraph.start + 3,
    )
  })
})

describe('the block surface caret clamp', () => {
  it('clamps scaffolding to the nearest editable position', () => {
    // A heading's source is `# Title`; its hashes are not editable, so both a
    // zero offset and an offset inside the marker land at the text start.
    const state = fromDocument(parseDocument('# Title\n'))
    const heading = state.blocks[0]!
    expect(positionAtSource(heading, 0, 0)).toEqual({ block: 0, row: 0, offset: 0 })
    expect(positionAtSource(heading, 0, 2)).toEqual({ block: 0, row: 0, offset: 0 })
    expect(positionAtSource(heading, 0, 9)).toEqual({ block: 0, row: 0, offset: 5 })
  })

  it('maps a text caret back to its offset in the block source', () => {
    const state = fromDocument(parseDocument('# Title\n'))
    const heading = state.blocks[0]!
    expect(sourceOffset(heading, { block: 0, row: 0, offset: 5 })).toBe(7)
  })
})
