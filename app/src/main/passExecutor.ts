/**
 * The sprint runner's pass-execution effect (TAN-281, part 2 of TAN-275).
 *
 * The core owns the transitions (`rust/core/src/domain/runner.rs`): the pure
 * function maps every (driver state, pass result) pair to exactly one (next
 * state, effect request), and it is the only decider. This module owns the
 * effect main executes for `Effect::Spawn`: it takes a structured brief, runs
 * it against the harness over HTTP, and returns the fixed-shape result the
 * transition function reads. Main owns spawn, lifetime, url and password
 * (ADR 0038); the core never sees the credential and the transport is a seam
 * (ADR 0037), so this module speaks HTTP and never a shell string.
 *
 * The harness endpoint is `POST {baseUrl}/api/sprint/pass` with
 * `Authorization: Basic base64("opencode:" + password)` on every request, the
 * same credential the loopback usage read carries (TAN-276, ADR 0038). The
 * renderer never touches this module: the password stays in main, never in a
 * core frame, a log or an error string.
 */

/** Which pass a brief dispatches: the three passes the runner speaks. */
export type PassKind = 'refine' | 'build' | 'review'

/** A structured brief: the ticket, its pass, and the brief that pass runs. */
export type PassBrief = {
  ticket: string
  pass: PassKind
  brief: string
}

/**
 * The fixed-shape result part 1 owns (`runner.rs::PassResult`): a refine
 * answers `ready`, a build answers `pr`, a review answers `merged`, and a
 * failure answers its pass with a `reason` for the run entry. Unknown shapes
 * are refused, never defaulted, the way the transition function refuses them.
 */
export type PassResult =
  | { kind: 'refined'; ticket: string; ready: boolean }
  | { kind: 'built'; ticket: string; pr: number }
  | { kind: 'reviewed'; ticket: string; merged: boolean }
  | { kind: 'failed'; ticket: string; pass: PassKind; reason: string }

export type PassExecutorDeps = {
  /** The harness origin main holds (`http://127.0.0.1:<port>`), loopback only. */
  baseUrl: string
  /** The loopback credential main holds. Never logged, never in an error. */
  password: string
  /** Overridable for tests; defaults to the global fetch. */
  fetchFn?: typeof fetch
  /** Bounded wait so a hung harness cannot hang the run (default 8000). */
  timeoutMs?: number
}

const PASS_PATH = '/api/sprint/pass'
const DEFAULT_TIMEOUT_MS = 8000

function isPassKind(value: unknown): value is PassKind {
  return value === 'refine' || value === 'build' || value === 'review'
}

/** Whether the host is loopback: the credential never leaves the machine. */
function isLoopbackHost(host: string): boolean {
  const normalized = host.trim().toLowerCase().replace(/^\[/, '').replace(/\]$/, '')
  return normalized === '127.0.0.1' || normalized === 'localhost' || normalized === '::1'
}

