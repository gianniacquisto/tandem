import { describe, expect, it } from 'vitest'

import { assetUrl, resolveImageTarget } from '../renderer/src/shell/image'

// The classifier is the editor bundle's (`Editor/src/pure/image.js`) with its
// rules unchanged, plus the host-path cases the core also refuses. The encoder
// is the block surface's: one segment at a time, so `#`, `?`, `%` and a space
// survive the URL and a `/` stays the path separator.

describe('the image target classifier', () => {
  it('resolves a bare relative path against the note folder', () => {
    expect(resolveImageTarget('img.png', 'n.md')).toEqual({
      kind: 'bikeshed',
      path: 'img.png',
    })
    expect(resolveImageTarget('sub/img.png', 'a/n.md')).toEqual({
      kind: 'bikeshed',
      path: 'a/sub/img.png',
    })
  })

  it('climbs within the bikeshed with ../ and treats ./ as the identity', () => {
    expect(resolveImageTarget('../x.png', 'a/n.md')).toEqual({
      kind: 'bikeshed',
      path: 'x.png',
    })
    expect(resolveImageTarget('./img.png', 'a/n.md')).toEqual({
      kind: 'bikeshed',
      path: 'a/img.png',
    })
  })

  it('treats a leading / as bikeshed-root-relative', () => {
    expect(resolveImageTarget('/abs.png', 'a/n.md')).toEqual({
      kind: 'bikeshed',
      path: 'abs.png',
    })
    expect(resolveImageTarget('/etc/passwd.png', 'a/n.md')).toEqual({
      kind: 'bikeshed',
      path: 'etc/passwd.png',
    })
  })

  it('refuses a climb above the bikeshed root', () => {
    expect(resolveImageTarget('../../x.png', 'a/n.md')).toEqual({ kind: 'escape' })
    expect(resolveImageTarget('../../..', 'a/b/n.md')).toEqual({ kind: 'escape' })
  })

  it('keeps a climb back to exactly the root inside', () => {
    expect(resolveImageTarget('../a/b/c/../../..', 'a/n.md')).toEqual({
      kind: 'bikeshed',
      path: '',
    })
  })

  it('classifies remote http(s) targets without fetching them', () => {
    expect(resolveImageTarget('http://x.example/y.png', 'n.md')).toEqual({
      kind: 'remote',
      src: 'http://x.example/y.png',
    })
    expect(resolveImageTarget('https://x.example/y.png', 'a/n.md')).toEqual({
      kind: 'remote',
      src: 'https://x.example/y.png',
    })
  })

  it('normalises a dot segment in the middle', () => {
    expect(resolveImageTarget('a/./b/../c.png', 'n.md')).toEqual({
      kind: 'bikeshed',
      path: 'a/c.png',
    })
  })

  it('refuses a backslash, a drive prefix and a control character as invalid', () => {
    for (const src of [
      'a\\b.png',
      'C:\\evil.png',
      'C:evil.png',
      '\u0000x.png',
      'a\u0001b.png',
      'a\u007fb.png',
    ]) {
      expect(resolveImageTarget(src, 'n.md'), src).toEqual({ kind: 'invalid' })
    }
  })

  it('refuses a percent-decoded slash inside a segment as invalid', () => {
    expect(resolveImageTarget('a%2Fb.png', 'n.md')).toEqual({ kind: 'invalid' })
    expect(resolveImageTarget('a%2fb.png', 'n.md')).toEqual({ kind: 'invalid' })
  })
})

describe('the tandem asset URL', () => {
  it('percent-encodes each segment and leaves the separators', () => {
    expect(assetUrl('a b.png')).toBe('tandem:a%20b.png')
    expect(assetUrl('a#b.png')).toBe('tandem:a%23b.png')
    expect(assetUrl('a?b.png')).toBe('tandem:a%3Fb.png')
    expect(assetUrl('a%b.png')).toBe('tandem:a%25b.png')
    expect(assetUrl('dir/a b.png')).toBe('tandem:dir/a%20b.png')
    expect(assetUrl('a+b.png')).toBe('tandem:a%2Bb.png')
    expect(assetUrl('naïve.png')).toBe('tandem:na%C3%AFve.png')
  })

  it('encodes an empty path to the bare scheme', () => {
    expect(assetUrl('')).toBe('tandem:')
  })
})
