import { mkdirSync } from 'node:fs'
import { join } from 'node:path'

import {
  isHarnessCall,
  type HarnessCall,
  type HarnessIntegration,
  type HarnessLoginReply,
  type HarnessStatusReply,
} from '../shared/bridge'

/**
 * The harness sign-in's main-process half (TAN-265, ADR 0038).
 *
 * The renderer never touches the harness: it calls `harness.*` over the one
 * bridge, and this module spawns every `opencode` child with the harness-home
 * environment, owns the poll (each status read runs here), opens the device
 * URL in the system browser, and kills the login on cancel. No Electron import
 * lives here: `spawn` and `openUrl` are injected, so the suite drives a fake
 * child and the shell wires the real ones.
 */

/** The seam a child is spawned through, so the suite can drive a fake. */
export type HarnessSpawn = (
  binary: string,
  args: string[],
  options: { env: Record<string, string>; cwd: string },
) => HarnessChild

/** The sliver of a child the controller needs: output, exit and a kill. */
export type HarnessChild = {
  onStdout(listener: (chunk: string) => void): void
  onStderr(listener: (chunk: string) => void): void
  onExit(listener: (code: number | null) => void): void
  kill(): void
}

export type TandemHomeSource = {
  env: Record<string, string | undefined>
  packaged: boolean
  appPath: string
  homeDir: string
}

/**
 * The Tandem home the harness roots in, by the shell's own rule
 * (`src/main/index.ts::homeEnvironment`): `TANDEM_HOME` wins, a packaged app
 * reads the user's home, and development reads the fixture home.
 */
export function tandemHomeDir(source: TandemHomeSource): string {
  const named = source.env['TANDEM_HOME']
  if (named !== undefined && named.trim() !== '') return named
  if (source.packaged) return join(source.homeDir, '.tandem')
  return join(source.appPath, '..', 'Fixtures', 'port', 'Home')
}

/**
 * The harness environment (ADR 0038): every global path the binary resolves
 * rooted in `<TandemHome>/harness`. `HOME` is deliberately absent: the harness
 * runs tools in the human's name, and a fake home would break that quietly.
 */
export function harnessEnv(tandemHome: string): Record<string, string> {
  const root = join(tandemHome, 'harness')
  const data = join(root, 'data')
  const config = join(root, 'config')
  return {
    XDG_DATA_HOME: data,
    XDG_CACHE_HOME: join(root, 'cache'),
    XDG_CONFIG_HOME: config,
    XDG_STATE_HOME: join(root, 'state'),
    TMPDIR: join(root, 'tmp'),
    OPENCODE_DB: join(root, 'opencode.db'),
    OPENCODE_CONFIG_DIR: config,
  }
}

/** The opencode binary: an override for a checkout, else the PATH. */
export function resolveHarnessBinary(env: Record<string, string | undefined>): string {
  const named = env['TANDEM_OPENCODE_BIN']
  if (named !== undefined && named.trim() !== '') return named
  return 'opencode'
}

/** `auth list` against the harness home, never the live service. */
export function statusArgs(): string[] {
  return ['auth', 'list', '--format', 'json', '--standalone']
}

/** The Console device flow, non-interactive: it prints a URL and waits. */
export function loginArgs(): string[] {
  return ['auth', 'login', 'opencode', '--method', 'device', '--standalone']
}

/** Log out of one stored account, in the harness home only. */
export function logoutArgs(integration: string, credential: string): string[] {
  return ['auth', 'logout', integration, credential, '--standalone']
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

/**
 * The per-integration rows of `auth list --format json`. Unknown fields ride
 * along ignored; a frame that is not a list is refused, because a status the
 * face cannot read must read as an error, not as unsigned.
 */
export function parseAuthList(text: string): HarnessIntegration[] {
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    throw new Error('the harness answered an auth list that is not JSON')
  }
  if (!Array.isArray(parsed)) throw new Error('the harness answered an auth list that is not a list')
  return parsed.map((entry) => {
    if (!isRecord(entry) || typeof entry['id'] !== 'string' || typeof entry['name'] !== 'string') {
      throw new Error('the harness answered an auth row without an id and a name')
    }
    const raw = entry['connections']
    const connections =
      raw === undefined
        ? []
        : (Array.isArray(raw) ? raw : []).map((connection) => {
            if (
              !isRecord(connection) ||
              typeof connection['label'] !== 'string' ||
              typeof connection['method'] !== 'string'
            ) {
              throw new Error('the harness answered an account without a label and a method')
            }
            // The logout names the credential id; a row without one falls back
            // to the label the same way the picker would.
            const id = typeof connection['id'] === 'string' ? connection['id'] : ''
            return { id, label: connection['label'], method: connection['method'] }
          })
    return { id: entry['id'], name: entry['name'], connections }
  })
}

