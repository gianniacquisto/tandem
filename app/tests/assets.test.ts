import { describe, expect, it } from 'vitest'

import {
  assetInitiator,
  assetPathFromUrl,
  respondToAsset,
  type AssetDeps,
  type AssetView,
} from '../src/main/assets'
import { assetUrl } from '../renderer/src/shell/image'

const FILE = '/vault/bikeshed/img/dot.png'

function streamOf(bytes: string): ReadableStream<Uint8Array> {
  return new ReadableStream({
    start(controller) {
      controller.enqueue(new TextEncoder().encode(bytes))
      controller.close()
    },
  })
}

type Answer = { asset?: AssetView } & { error?: { code: string; message: string } }

function harness(
  answer: Promise<Answer> = Promise.resolve({
    asset: {
      path: 'img/dot.png',
      file: FILE,
      mediaType: 'image/png',
      size: 9,
    },
  }),
  overrides: Partial<AssetDeps> = {},
) {
  const calls: string[] = []
  const reads: string[] = []
  const deps: AssetDeps = {
    allowedInitiator: 'file://',
    call: (path) => {
      calls.push(path)
      return answer
    },
    readFile: (file) => {
      reads.push(file)
      return Promise.resolve(streamOf('png-bytes'))
    },
    ...overrides,
  }
  return { calls, reads, deps }
}

function ask(url: string, deps: AssetDeps, initiatorOrigin: string | null = 'file://') {
  return respondToAsset({ url, initiatorOrigin }, deps)
}

describe('the tandem asset protocol handler', () => {
  it('serves a good URL from the core reply with its type and safe headers', async () => {
    const { calls, reads, deps } = harness()
    const response = await ask('tandem:img/dot.png', deps)

    expect(response.status).toBe(200)
    expect(calls).toEqual(['img/dot.png'])
    expect(reads).toEqual([FILE])
    expect(response.headers.get('content-type')).toBe('image/png')
    expect(response.headers.get('x-content-type-options')).toBe('nosniff')
    expect(response.headers.get('cache-control')).toBe('no-store')
    expect(response.headers.get('content-length')).toBeNull()
    expect(new TextDecoder().decode(await response.arrayBuffer())).toBe('png-bytes')
  })

  it('decodes the path before asking the core', async () => {
    const { calls, deps } = harness()
    await ask('tandem:img/dot%20one.png', deps)
    expect(calls).toEqual(['img/dot one.png'])
  })

  it('refuses every initiator but the app own, without asking the core', async () => {
    for (const initiator of [null, 'https://evil.example', 'http://localhost:5173']) {
      const { calls, deps } = harness()
      expect((await ask('tandem:img/dot.png', deps, initiator)).status).toBe(404)
      expect(calls).toEqual([])
    }

    const { calls, deps } = harness(undefined, {
      allowedInitiator: 'http://localhost:5173',
    })
    expect((await ask('tandem:img/dot.png', deps, 'file://')).status).toBe(404)
    expect((await ask('tandem:img/dot.png', deps, 'http://localhost:5173')).status).toBe(200)
    expect(calls).toEqual(['img/dot.png'])
  })

  it('refuses a remote URL, an empty path, an escape, an encoded slash, a query and a bad escape', async () => {
    for (const url of [
      'https://example.com/x.png',
      'tandem:',
      'tandem:../escape.png',
      'tandem:a%2Fb.png',
      'tandem:a.png?x=1',
      'tandem:a.png#b',
      'tandem:a%ZZ.png',
    ]) {
      const { calls, reads, deps } = harness()
      expect((await ask(url, deps)).status, url).toBe(404)
      expect(calls, url).toEqual([])
      expect(reads, url).toEqual([])
    }
  })

  it('answers 404 for a domain error without reading the file', async () => {
    const { reads, deps } = harness(
      Promise.resolve({ error: { code: 'domain', message: 'No note at "img/dot.png".' } }),
    )
    expect((await ask('tandem:img/dot.png', deps)).status).toBe(404)
    expect(reads).toEqual([])
  })

  it('answers 404 for a reply with no asset without reading the file', async () => {
    const { reads, deps } = harness(Promise.resolve({}))
    expect((await ask('tandem:img/dot.png', deps)).status).toBe(404)
    expect(reads).toEqual([])
  })

  it('answers 404 when the transport rejects or the stream fails', async () => {
    const refused = harness(Promise.reject(new Error('the rpc child is not running')))
    expect((await ask('tandem:img/dot.png', refused.deps)).status).toBe(404)

    const failing = harness()
    failing.deps.readFile = () => Promise.reject(new Error('the file vanished'))
    expect((await ask('tandem:img/dot.png', failing.deps)).status).toBe(404)
  })
})

describe('the asset path parse', () => {
  it('parses an opaque tandem URL to its decoded path', () => {
    expect(assetPathFromUrl('tandem:img/dot.png')).toEqual({ path: 'img/dot.png' })
    expect(assetPathFromUrl('tandem:img/dot%20one.png')).toEqual({ path: 'img/dot one.png' })
    expect(assetPathFromUrl('tandem:a+b.png')).toEqual({ path: 'a+b.png' })
    expect(assetPathFromUrl('tandem:na%C3%AFve.png')).toEqual({ path: 'naïve.png' })
  })

  it('refuses anything that is not one clean vault path', () => {
    for (const url of [
      'https://example.com/x.png',
      'file:///etc/passwd.png',
      'tandem:',
      'tandem:/etc/passwd.png',
      'tandem:../escape.png',
      'tandem:a//b.png',
      'tandem:a/./b.png',
      'tandem:a%2Fb.png',
      'tandem:a%5Cb.png',
      'tandem:a%00b.png',
      'tandem:C:/evil.png',
      'tandem:a.png?x=1',
      'tandem:a.png#b',
      'tandem:a%ZZ.png',
    ]) {
      expect(assetPathFromUrl(url), url).toBeNull()
    }
  })

  it('round-trips every name the block surface encodes', () => {
    for (const path of ['a b.png', 'a#b.png', 'a?b.png', 'a%b.png', 'naïve.png', 'a+b.png']) {
      expect(assetPathFromUrl(assetUrl(path))).toEqual({ path })
    }
  })
})

describe('the allowed initiator', () => {
  it('is file:// packaged and the dev origin in development', () => {
    expect(assetInitiator(undefined)).toBe('file://')
    expect(assetInitiator('http://localhost:5173')).toBe('http://localhost:5173')
    expect(assetInitiator('http://127.0.0.1:5173')).toBe('http://127.0.0.1:5173')
  })
})
