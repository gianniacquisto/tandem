import { describe, expect, it } from 'vitest'

import {
  documentBlocks,
  editBlock,
  parseDocument,
  serialiseDocument,
} from '../renderer/src/shell/blocks'
import { resolveWikilink } from '../renderer/src/shell/bikeshed'
import {
  filterNotes,
  parseInline,
  parseWikilink,
  scanInlineDocument,
  scanMath,
  scanWikilinks,
  serialiseInline,
  type WikilinkRun,
} from '../renderer/src/shell/inline'

const serialise = (source: string): string => serialiseInline(parseInline(source))
const notes = ['Index.md', 'projects/fixture/fixture.md', 'daily/Index.md']

describe('the inline round trip', () => {
  it('round trips each construct byte for byte when it is already canonical', () => {
    const cases = [
      '**bold**',
      '*italic*',
      '~~strike~~',
      '`code`',
      '**bold and *italic* inside**',
      '\\*not italic\\*',
      '[label](href.md)',
      '[[note]]',
      '[[folder/note]]',
      '[[note#Heading]]',
      '[[note|Alias]]',
      '[[note#Heading|Alias]]',
      '$x^2$',
    ]
    for (const source of cases) expect(serialise(source), source).toBe(source)
  })

  it('serialises an edited block with canonical delimiters', () => {
    expect(serialise('_italic_')).toBe('*italic*')
    expect(serialise('__bold__')).toBe('**bold**')
    expect(serialise('~~strike~~')).toBe('~~strike~~')
    expect(serialise('``code``')).toBe('`code`')
    expect(serialise('[label](href.md)')).toBe('[label](href.md)')
  })

  it('is a fixed point: serialise(parse(serialise(parse(source)))) is stable', () => {
    for (const source of ['_italic_', '__bold__', '**bold and *italic* inside**', '[[note|Alias]]']) {
      const once = serialise(source)
      expect(serialise(once), source).toBe(once)
    }
  })

  it('keeps `**` inside inline code literal', () => {
    const runs = parseInline('`**x**`')
    expect(runs).toHaveLength(1)
    expect(runs[0]).toMatchObject({ kind: 'text', text: '**x**', styles: ['code'] })
    expect(serialise('`**x**`')).toBe('`**x**`')
  })

  it('keeps `$` inside a wikilink target literal', () => {
    const runs = parseInline('[[a $b$ c]]')
    expect(runs).toHaveLength(1)
    expect(runs[0]).toMatchObject({
      kind: 'wikilink',
      target: 'a $b$ c',
      display: 'a $b$ c',
    })
    expect(serialise('[[a $b$ c]]')).toBe('[[a $b$ c]]')
  })

  it('carries a style set, so bold plus inline code is one run with both', () => {
    const runs = parseInline('**bold with `code`**')
    const code = runs.find((run) => run.kind === 'text' && run.styles.includes('code'))
    expect(code).toMatchObject({ kind: 'text', text: 'code', styles: ['bold', 'code'] })
    expect(serialise('**bold with `code`**')).toBe('**bold with `code`**')
  })

  it('carries a link run with its label and href', () => {
    expect(parseInline('[Notes](folder/Notes.md)')).toMatchObject([
      { kind: 'link', text: 'Notes', href: 'folder/Notes.md', styles: [] },
    ])
  })
})

describe('the wikilink run', () => {
  it('carries target, fragment, alias and display and resolves against the note index', () => {
    const [run] = parseInline('[[note#Heading|Alias]]', notes) as WikilinkRun[]
    expect(run).toMatchObject({
      kind: 'wikilink',
      target: 'note',
      fragment: 'Heading',
      alias: 'Alias',
      display: 'Alias',
      path: null,
      broken: true,
    })
  })

  it('resolves the exactly-one rule, bare and qualified, case-insensitively', () => {
    const [bare] = parseInline('[[index]]', notes) as WikilinkRun[]
    expect(bare).toMatchObject({ path: null, broken: true })
    const [qualified] = parseInline('[[daily/index]]', notes) as WikilinkRun[]
    expect(qualified).toMatchObject({ path: 'daily/Index.md', broken: false })
  })
})

