import { readFileSync, readdirSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import {
  documentBlocks,
  editBlock,
  parseDocument,
  serialiseDocument,
  type Block,
  type Document,
} from '../renderer/src/shell/blocks'

const here = path.dirname(fileURLToPath(import.meta.url))
const corpus = path.join(here, 'fixtures', 'bikeshed-corpus')
const repo = path.resolve(here, '..', '..')

type Manifest = { files: { name: string; kinds?: string[] }[] }

const manifest = JSON.parse(
  readFileSync(path.join(corpus, 'manifest.json'), 'utf8'),
) as Manifest

const corpusBytes = (name: string): Buffer => readFileSync(path.join(corpus, name))
const corpusText = (name: string): string => corpusBytes(name).toString('utf8')
const blocksOf = (name: string): Block[] => documentBlocks(parseDocument(corpusText(name)))

function only(name: string): Block {
  const blocks = blocksOf(name)
  expect(blocks).toHaveLength(1)
  return blocks[0]!
}

function opaqueSource(name: string, index = 0): string {
  const block = blocksOf(name)[index]
  if (block === undefined || block.kind !== 'opaque') {
    throw new Error(`${name}[${index}] is not an opaque block`)
  }
  return block.source
}

function listOf(name: string) {
  const block = only(name)
  if (block.kind !== 'list') throw new Error(`${name} is not a list`)
  return block
}

function quoteOf(name: string, index = 0) {
  const block = blocksOf(name)[index]
  if (block === undefined || block.kind !== 'quote') {
    throw new Error(`${name}[${index}] is not a quote`)
  }
  return block
}

/** The parts of the document that precede and follow one part, as raw source. */
function splitAt(document: Document, partIndex: number): { prefix: string; suffix: string } {
  const source = (part: Document['parts'][number]): string =>
    part.type === 'gap' ? part.source : part.block.source
  return {
    prefix: document.parts.slice(0, partIndex).map(source).join(''),
    suffix: document.parts.slice(partIndex + 1).map(source).join(''),
  }
}

function blockPartIndex(document: Document, blockIndex: number): number {
  let seen = 0
  for (let i = 0; i < document.parts.length; i++) {
    const part = document.parts[i]
    if (part === undefined || part.type !== 'block') continue
    if (seen === blockIndex) return i
    seen++
  }
  throw new Error(`no block at index ${blockIndex}`)
}

/** Parse `source`, replace its block at `index` with `change(block)` and serialise. */
function editedText(source: string, change: (block: Block) => Block, index = 0): string {
  const document = parseDocument(source)
  const block = documentBlocks(document)[index]
  if (block === undefined) throw new Error(`no block at index ${index}`)
  return serialiseDocument(editBlock(document, index, change(block)))
}

function walkMarkdown(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) walkMarkdown(full, out)
    else if (entry.name.endsWith('.md')) out.push(full)
  }
  return out
}

function repoMarkdown(): string[] {
  return [
    ...walkMarkdown(path.join(repo, 'docs', 'adr')),
    path.join(repo, 'README.md'),
    path.join(repo, 'GLOSSARY.md'),
    path.join(repo, 'AGENTS.md'),
    ...walkMarkdown(path.join(repo, 'skills')),
    ...walkMarkdown(path.join(repo, 'agents')),
  ]
}