/** The harness origin, or a refusal before anything connects. */
function loopbackBase(baseUrl: string): string {
  let parsed: URL
  try {
    parsed = new URL(baseUrl)
  } catch {
    throw new Error(
      `Refusing to send the pass to "${baseUrl}": the harness API is loopback-only ` +
        `(127.0.0.1, ::1 or localhost). Check --harness-url or TANDEM_HARNESS_URL, and try again.`,
    )
  }
  if (!isLoopbackHost(parsed.hostname)) {
    throw new Error(
      `Refusing to send the pass to "${baseUrl}": the harness API is loopback-only ` +
        `(127.0.0.1, ::1 or localhost). Check --harness-url or TANDEM_HARNESS_URL, and try again.`,
    )
  }
  return parsed.origin
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

/**
 * The harness JSON to the fixed shape: success fields per pass, or a `reason`
 * for the run entry. A result for another ticket or another pass is refused,
 * and an unknown shape is refused, never defaulted.
 */
function toPassResult(raw: unknown, brief: PassBrief): PassResult {
  if (!isRecord(raw)) {
    throw new Error(
      `Unknown pass result ${JSON.stringify(raw)?.slice(0, 200) ?? 'null'}. ` +
        `The runner speaks refine, build and review results; nothing else.`,
    )
  }
  const ticket = raw['ticket']
  if (ticket !== brief.ticket) {
    if (typeof ticket === 'string' && ticket !== '') {
      throw new Error(
        `Pass result for "${ticket}" does not belong to the outstanding pass on ` +
          `"${brief.ticket}". Route each result to its own ticket's step.`,
      )
    }
    throw new Error(
      `Unknown pass result ${JSON.stringify(raw).slice(0, 200)}. ` +
        `The runner speaks refine, build and review results; nothing else.`,
    )
  }
  const pass = raw['pass']
  if (pass !== brief.pass) {
    if (typeof pass === 'string' && pass !== '') {
      throw new Error(
        `The runner refused the step: pass result names pass "${pass}" for ticket ` +
          `"${brief.ticket}", but the outstanding pass is "${brief.pass}".`,
      )
    }
    throw new Error(
      `Unknown pass result ${JSON.stringify(raw).slice(0, 200)}. ` +
        `The runner speaks refine, build and review results; nothing else.`,
    )
  }
  const reason = raw['reason']
  if (typeof reason === 'string') {
    if (reason.trim() === '') {
      throw new Error(
        `Unknown pass result ${JSON.stringify(raw).slice(0, 200)}. ` +
          `The runner speaks refine, build and review results; nothing else.`,
      )
    }
    if (!isPassKind(pass)) {
      throw new Error(
        `Unknown pass result ${JSON.stringify(raw).slice(0, 200)}. ` +
          `The runner speaks refine, build and review results; nothing else.`,
      )
    }
    return { kind: 'failed', ticket: brief.ticket, pass, reason }
  }
  switch (brief.pass) {
    case 'refine': {
      const ready = raw['ready']
      if (typeof ready !== 'boolean') {
        throw new Error(
          `Unknown pass result ${JSON.stringify(raw).slice(0, 200)}. ` +
            `The runner speaks refine, build and review results; nothing else.`,
        )
      }
      return { kind: 'refined', ticket: brief.ticket, ready }
    }
    case 'build': {
      const pr = raw['pr']
      if (typeof pr !== 'number' || !Number.isInteger(pr) || pr < 1) {
        throw new Error(
          `Unknown pass result ${JSON.stringify(raw).slice(0, 200)}. ` +
            `The runner speaks refine, build and review results; nothing else.`,
        )
      }
      return { kind: 'built', ticket: brief.ticket, pr }
    }
    case 'review': {
      const merged = raw['merged']
      if (typeof merged !== 'boolean') {
        throw new Error(
          `Unknown pass result ${JSON.stringify(raw).slice(0, 200)}. ` +
            `The runner speaks refine, build and review results; nothing else.`,
        )
      }
      return { kind: 'reviewed', ticket: brief.ticket, merged }
    }
  }
}

/**
 * Dispatch one pass: brief in over loopback HTTP with Basic auth, fixed-shape
 * result out. The transition function from part 1 stays the only decider:
 * this function executes the effect and reports, it never re-derives state.
 */
export async function executePass(brief: PassBrief, deps: PassExecutorDeps): Promise<PassResult> {
  if (brief.ticket.trim() === '') {
    throw new Error('Ticket id is empty. Provide the ticket the pass runs on, and try again.')
  }
  if (!isPassKind(brief.pass)) {
    throw new Error(
      `Unknown pass "${String(brief.pass)}". The runner speaks refine, build and review; nothing else.`,
    )
  }
  if (brief.brief.trim() === '') {
    throw new Error('Brief is empty. Provide the structured brief the pass runs, and try again.')
  }
  if (deps.password === '') {
    throw new Error(
      'The harness password is empty. Check TANDEM_HARNESS_PASSWORD or the rendezvous file it was read from, and try again.',
    )
  }
  const base = loopbackBase(deps.baseUrl)
  const auth = `Basic ${Buffer.from(`opencode:${deps.password}`, 'utf8').toString('base64')}`
  const fetchFn = deps.fetchFn ?? globalThis.fetch
  const timeoutMs = deps.timeoutMs ?? DEFAULT_TIMEOUT_MS

  let response: Response
  try {
    response = await fetchFn(`${base}${PASS_PATH}`, {
      method: 'POST',
      headers: {
        authorization: auth,
        'content-type': 'application/json',
      },
      body: JSON.stringify({ ticket: brief.ticket, pass: brief.pass, brief: brief.brief }),
      signal: AbortSignal.timeout(timeoutMs),
    })
  } catch {
    throw new Error(
      `Cannot reach the harness at ${base}. Check that the harness is running, ` +
        `or pass --harness-url <url> or set TANDEM_HARNESS_URL, and try again.`,
    )
  }
  if (response.status === 401) {
    throw new Error(
      `The harness at ${base} refused the password. Check TANDEM_HARNESS_PASSWORD ` +
        `or the rendezvous file it was read from, and try again.`,
    )
  }
  const text = await response.text().catch(() => '')
  if (!response.ok) {
    const prefix = text.slice(0, 200)
    throw new Error(
      `The harness at ${base} answered status ${response.status}` +
        (prefix === '' ? '.' : `: ${prefix}.`) +
        ' Check the harness log, and try again.',
    )
  }
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    const prefix = text.slice(0, 200)
    throw new Error(
      `The harness at ${base} answered a pass result that is not JSON` +
        (prefix === '' ? '.' : `: ${prefix}.`) +
        ' Check the harness log, and try again.',
    )
  }
  return toPassResult(parsed, brief)
}
