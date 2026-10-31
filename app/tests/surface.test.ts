import { describe, expect, it } from 'vitest'

import { createBridge, type IpcSurface } from '../src/preload/surface'
import { CALL_CHANNEL, CHANGED_CHANNEL, RpcError } from '../src/shared/bridge'

function fakeIpc() {
  const invokes: Array<{ channel: string; payload: unknown }> = []
  const listeners = new Map<string, (revision: number) => void>()

  const ipc: IpcSurface = {
    invoke: async (channel, method) => {
      invokes.push({ channel, payload: method })
      if (method.method === 'lane.list') return { reply: { revision: 3, lanes: [] } }
      return { error: { code: 'domain', message: 'refused' } }
    },
    on: (channel, listener) => listeners.set(channel, listener),
    off: (channel) => listeners.delete(channel),
  }

  return { ipc, invokes, listeners }
}

describe('the preload surface', () => {
  it('is exactly two members: one call and one subscription', () => {
    const bridge = createBridge(fakeIpc().ipc)
    expect(Object.keys(bridge).sort()).toEqual(['call', 'onChanged'])
  })

  it('carries a call over the one channel and answers the reply', async () => {
    const { ipc, invokes } = fakeIpc()
    const bridge = createBridge(ipc)

    const reply = await bridge.call({ method: 'lane.list', params: {} })

    expect(reply).toEqual({ revision: 3, lanes: [] })
    expect(invokes).toEqual([{ channel: CALL_CHANNEL, payload: { method: 'lane.list', params: {} } }])
  })

  it('turns an error frame into a thrown RpcError with its code', async () => {
    const bridge = createBridge(fakeIpc().ipc)

    const failure = bridge.call({ method: 'ticket.list', params: {
      filter: null, text: null, status: null, assignee: null, lane: null,
      archived: false, blocked: false,
    } })

    await expect(failure).rejects.toBeInstanceOf(RpcError)
    await expect(failure).rejects.toMatchObject({ code: 'domain', message: 'refused' })
  })

  it('subscribes on the change channel and unsubscribes on demand', () => {
    const { ipc, listeners } = fakeIpc()
    const bridge = createBridge(ipc)
    const seen: number[] = []

    const stop = bridge.onChanged((revision) => seen.push(revision))
    listeners.get(CHANGED_CHANNEL)?.(9)
    expect(seen).toEqual([9])

    stop()
    expect(listeners.has(CHANGED_CHANNEL)).toBe(false)
  })
})
