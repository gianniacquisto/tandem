import {
  CALL_CHANNEL,
  CHANGED_CHANNEL,
  RpcError,
  type Bridge,
  type BridgeCall,
  type Call,
  type Reply,
} from '../shared/bridge'

/**
 * What the preload is allowed to know about Electron: two channels and the
 * renderer's own listener bookkeeping. It is an interface, not an import, so
 * this file stays testable in Node and the surface stays one screen long.
 */
export type IpcSurface = {
  invoke(channel: string, call: Call): Promise<BridgeCall>
  on(channel: string, listener: (revision: number) => void): void
  off(channel: string): void
}

/**
 * The window's whole API: one call, one subscription.
 *
 * `contextBridge` deep-freezes what it exposes, and this factory freezes it a
 * second time so the shape is the same under a test. A consumer that needs
 * more opens a ticket on the protocol; it does not widen this object.
 */
export function createBridge(ipc: IpcSurface): Bridge {
  return Object.freeze({
    async call(call: Call): Promise<Reply> {
      const answer = await ipc.invoke(CALL_CHANNEL, call)
      if ('error' in answer) throw new RpcError(answer.error)
      return answer.reply
    },

    onChanged(listener: (revision: number) => void): () => void {
      ipc.on(CHANGED_CHANNEL, listener)
      return () => {
        ipc.off(CHANGED_CHANNEL)
      }
    },
  })
}