describe('the lossless round trip', () => {
  it('a corpus note parses and serialises back to its exact bytes', () => {
    for (const entry of manifest.files) {
      const original = corpusBytes(entry.name)
      const output = Buffer.from(serialiseDocument(parseDocument(original.toString('utf8'))), 'utf8')
      expect(output.equals(original), entry.name).toBe(true)
    }
  })

  it("the repository's own markdown parses and serialises back to its exact bytes", () => {
    const files = repoMarkdown()
    expect(files.length).toBeGreaterThan(20)
    for (const file of files) {
      const original = readFileSync(file)
      const output = Buffer.from(serialiseDocument(parseDocument(original.toString('utf8'))), 'utf8')
      expect(output.equals(original), path.relative(repo, file)).toBe(true)
    }
  })

  it('every prefix and suffix of a corpus note round-trips', () => {
    for (const entry of manifest.files) {
      const text = corpusText(entry.name)
      const lines = text.match(/[^\n]*\n|[^\n]+$/g) ?? []
      for (let i = 0; i <= lines.length; i++) {
        const prefix = lines.slice(0, i).join('')
        const suffix = lines.slice(i).join('')
        expect(serialiseDocument(parseDocument(prefix)), `${entry.name} prefix ${i}`).toBe(prefix)
        expect(serialiseDocument(parseDocument(suffix)), `${entry.name} suffix ${i}`).toBe(suffix)
      }
    }
  })

  it('is total for arbitrary text', () => {
    const samples = [
      '',
      '\n\n\n',
      '   \t\n',
      'no final newline',
      'trailing spaces   ',
      '\uFEFFa byte order mark',
      'a\rb\r\nc\r',
      '[[an unclosed wikilink',
      '$$',
      '```',
      '[^x]:',
    ]
    for (const sample of samples) {
      expect(serialiseDocument(parseDocument(sample)), JSON.stringify(sample)).toBe(sample)
    }
  })
})

