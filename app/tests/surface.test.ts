import { describe, expect, it } from 'vitest'

import { createBridge, type IpcSurface } from '../src/preload/surface'
import { CALL_CHANNEL, CHANGED_CHANNEL } from '../src/shared/bridge'

const refused = {
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
} as const

function fakeIpc() {
  const invokes: Array<{ channel: string; payload: unknown }> = []
  const listeners = new Set<(revision: number) => void>()

  const ipc: IpcSurface = {
    invoke: async (channel, method) => {
      invokes.push({ channel, payload: method })
      if (method.method === 'lane.list') return { reply: { revision: 3, lanes: [] } }
      return { error: { code: 'domain', message: 'refused' } }
    },
    on: (channel, listener) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
  }

  const emit = (revision: number) => {
    for (const listener of listeners) listener(revision)
  }

  return { ipc, invokes, listeners, emit }
}

describe('the preload surface', () => {
  it('is exactly two members: one call and one subscription', () => {
    const bridge = createBridge(fakeIpc().ipc)
    expect(Object.keys(bridge).sort()).toEqual(['call', 'onChanged'])
  })

  it('carries a call over the one channel and answers the reply', async () => {
    const { ipc, invokes } = fakeIpc()
    const bridge = createBridge(ipc)

    const answer = await bridge.call({ method: 'lane.list', params: {} })

    expect(answer).toEqual({ reply: { revision: 3, lanes: [] } })
    expect(invokes).toEqual([{ channel: CALL_CHANNEL, payload: { method: 'lane.list', params: {} } }])
  })

  // `contextBridge` clones the value it returns to the main world, and a thrown
  // value does not survive that clone: it becomes a plain `Error` with the
  // message alone. So a refusal crosses as data, and the check is that the data
  // is still whole after the clone the bridge makes.
  it('answers a refusal as data whose code survives the bridge clone', async () => {
    const bridge = createBridge(fakeIpc().ipc)

    const answer = await bridge.call(refused)

    expect(answer).toEqual({ error: { code: 'domain', message: 'refused' } })
    expect(structuredClone(answer)).toEqual({ error: { code: 'domain', message: 'refused' } })
  })

  it('never rejects a refusal, because nothing it raises would cross the wall', async () => {
    const bridge = createBridge(fakeIpc().ipc)

    await expect(bridge.call(refused)).resolves.not.toBeInstanceOf(Error)
  })

  it('subscribes on the change channel and detaches only the subscriber that stopped', () => {
    const { ipc, listeners, emit } = fakeIpc()
    const bridge = createBridge(ipc)
    const first: number[] = []
    const second: number[] = []

    const stop = bridge.onChanged((revision) => first.push(revision))
    bridge.onChanged((revision) => second.push(revision))
    emit(9)
    expect(first).toEqual([9])
    expect(second).toEqual([9])

    stop()
    emit(10)
    expect(first).toEqual([9])
    expect(second).toEqual([9, 10])
    expect(listeners.size).toBe(1)
  })

  it('addresses the change channel and nothing else', () => {
    const seen: string[] = []
    const bridge = createBridge({
      invoke: async () => ({ reply: { revision: 0, lanes: [] } }),
      on: (channel, _listener) => {
        seen.push(channel)
        return () => {}
      },
    })

    bridge.onChanged(() => {})
    expect(seen).toEqual([CHANGED_CHANNEL])
  })
})
