import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import { RpcClient } from '../src/main/rpcClient'

/** A stand-in for `tandem rpc`: one JSON frame per line, the same contract. */
function fakeServer(directory: string): string {
  const path = join(directory, 'fake-rpc.mjs')
  writeFileSync(
    path,
    `
import { createInterface } from 'node:readline'

const out = (frame) => process.stdout.write(JSON.stringify(frame) + '\\n')

// One change event before any request, the way the watch pushes one.
out({ event: 'changed', revision: 7 })

const lines = createInterface({ input: process.stdin })
lines.on('line', (line) => {
  const request = JSON.parse(line)
  if (request.method === 'ticket.list') {
    out({ id: request.id, result: { revision: 7, tickets: [] } })
  } else {
    out({ id: request.id, error: { code: 'domain', message: 'no such method' } })
  }
})
`,
  )
  return path
}

const cleanups: Array<() => void> = []

afterEach(() => {
  while (cleanups.length > 0) cleanups.pop()?.()
})

function connect(): { client: RpcClient; exited: Promise<number | null> } {
  const directory = mkdtempSync(join(tmpdir(), 'tandem-rpc-'))
  const server = fakeServer(directory)
  const { client, exited } = RpcClient.spawn(process.execPath, [server])
  cleanups.push(() => {
    client.close()
    rmSync(directory, { recursive: true, force: true })
  })
  return { client, exited }
}

describe('the rpc client', () => {
  it('resolves a call with its reply', async () => {
    const { client } = connect()

    const reply = await client.call({
      method: 'ticket.list',
      params: {
        filter: null,
        text: null,
        status: null,
        assignee: null,
        lane: null,
        archived: false,
        blocked: false,
      },
    })

    expect(reply).toEqual({ revision: 7, tickets: [] })
  })

  it('rejects an error frame with its code and message', async () => {
    const { client } = connect()

    await expect(client.call({ method: 'lane.list', params: {} })).rejects.toMatchObject({
      code: 'domain',
      message: 'no such method',
    })
  })

  it('delivers a change event to every subscriber, and unsubscribes', async () => {
    const { client } = connect()
    const seen: number[] = []

    const stop = client.onChanged((revision) => seen.push(revision))
    await client.call({ method: 'lane.list', params: {} }).catch(() => {})
    expect(seen).toEqual([7])

    stop()
    expect(client.listenerCount()).toBe(0)
  })

  it('ends the child when the client closes the pipe', async () => {
    const { client, exited } = connect()

    client.close()

    await expect(exited).resolves.toBe(0)
  })
})