/** The first URL in the login output: the device page the browser opens. */
export function parseLoginUrl(output: string): string | null {
  const match = output.match(/https:\/\/\S+/)
  return match?.[0] ?? null
}

/** The `Enter code: XXXX-XXXX` line beside the URL, for a browser-less copy. */
export function parseLoginCode(output: string): string | null {
  const match = output.match(/Enter code:\s*([A-Za-z0-9]+-[A-Za-z0-9]+)/)
  return match?.[1] ?? null
}

export type HarnessControllerDeps = {
  spawn: HarnessSpawn
  openUrl: (url: string) => void
  tandemHome: string
  env: Record<string, string | undefined>
  baseEnv?: NodeJS.ProcessEnv
}

/**
 * One sign-in session: at most one pending login, cancelled or completed.
 * Every spawn carries the harness environment, and every read answers from
 * the harness home, so a cancel that kills the child is what leaves no
 * partial credential behind.
 */
export class HarnessController {
  private readonly deps: HarnessControllerDeps
  private loginChild: HarnessChild | null = null
  private loginAttempt: Promise<HarnessLoginReply> | null = null
  private pending: { url: string; code: string | null } | null = null
  private failure: string | null = null

  constructor(deps: HarnessControllerDeps) {
    this.deps = deps
  }

  private spawnEnv(): Record<string, string> {
    const base: Record<string, string> = {}
    for (const [key, value] of Object.entries(this.deps.baseEnv ?? {})) {
      if (value !== undefined) base[key] = value
    }
    return { ...base, ...harnessEnv(this.deps.tandemHome) }
  }

  private ensureHome(): void {
    const env = harnessEnv(this.deps.tandemHome)
    for (const dir of [
      this.deps.tandemHome,
      join(this.deps.tandemHome, 'harness'),
      env['XDG_DATA_HOME'],
      env['XDG_CACHE_HOME'],
      env['XDG_CONFIG_HOME'],
      env['XDG_STATE_HOME'],
      env['TMPDIR'],
    ]) {
      if (dir !== undefined) mkdirSync(dir, { recursive: true })
    }
  }

  /** The URLs and codes the device flow has printed so far. */
  private run(
    args: string[],
  ): Promise<{ stdout: string; stderr: string; code: number | null }> {
    return new Promise((resolve, reject) => {
      let child: HarnessChild
      try {
        this.ensureHome()
        child = this.deps.spawn(resolveHarnessBinary(this.deps.env), args, {
          env: this.spawnEnv(),
          cwd: join(this.deps.tandemHome, 'harness'),
        })
      } catch (error) {
        reject(error instanceof Error ? error : new Error('the harness binary did not start'))
        return
      }
      let stdout = ''
      let stderr = ''
      child.onStdout((chunk) => {
        stdout += chunk
      })
      child.onStderr((chunk) => {
        stderr += chunk
      })
      child.onExit((code) => {
        if (code === 0) {
          resolve({ stdout, stderr, code })
        } else {
          reject(new Error(tail(stderr) || `the harness exited with code ${code}`))
        }
      })
    })
  }