describe('the math run', () => {
  it('carries the source between its delimiters', () => {
    expect(parseInline('a $x + y$ b')[1]).toMatchObject({
      kind: 'math',
      display: false,
      content: 'x + y',
    })
  })

  it('draws a display-math block from the block model and round trips it', () => {
    const document = parseDocument('$$\nx = 1\n$$')
    const block = documentBlocks(document)[0]
    expect(block?.kind).toBe('math')
    if (block?.kind !== 'math') throw new Error('expected a math block')
    expect(block.text).toBe('x = 1')
    expect(serialiseDocument(document)).toBe('$$\nx = 1\n$$')
    expect(serialiseDocument(editBlock(document, 0, block))).toBe('$$\nx = 1\n$$')
  })

  it('canonicalises a single-line display block when it is edited', () => {
    const document = parseDocument('$$e = mc^2$$')
    const block = documentBlocks(document)[0]
    if (block?.kind !== 'math') throw new Error('expected a math block')
    expect(block.text).toBe('e = mc^2')
    expect(serialiseDocument(editBlock(document, 0, block))).toBe('$$\ne = mc^2\n$$')
  })
})

describe('a draw never mutates the block model', () => {
  it('leaves an untouched block byte-identical after a parse pass', () => {
    const source = 'A **bold** [[Index]] $x$ paragraph.'
    const document = parseDocument(source)
    const block = documentBlocks(document)[0]
    if (block === undefined) throw new Error('expected a block')
    const before = block.source
    parseInline(block.source, notes)
    expect(block.source).toBe(before)
    expect(serialiseDocument(document)).toBe(source)
  })
})

describe('the ported fixtures: wikilink grammar', () => {
  it('splits target, fragment and alias exactly as Editor/test/grammar.test.js pins', () => {
    expect(parseWikilink('note')).toEqual({
      kind: 'link',
      target: 'note',
      display: 'note',
    })
    expect(parseWikilink('note|alias')).toMatchObject({
      target: 'note',
      alias: 'alias',
      display: 'alias',
    })
    expect(parseWikilink('note#Heading')).toMatchObject({
      target: 'note',
      fragment: 'Heading',
      display: 'Heading',
    })
    expect(parseWikilink('note#Heading|alias')).toMatchObject({
      target: 'note',
      fragment: 'Heading',
      alias: 'alias',
      display: 'alias',
    })
    expect(parseWikilink('a#b#c|d')).toMatchObject({ target: 'a', fragment: 'b#c', alias: 'd' })
    expect(parseWikilink('a|b|c')).toMatchObject({ target: 'a', alias: 'b|c' })
    expect(parseWikilink('a|b#c')).toMatchObject({ target: 'a', alias: 'b', fragment: 'c' })
    expect(parseWikilink('a#b|c#d')).toMatchObject({ target: 'a', fragment: 'b', alias: 'c#d' })
    expect(parseWikilink('a|')).toMatchObject({ target: 'a', display: 'a' })
    expect(parseWikilink('a|')).not.toHaveProperty('alias')
    expect(parseWikilink('a#')).toMatchObject({ target: 'a', display: 'a' })
    expect(parseWikilink('a#')).not.toHaveProperty('fragment')
    expect(parseWikilink('a#|b')).toMatchObject({ target: 'a', alias: 'b', display: 'b' })
  })

  it('keeps the not-link cases literal', () => {
    expect(parseWikilink('').kind).toBe('not-link')
    expect(parseWikilink('   ').kind).toBe('not-link')
    expect(parseWikilink('|alias').kind).toBe('not-link')
    expect(parseWikilink('#Heading')).toMatchObject({ kind: 'not-link', reason: 'same-note' })
    expect(parseWikilink('#Heading|alias').kind).toBe('not-link')
    expect(parseWikilink('#').kind).toBe('not-link')
  })
})

