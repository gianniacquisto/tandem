import type { Method } from '../../renderer/src/wire/Method'
import type { Reply } from '../../renderer/src/wire/Reply'
import type { WireError } from '../../renderer/src/wire/WireError'

export type { Method, Reply, WireError }

/**
 * The bridge between the three processes, and its message shapes.
 *
 * One request in, one answer out: the preload never invents a method, and the
 * renderer never sees a frame. A read and a write travel the same call, so a
 * UI-only write cannot appear without a method the CLI also exposes, except
 * the harness sign-in: `harness.*` is answered by the main process itself
 * (TAN-265), which owns the harness binary and its environment, and the core
 * never sees it.
 */
export type Call = Method | HarnessCall

/** What the renderer gets back from one core call: the reply, or the refusal. */
export type MethodCall = { reply: Reply } | { error: WireError }

/** What the renderer gets back from one harness call: the reply, or the refusal. */
export type HarnessCallAnswer = { reply: HarnessReply } | { error: WireError }

/** What the renderer gets back from one call: a reply, or a refusal. */
export type BridgeCall = MethodCall | HarnessCallAnswer

/** The two channels a preload may touch. They are the whole surface. */
export const CALL_CHANNEL = 'tandem:call'
export const CHANGED_CHANNEL = 'tandem:changed'

/**
 * One stored account on an integration, as `auth list --format json` reports
 * it minus the secret: the id the logout names, the label the face draws, and
 * the method that stored it.
 */
export type HarnessConnection = {
  id: string
  label: string
  method: string
}

/**
 * One integration row, as `auth list` reports it: the id the logout names,
 * the name the face draws, and the stored accounts (empty when unsigned).
 */
export type HarnessIntegration = {
  id: string
  name: string
  connections: HarnessConnection[]
}

/** The pending device-flow attempt a `harness.login` started. */
export type HarnessPending = {
  url: string
  code: string | null
}

/**
 * The harness sign-in calls (TAN-265). Answered by the main process, never by
 * the core: the renderer reaches the harness home only through this door, and
 * no CLI verb spells these acts. The one exception to the UI-only-write rule,
 * decided on the ticket: a sign-in is not a domain write and the harness is
 * not the store.
 */
export type HarnessCall =
  | { method: 'harness.status'; params: Record<string, never> }
  | { method: 'harness.login'; params: Record<string, never> }
  | { method: 'harness.cancel'; params: Record<string, never> }
  | { method: 'harness.logout'; params: { integration?: string } }

/** The harness answers: the status read, the login attempt, or a receipt. */
export type HarnessReply =
  | {
      integrations: HarnessIntegration[]
      pending: HarnessPending | null
      failure: string | null
    }
  | { url: string; code: string | null }
  | { cancelled: boolean }
  | { integration: string }

/** A status read, narrowed from the union by the method the face called. */
export type HarnessStatusReply = {
  integrations: HarnessIntegration[]
  pending: HarnessPending | null
  failure: string | null
}

/** A started login, narrowed from the union the same way. */
export type HarnessLoginReply = {
  url: string
  code: string | null
}

/** Whether `call` is a harness call the main process answers itself. */
export function isHarnessCall(call: Call): call is HarnessCall {
  return typeof call.method === 'string' && call.method.startsWith('harness.')
}

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
  call(call: Method): Promise<MethodCall>
  call(call: HarnessCall): Promise<HarnessCallAnswer>
  onChanged(listener: (revision: number) => void): () => void
}