  /**
   * Start the Console device flow: the browser opens at the printed URL, the
   * call answers with that URL and the code, and the child keeps waiting until
   * the human approves or the attempt is cancelled.
   */
  login(): Promise<HarnessLoginReply> {
    if (this.loginAttempt !== null) return this.loginAttempt
    this.failure = null
    this.loginAttempt = new Promise<HarnessLoginReply>((resolve, reject) => {
      let child: HarnessChild
      try {
        this.ensureHome()
        child = this.deps.spawn(resolveHarnessBinary(this.deps.env), loginArgs(), {
          env: this.spawnEnv(),
          cwd: join(this.deps.tandemHome, 'harness'),
        })
      } catch (error) {
        this.loginAttempt = null
        reject(error instanceof Error ? error : new Error('the harness binary did not start'))
        return
      }
      this.loginChild = child
      let output = ''
      let stderr = ''
      let settled = false
      const timer = setTimeout(() => {
        if (settled) return
        settled = true
        child.kill()
        this.loginChild = null
        this.loginAttempt = null
        reject(new Error('the sign-in printed no URL before the wait ran out'))
      }, 60_000)
      const settle = (conclude: () => void) => {
        if (settled) return
        settled = true
        clearTimeout(timer)
        conclude()
      }
      child.onStdout((chunk) => {
        output += chunk
        const url = parseLoginUrl(output)
        if (url !== null && this.pending === null) {
          const code = parseLoginCode(output)
          this.pending = { url, code }
          try {
            this.deps.openUrl(url)
          } catch {
            // The browser failing to open must not fail the attempt: the face
            // shows the URL and the code beside it.
          }
          settle(() => resolve({ url, code }))
        }
      })
      child.onStderr((chunk) => {
        stderr += chunk
      })
      child.onExit((code) => {
        this.loginChild = null
        if (code === 0) {
          this.pending = null
          this.loginAttempt = null
          return
        }
        const url = parseLoginUrl(output)
        if (url !== null && code !== null) {
          // The wait ended without a credential: the face reads this on its
          // next status poll.
          this.failure = tail(stderr) || `the sign-in exited with code ${code}`
          this.pending = null
          this.loginAttempt = null
          return
        }
        settle(() => {
          this.pending = null
          this.loginAttempt = null
          reject(new Error(tail(stderr) || `the sign-in exited with code ${code}`))
        })
      })
    })
    return this.loginAttempt
  }

  /** The integrations the harness home holds, with the pending attempt. */
  async status(): Promise<HarnessStatusReply> {
    const { stdout } = await this.run(statusArgs())
    return {
      integrations: parseAuthList(stdout),
      pending: this.loginChild === null ? null : this.pending,
      failure: this.failure,
    }
  }

  /** Kill the pending login; an attempt that never ran cancels to false. */
  async cancel(): Promise<{ cancelled: boolean }> {
    const child = this.loginChild
    if (child === null) return { cancelled: false }
    child.kill()
    this.loginChild = null
    this.pending = null
    this.loginAttempt = null
    this.failure = null
    // The killed device flow writes no partial credential (measured on
    // v2.2.5: `auth list` reads empty after the kill), so there is nothing to
    // scrub beyond the kill itself.
    return { cancelled: true }
  }

  /**
   * Log out of one integration in the harness home: every stored account of
   * that integration is removed there, and the human's own store is never
   * named. `auth logout` needs the credential id outside a terminal, so the
   * status read supplies it; an integration with nothing stored is already
   * logged out and answers so.
   */
  async logout(integration: string): Promise<{ integration: string }> {
    const status = await this.status()
    const accounts =
      status.integrations.find((row) => row.id === integration)?.connections ?? []
    for (const account of accounts) {
      await this.run(logoutArgs(integration, account.id === '' ? account.label : account.id))
    }
    return { integration }
  }
}

/** Re-answer a harness call the core never sees. */
export async function answerHarness(
  call: HarnessCall,
  controller: HarnessController,
): Promise<HarnessStatusReply | HarnessLoginReply | { cancelled: boolean } | { integration: string }> {
  switch (call.method) {
    case 'harness.status':
      return controller.status()
    case 'harness.login':
      return controller.login()
    case 'harness.cancel':
      return controller.cancel()
    case 'harness.logout':
      return controller.logout(call.params.integration ?? 'opencode')
  }
}

export { isHarnessCall }

/** The last line of the child's stderr: the reason the face shows. */
function tail(stderr: string): string {
  const lines = stderr
    .replace(/\x1b\[[0-9;?]*[a-zA-Z]/g, '')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
  return lines[lines.length - 1] ?? ''
}