describe('the block model', () => {
  it('the manifest names every top-level part', () => {
    for (const entry of manifest.files) {
      if (entry.kinds === undefined) continue
      expect(blocksOf(entry.name).map((block) => block.kind), entry.name).toEqual(entry.kinds)
    }
  })

  it('each construct becomes the part the model names', () => {
    expect(only('paragraph.md')).toMatchObject({
      kind: 'paragraph',
      text: 'A plain paragraph with **bold**, *italic*, `code` and a [link](https://example.com).',
    })

    const [atxOne, atxThree] = blocksOf('heading-atx.md')
    expect(atxOne).toMatchObject({ kind: 'heading', level: 1, text: 'One' })
    expect(atxThree).toMatchObject({ kind: 'heading', level: 3, text: 'Three' })

    const [setextOne, setextTwo] = blocksOf('heading-setext.md')
    expect(setextOne).toMatchObject({ kind: 'heading', level: 1, text: 'Title' })
    expect(setextTwo).toMatchObject({ kind: 'heading', level: 2, text: 'Sub' })

    const bullet = listOf('list-bullet.md')
    expect(bullet.ordered).toBe(false)
    expect(bullet.start).toBe(1)
    expect(bullet.items).toHaveLength(2)
    expect(bullet.items[1]?.[1]).toMatchObject({ kind: 'list' })

    const ordered = listOf('list-ordered.md')
    expect(ordered.ordered).toBe(true)
    expect(ordered.start).toBe(3)
    expect(ordered.items).toHaveLength(2)
    expect(ordered.items[1]?.[1]).toMatchObject({ kind: 'list' })

    const tasks = listOf('task-list.md')
    expect(tasks.items[0]?.[0]).toMatchObject({ kind: 'task', checked: false })
    expect(tasks.items[1]?.[0]).toMatchObject({ kind: 'task', checked: true })
    const secondTask = tasks.items[1]?.[0]
    if (secondTask === undefined || secondTask.kind !== 'task') throw new Error('no second task')
    expect(secondTask.blocks[0]).toMatchObject({ kind: 'paragraph', text: 'done' })

    const firstQuote = quoteOf('quote.md')
    expect(firstQuote.blocks.map((block) => block.kind)).toEqual(['paragraph', 'quote'])

    expect(only('code-fenced.md')).toMatchObject({
      kind: 'code',
      language: 'ts',
      text: 'const answer = 42\n\nexport default answer',
    })
    expect(only('code-indented.md')).toMatchObject({
      kind: 'code',
      language: null,
      text: 'indented code\na second line',
    })
    expect(only('mermaid.md')).toMatchObject({ kind: 'code', language: 'mermaid' })

    expect(blocksOf('rule.md').map((block) => block.kind)).toEqual(['rule', 'rule', 'rule'])
    expect(only('image.md')).toMatchObject({
      kind: 'image',
      alt: 'A diagram',
      destination: 'pics/diagram.png',
      title: 'The diagram',
    })
  })

  it('frontmatter, a table, raw HTML and a footnote are one opaque part each', () => {
    expect(opaqueSource('frontmatter.md')).toBe(
      '---\ntitle: Frontmatter fixture\ntags:\n  - corpus\n  - notes\n---',
    )
    expect(opaqueSource('table.md')).toBe(
      '| Name | Count |\n| ---- | ----- |\n| one  | 1     |\n| two  | 2     |',
    )
    expect(opaqueSource('html.md', 0)).toBe('<div class="note">\n  raw HTML block\n</div>')
    expect(opaqueSource('html.md', 1)).toBe('<!-- a comment\nacross two lines -->')
    expect(opaqueSource('footnote.md')).toBe('[^1]: A footnote definition.')
  })

  it('a display-math block is its own kind with the text between the delimiters', () => {
    const math = blocksOf('math.md')[0]
    expect(math).toMatchObject({
      kind: 'math',
      display: true,
      text: 'E = mc^2',
      source: '$$\nE = mc^2\n$$',
    })
  })

  it('editing one block leaves every other part byte-identical', () => {
    const text = corpusText('kitchen-sink.md')
    const before = parseDocument(text)
    const index = documentBlocks(before).findIndex((block) => block.kind === 'paragraph')
    expect(index).toBeGreaterThanOrEqual(0)
    const target = documentBlocks(before)[index]
    if (target === undefined || target.kind !== 'paragraph') throw new Error('no paragraph')

    const after = editBlock(before, index, { ...target, text: 'A replacement sentence.' })
    const partIndex = blockPartIndex(before, index)
    expect(after.parts).toHaveLength(before.parts.length)

    after.parts.forEach((part, i) => {
      if (i === partIndex) return
      expect(part, `part ${i}`).toEqual(before.parts[i])
    })

    const output = serialiseDocument(after)
    const { prefix, suffix } = splitAt(before, partIndex)
    expect(output.startsWith(prefix)).toBe(true)
    expect(output.endsWith(suffix)).toBe(true)
    expect(output.slice(prefix.length, output.length - suffix.length)).toBe(
      'A replacement sentence.',
    )
  })

  it('an edited block renders its kind canonically', () => {
    expect(editedText('## Old\n', (block) => ({ ...block, text: 'New' }))).toBe('## New\n')
    expect(editedText('Title\n=====\n', (block) => ({ ...block, text: 'New title' }))).toBe(
      '# New title\n',
    )
    expect(editedText('* one\n* two\n', (block) => block)).toBe('- one\n- two\n')
    expect(editedText('3. three\n4. four\n', (block) => block)).toBe('3. three\n4. four\n')
    expect(editedText('* [X] done\n', (block) => block)).toBe('- [x] done\n')
    expect(editedText('> one\n> two\n', (block) => block)).toBe('> one\n> two\n')
    expect(editedText('~~~py\nx\n~~~\n', (block) => block)).toBe('```py\nx\n```\n')
    expect(editedText('    a\n    b\n', (block) => block)).toBe('```\na\nb\n```\n')
    expect(editedText('***\n', (block) => block)).toBe('---\n')
    expect(
      editedText('![old](old.png)\n', (block) => ({
        ...block,
        alt: 'new',
        destination: 'new.png',
        title: 'A title',
      })),
    ).toBe('![new](new.png "A title")\n')
    expect(editedText('one line\n', (block) => ({ ...block, text: 'another line' }))).toBe(
      'another line\n',
    )
  })

  it('an untouched block never re-renders', () => {
    const setext = parseDocument('Title\n=====\n')
    expect(documentBlocks(setext)[0]?.source).toBe('Title\n=====')
    expect(serialiseDocument(setext)).toBe('Title\n=====\n')

    const starred = parseDocument('* one\n* two\n')
    expect(documentBlocks(starred)[0]?.source).toBe('* one\n* two')
    expect(serialiseDocument(starred)).toBe('* one\n* two\n')

    const tilde = parseDocument('~~~py\nx\n~~~\n')
    expect(serialiseDocument(tilde)).toBe('~~~py\nx\n~~~\n')
  })

  it('refuses to render an edited opaque block', () => {
    const document = parseDocument('<div>raw</div>\n')
    const block = documentBlocks(document)[0]
    if (block === undefined || block.kind !== 'opaque') throw new Error('no opaque block')

    expect(() => serialiseDocument(editBlock(document, 0, block))).toThrow()
  })
})
