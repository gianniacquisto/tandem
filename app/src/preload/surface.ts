import {
  CALL_CHANNEL,
  CHANGED_CHANNEL,
  type Bridge,
  type BridgeCall,
  type Call,
} from '../shared/bridge'

/**
 * What the preload is allowed to know about Electron: two channels and the
 * listener's own detach. It is an interface, not an import, so this file stays
 * testable in Node and the surface stays one screen long.
 */
export type IpcSurface = {
  invoke(channel: string, call: Call): Promise<BridgeCall>
  /** Subscribes, and answers with the detach for that one listener. */
  on(channel: string, listener: (revision: number) => void): () => void
}

/**
 * The window's whole API: one call, one subscription.
 *
 * `contextBridge` deep-freezes what it exposes, and this factory freezes it a
 * second time so the shape is the same under a test. The refusal is answered,
 * not thrown, because a throw loses the code at the bridge's clone. A consumer
 * that needs more opens a ticket on the protocol; it does not widen this object.
 */
export function createBridge(ipc: IpcSurface): Bridge {
  return Object.freeze({
    async call(call: Call): Promise<BridgeCall> {
      return ipc.invoke(CALL_CHANNEL, call)
    },

    onChanged(listener: (revision: number) => void): () => void {
      return ipc.on(CHANGED_CHANNEL, listener)
    },
  })
}
