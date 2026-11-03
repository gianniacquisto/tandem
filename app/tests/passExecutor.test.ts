import { createServer, type IncomingMessage, type Server } from 'node:http'
import type { AddressInfo } from 'node:net'

import { describe, expect, it } from 'vitest'

import {
  executePass,
  type PassBrief,
  type PassResult,
} from '../src/main/passExecutor'

const PASSWORD = 'stub-password'

function expectedAuth(): string {
  return `Basic ${Buffer.from(`opencode:${PASSWORD}`, 'utf8').toString('base64')}`
}

type Seen = {
  method: string | undefined
  url: string | undefined
  auth: string | undefined
  body: unknown
}

/**
 * A stub harness: loopback HTTP, Basic auth, brief in and the fixed-shape
 * result out. Every request asserts the credential the app holds; a request
 * without it answers 401, the way the bundled harness does (ADR 0038).
 */
function stubHarness(onBody: (body: Record<string, unknown>) => Record<string, unknown>): {
  server: Server
  seen: Seen[]
  baseUrl: (server: Server) => string
} {
  const seen: Seen[] = []
  const server = createServer((req: IncomingMessage, res) => {
    let text = ''
    req.on('data', (chunk: Buffer) => {
      text += chunk.toString('utf8')
    })
    req.on('end', () => {
      const body: Record<string, unknown> = text === '' ? {} : JSON.parse(text)
      seen.push({ method: req.method, url: req.url, auth: req.headers['authorization'], body })
      if (req.headers['authorization'] !== expectedAuth()) {
        res.writeHead(401, { 'content-type': 'application/json' })
        res.end(JSON.stringify({ error: 'unauthorized' }))
        return
      }
      if (req.method !== 'POST' || req.url !== '/api/sprint/pass') {
        res.writeHead(404, { 'content-type': 'application/json' })
        res.end(JSON.stringify({ error: 'not found' }))
        return
      }
      res.writeHead(200, { 'content-type': 'application/json' })
      res.end(JSON.stringify(onBody(body)))
    })
  })
  return {
    server,
    seen,
    baseUrl: (running: Server) =>
      `http://127.0.0.1:${(running.address() as AddressInfo).port}`,
  }
}

function listen(server: Server): Promise<void> {
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => resolve())
  })
}

function close(server: Server): Promise<void> {
  return new Promise((resolve, reject) => {
    server.close((error) => (error === undefined ? resolve() : reject(error)))
  })
}

describe('the pass executor', () => {
  it('executor_returns_the_fixed_shape_over_http', async () => {
    const { server, seen, baseUrl } = stubHarness((body) => {
      if (body['brief'] === 'FAIL') {
        return { ticket: body['ticket'], pass: body['pass'], reason: 'the stub refused the brief' }
      }
      switch (body['pass']) {
        case 'refine':
          return { ticket: body['ticket'], pass: 'refine', ready: true }
        case 'review':
          return { ticket: body['ticket'], pass: 'review', merged: true }
        default:
          return { ticket: body['ticket'], pass: 'build', pr: 42 }
      }
    })
    await listen(server)
    try {
      const base = baseUrl(server)
      const refine: PassBrief = { ticket: 't-one', pass: 'refine', brief: '## Why\n\nA.\n' }
      const build: PassBrief = { ticket: 't-two', pass: 'build', brief: '## Why\n\nB.\n' }
      const review: PassBrief = { ticket: 't-three', pass: 'review', brief: '## Why\n\nC.\n' }
      const failed: PassBrief = { ticket: 't-four', pass: 'build', brief: 'FAIL' }

      const refined: PassResult = await executePass(refine, { baseUrl: base, password: PASSWORD })
      const built: PassResult = await executePass(build, { baseUrl: base, password: PASSWORD })
      const reviewed: PassResult = await executePass(review, { baseUrl: base, password: PASSWORD })
      const refused: PassResult = await executePass(failed, { baseUrl: base, password: PASSWORD })

      // The fixed shapes part 1 (`rust/core/src/domain/runner.rs::PassResult`)
      // owns: the transition function stays the only decider, main only reports.
      expect(refined).toEqual({ kind: 'refined', ticket: 't-one', ready: true })
      expect(built).toEqual({ kind: 'built', ticket: 't-two', pr: 42 })
      expect(reviewed).toEqual({ kind: 'reviewed', ticket: 't-three', merged: true })
      expect(refused).toEqual({
        kind: 'failed',
        ticket: 't-four',
        pass: 'build',
        reason: 'the stub refused the brief',
      })

      // The stub asserted Basic auth on loopback on every request.
      expect(seen).toHaveLength(4)
      for (const request of seen) {
        expect(request.url).toBe('/api/sprint/pass')
        expect(request.auth).toBe(expectedAuth())
      }
      expect(seen[1]?.body).toMatchObject({ ticket: 't-two', pass: 'build', brief: '## Why\n\nB.\n' })
    } finally {
      await close(server)
    }
  })

  it('refuses_a_non_loopback_url_without_connecting', async () => {
    const brief: PassBrief = { ticket: 't-one', pass: 'build', brief: '## Why\n\nA.\n' }
    await expect(
      executePass(brief, { baseUrl: 'http://10.0.0.9:3939', password: PASSWORD }),
    ).rejects.toThrow(/loopback/i)
  })

  it('maps_a_wrong_password_to_the_password_sources_without_leaking_the_secret', async () => {
    const { server, baseUrl } = stubHarness((body) => ({
      ticket: body['ticket'],
      pass: body['pass'],
      reason: 'unreached',
    }))
    await listen(server)
    try {
      const brief: PassBrief = { ticket: 't-one', pass: 'build', brief: '## Why\n\nA.\n' }
      const failure = await executePass(brief, {
        baseUrl: baseUrl(server),
        password: 'wrong-password',
      }).catch((error: unknown) => error)
      expect(String((failure as Error).message)).toMatch(/password/i)
      expect(String((failure as Error).message)).not.toContain('wrong-password')
    } finally {
      await close(server)
    }
  })

  it('refuses_an_unknown_result_shape_instead_of_defaulting', async () => {
    const { server, baseUrl } = stubHarness(() => ({ surprising: 'shape' }))
    await listen(server)
    try {
      const brief: PassBrief = { ticket: 't-one', pass: 'build', brief: '## Why\n\nA.\n' }
      await expect(
        executePass(brief, { baseUrl: baseUrl(server), password: PASSWORD }),
      ).rejects.toThrow(/Unknown pass result/i)
    } finally {
      await close(server)
    }
  })
})
