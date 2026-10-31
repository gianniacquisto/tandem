import type { Method } from '../../renderer/src/wire/Method'
import type { Reply } from '../../renderer/src/wire/Reply'
import type { WireError } from '../../renderer/src/wire/WireError'

export type { Method, Reply, WireError }

/**
 * The bridge between the three processes, and its message shapes.
 *
 * One request in, one answer out: the preload never invents a method, and the
 * renderer never sees a frame. A read and a write travel the same call, so a
 * UI-only write cannot appear without a method the CLI also exposes.
 */
export type Call = Method

/** What the renderer gets back from one call: a reply, or the core's refusal. */
export type BridgeCall = { reply: Reply } | { error: WireError }

/** The two channels a preload may touch. They are the whole surface. */
export const CALL_CHANNEL = 'tandem:call'
export const CHANGED_CHANNEL = 'tandem:changed'

/**
 * A refusal from the core inside the main process, where a real `Error` can
 * carry the code. It never crosses the bridge: `contextBridge` clones what it
 * returns and rebuilds a thrown value as a plain `Error` with the message
 * only, so a refusal travels as `BridgeCall`'s `error` member.
 */
export class RpcError extends Error {
  readonly code: string

  constructor(error: WireError) {
    super(error.message)
    this.name = 'RpcError'
    this.code = error.code
  }
}

/**
 * The narrow window surface: one call and one subscription.
 *
 * A refusal is answered, not thrown: `contextBridge` clones the value, and a
 * thrown `Error` loses everything but its message at that clone, so the code
 * has to be part of the data. A renderer narrows on `'error' in answer`.
 *
 * A renderer that needs a third verb opens a ticket on the protocol, not a
 * wider bridge. The preload exposes exactly this type as `window.tandem`.
 */
export type Bridge = {
  call(call: Call): Promise<BridgeCall>
  onChanged(listener: (revision: number) => void): () => void
}
