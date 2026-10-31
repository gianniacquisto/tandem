import { describe, expect, it } from 'vitest'

import {
  editorCall,
  editorDocument,
  parseEditorMessage,
} from '../renderer/src/shell/editor'

describe('the editor page bridge', () => {
  it('reads a dirty message with the whole markdown document', () => {
    const message = parseEditorMessage(
      JSON.stringify({ type: 'dirty', text: '# Note\n\n- [ ] one' }),
    )
    expect(message).toEqual({ type: 'dirty', text: '# Note\n\n- [ ] one' })
  })

  it('reads a link click as the raw target', () => {
    expect(
      parseEditorMessage(JSON.stringify({ type: 'link-opened', target: 'projects/fixture' })),
    ).toEqual({ type: 'link-opened', target: 'projects/fixture' })
  })

  it('reads a checkbox toggle and the ready message', () => {
    expect(
      parseEditorMessage(JSON.stringify({ type: 'checkbox-toggled', line: 3, checked: true })),
    ).toEqual({ type: 'checkbox-toggled', line: 3, checked: true })
    expect(parseEditorMessage(JSON.stringify({ type: 'loaded' }))).toEqual({ type: 'loaded' })
  })

  it('ignores a message that is not JSON, not an object, or its own echoes', () => {
    expect(parseEditorMessage('not json')).toBeNull()
    expect(parseEditorMessage(JSON.stringify({ type: 'tandem-editor-call' }))).toBeNull()
    expect(parseEditorMessage(JSON.stringify({ type: 'dirty' }))).toBeNull()
    expect(parseEditorMessage(JSON.stringify(null))).toBeNull()
  })
})

describe('the page globals the bundle expects', () => {
  it('replaces the document with its markdown and note path', () => {
    expect(editorDocument('Index.md', '# Index')).toEqual({
      type: 'tandem-editor-call',
      name: 'setDocument',
      args: ['# Index', 'Index.md'],
    })
  })

  it('hands the note index as the flat path list the committed bundle reads', () => {
    expect(editorCall('setNoteIndex', [['Index.md', 'daily/2026-10-30.md']])).toEqual({
      type: 'tandem-editor-call',
      name: 'setNoteIndex',
      args: [['Index.md', 'daily/2026-10-30.md']],
    })
  })

  it('focuses the editor and asks for a ticket candidate with no arguments', () => {
    expect(editorCall('focusEditor')).toEqual({
      type: 'tandem-editor-call',
      name: 'focusEditor',
      args: [],
    })
    expect(editorCall('requestTicketCandidate')).toEqual({
      type: 'tandem-editor-call',
      name: 'requestTicketCandidate',
      args: [],
    })
  })
})