describe('the ported fixtures: the wikilink scanner', () => {
  it('finds a link with exact offsets and the first closer', () => {
    const text = 'see [[note]] for more'
    const [link] = scanWikilinks(text)
    expect(link).toMatchObject({ from: 4, to: 12, content: 'note', target: 'note' })
    expect(text.slice(link!.from, link!.to)).toBe('[[note]]')
    expect(scanWikilinks('[[a]]b]]')[0]?.to).toBe(5)
    expect(scanWikilinks('[[a[[b]]')[0]?.target).toBe('a[[b')
    const multiple = scanWikilinks('[[a]] mid [[b|B]] end')
    expect(multiple.map((link) => link.target)).toEqual(['a', 'b'])
    expect(multiple[1]?.display).toBe('B')
  })

  it('keeps unclosed, embedded and later-line closers literal', () => {
    expect(scanWikilinks('open [[note\nclosed [[ok]]')).toHaveLength(1)
    expect(scanWikilinks('dangling [[note')).toHaveLength(0)
    expect(scanWikilinks('[[note\n]]')).toHaveLength(0)
    expect(scanWikilinks('![[img]]')).toHaveLength(0)
    expect(scanWikilinks('![[img]] and [[note]]')).toHaveLength(1)
  })

  it('masks fenced code, inline code and frontmatter', () => {
    expect(scanWikilinks('before\n```\n[[note]]\n```\nafter [[real]]')).toHaveLength(1)
    expect(scanWikilinks('~~~\n[[note]]\n~~~\n[[real]]')).toHaveLength(1)
    expect(scanWikilinks('````\n```\n[[note]]\n````\n[[real]]')).toHaveLength(1)
    expect(scanWikilinks('use `[[note]]` carefully and [[real]] too')).toHaveLength(1)
    expect(scanWikilinks('``a [[x]] b`` and [[real]]')).toHaveLength(1)
    expect(scanWikilinks('half `open and [[real]]')).toHaveLength(1)
    expect(scanWikilinks('---\ntags: [[x]]\n---\nbody [[real]]')).toHaveLength(1)
    expect(scanWikilinks('---\n[[real]]')).toHaveLength(1)
  })

  it('keeps the same-note form literal but finds the next link', () => {
    const found = scanWikilinks('[[#Heading]] then [[note]]')
    expect(found).toHaveLength(1)
    expect(found[0]?.target).toBe('note')
  })
})

describe('the ported fixtures: the math scanner', () => {
  it('finds inline math with the settled trigger rules', () => {
    const [inline] = scanMath('a $x + y$ b')
    expect(inline).toMatchObject({ kind: 'inline', content: 'x + y', from: 2, to: 9 })
    expect(scanMath('it costs $5 today')).toHaveLength(0)
    expect(scanMath('costs $5 and $10 total')).toHaveLength(0)
    expect(scanMath('a $ x$ b')).toHaveLength(0)
    expect(scanMath('a $x $ b')).toHaveLength(0)
    expect(scanMath('a $$ b')).toHaveLength(0)
    expect(scanMath('a $ $ b')).toHaveLength(0)
    expect(scanMath('a $x\ny$ b')).toHaveLength(0)
  })

  it('finds display math on its own lines only', () => {
    expect(scanMath('$$e = mc^2$$')[0]).toMatchObject({ kind: 'display', content: 'e = mc^2' })
    const text = 'before\n$$\nx = 1\n$$\nafter'
    const [display] = scanMath(text)
    expect(display).toMatchObject({ kind: 'display', content: 'x = 1' })
    expect(text.slice(display!.from, display!.to)).toBe('$$\nx = 1\n$$')
    expect(scanMath('$$\nx = 1\n')).toHaveLength(0)
    expect(scanMath('money $$ stuff\n$$\n')).toHaveLength(0)
    expect(scanMath('$$\nx = 1 $$\n')).toHaveLength(0)
    expect(scanMath('text $$z$$ more')).toHaveLength(0)
  })

  it('keeps three-dollar runs and code literals out of math', () => {
    expect(scanMath('$$$x$$$')).toHaveLength(0)
    expect(scanMath('$x$ inside $$$')[0]).toMatchObject({ content: 'x', from: 0 })
    expect(scanMath('```\n$x$\n```\nreal $y$')).toHaveLength(1)
    expect(scanMath('`$x$` but $y$')).toHaveLength(1)
    expect(scanMath('`$`x$')).toHaveLength(0)
    expect(scanMath('$a$ mid $b$')).toHaveLength(2)
    expect(scanMath('$ $a$')[0]).toMatchObject({ content: 'a', from: 2, to: 5 })
  })

  it('honours caller masks', () => {
    const masked = scanMath('[[a $b$ c]] and $d$', [{ from: 0, to: 13 }])
    expect(masked).toHaveLength(1)
    expect(masked[0]?.content).toBe('d')
  })
})

describe('the ported fixtures: resolution', () => {
  it('matches the renderer resolver the pill uses', () => {
    expect(resolveWikilink('my note', ['Notes/My note.md'])).toBe('Notes/My note.md')
    expect(resolveWikilink('ghost', ['a.md'])).toBeNull()
    expect(resolveWikilink('a', ['x/a.md', 'y/a.md'])).toBeNull()
    expect(resolveWikilink('a', ['a.md', 'A.md'])).toBeNull()
    expect(resolveWikilink('FOLDER/NOTE', ['folder/note.md'])).toBe('folder/note.md')
    expect(resolveWikilink('folder/note', ['other/note.md'])).toBeNull()
    expect(resolveWikilink('a', ['A.MD'])).toBe('A.MD')
    expect(resolveWikilink('a.b', ['a.b.md'])).toBe('a.b.md')
    expect(resolveWikilink('a', ['a.b.md'])).toBeNull()
    expect(resolveWikilink('a.md', ['a.md'])).toBe('a.md')
  })
})

describe('the ported fixtures: the fixed point', () => {
  it('keeps math in a wikilink target out of math', () => {
    const found = scanInlineDocument('[[a $b$ c]]')
    expect(found.wikilinks).toHaveLength(1)
    expect(found.wikilinks[0]?.target).toBe('a $b$ c')
    expect(found.math).toHaveLength(0)
  })

  it('keeps a wikilink inside display math out of links', () => {
    for (const text of ['$$\n[[x]]\n$$', '$$[[x]]$$']) {
      const found = scanInlineDocument(text)
      expect(found.wikilinks).toHaveLength(0)
      expect(found.math).toHaveLength(1)
    }
  })

  it('lets both kinds coexist when disjoint', () => {
    const found = scanInlineDocument('[[x]] and $y$\n$$z$$')
    expect(found.wikilinks).toHaveLength(1)
    expect(found.math.map((math) => math.kind)).toEqual(['inline', 'display'])
  })

  it('reports frontmatter without scanning inside it', () => {
    const found = scanInlineDocument('---\nt: [[x]]\n---\n[[real]]')
    expect(found.frontmatter?.to).toBeGreaterThan(0)
    expect(found.wikilinks).toHaveLength(1)
    expect(found.wikilinks[0]?.target).toBe('real')
  })
})

describe("the picker's filter", () => {
  it('matches the basename and the root-relative path, case-insensitively', () => {
    expect(filterNotes(notes, 'IND').map((choice) => choice.path)).toEqual([
      'daily/Index.md',
      'Index.md',
    ])
    expect(filterNotes(notes, 'fixture').map((choice) => choice.path)).toEqual([
      'projects/fixture/fixture.md',
    ])
    expect(filterNotes(notes, 'projects/fix').map((choice) => choice.path)).toEqual([
      'projects/fixture/fixture.md',
    ])
  })

  it('inserts a bare basename when it is unique and a qualified path when it is duplicated', () => {
    const found = filterNotes(notes, 'index')
    expect(found.find((choice) => choice.path === 'Index.md')?.insert).toBe('Index')
    expect(found.find((choice) => choice.path === 'daily/Index.md')?.insert).toBe('daily/Index')
    expect(filterNotes(notes, 'fixture')[0]?.insert).toBe('fixture')
  })

  it('returns an empty list for no match and never invents a note', () => {
    expect(filterNotes(notes, 'zzz')).toEqual([])
    for (const choice of filterNotes(notes, '')) expect(notes).toContain(choice.path)
  })
})
