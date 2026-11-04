import { randomBytes } from 'node:crypto'
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'

import { ensureHarnessHome, harnessEnv, resolveHarnessBinary, type HarnessChild } from './harness'

/**
 * The harness server's main-process half (ADR 0038, PRT-1): the long-lived
 * `opencode serve --stdio --port 0` child that runs the app's own agent loop.
 *
 * The server is spawned once per app run and dies with its stdin, like the
 * rpc child. The app reads the ephemeral url from the handshake on the first
 * stdout line and holds the credential: `OPENCODE_SERVER_PASSWORD` goes to the
 * child only, and the renderer never sees it. The main process signs each of
 * its own requests with `authorization()`; the renderer talks to the app, not
 * to the harness. No Electron import lives here: `spawn` is injected, so the
 * suite drives a fake child and the shell wires the real one.
 */

/** The sliver of the serve child the server needs, plus the stdin end. */
export type HarnessServerChild = HarnessChild & {
  /** Close stdin: the child's lifetime handle, it exits 0 on end of stream. */
  endStdin(): void
}

export type HarnessServerSpawn = (
  binary: string,
  args: string[],
  options: { env: Record<string, string>; cwd: string },
) => HarnessServerChild

export type HarnessBinarySource = {
  env: Record<string, string | undefined>
  packaged: boolean
  platform: NodeJS.Platform
  appPath: string
  resourcesPath: string
  exists?: (path: string) => boolean
}

/**
 * Where the shell finds the harness binary the launcher spawns (ADR 0038):
 * `TANDEM_OPENCODE_BIN` wins; a packaged app reads the copy bundled at
 * `<resourcesPath>/harness/opencode` when the bundle carries one, and
 * development reads the staged copy `harness:stage` wrote at
 * `build/harness/opencode`. When neither is there, the resolution is the
 * sign-in controller's own (`resolveHarnessBinary`, TAN-270): the install
 * locations first, because a GUI app's PATH is the launchd PATH and a bare
 * `opencode` fails there, and the PATH name last. Only the Linux bundle
 * embeds the harness today, so a packaged darwin app resolves exactly like
 * the sign-in face does.
 */
export function resolveServerBinary(source: HarnessBinarySource): string {
  const named = source.env['TANDEM_OPENCODE_BIN']
  if (named !== undefined && named.trim() !== '') return named
  const name = source.platform === 'win32' ? 'opencode.exe' : 'opencode'
  const exists = source.exists ?? existsSync
  if (source.packaged) {
    const bundled = join(source.resourcesPath, 'harness', name)
    if (exists(bundled)) return bundled
    return resolveHarnessBinary(source.env, exists, source.platform)
  }
  const staged = join(source.appPath, 'build', 'harness', name)
  if (exists(staged)) return staged
  return resolveHarnessBinary(source.env, exists, source.platform)
}

/** The serve invocation: stdin is the lifetime handle, port 0 leaves the port to the handshake. */
export function serveArgs(): string[] {
  return ['serve', '--stdio', '--port', '0']
}

/** The one-shot version probe: `opencode v2.2.5` on stdout. */
export function versionArgs(): string[] {
  return ['--version']
}

/** `opencode v2.2.5` -> `2.2.5`; anything else is not a version this build reads. */
export function parseHarnessVersion(output: string): string | null {
  const match = output.trim().match(/^opencode v(\d+\.\d+\.\d+)$/m)
  return match?.[1] ?? null
}

/** Tuple compare of two `x.y.z` versions: negative when `a` is older than `b`. */
export function compareVersions(a: string, b: string): number {
  const left = a.split('.').map((part) => Number.parseInt(part, 10))
  const right = b.split('.').map((part) => Number.parseInt(part, 10))
  for (let index = 0; index < 3; index += 1) {
    const a = left[index] ?? 0
    const b = right[index] ?? 0
    if (a !== b) return a - b
  }
  return 0
}

const LOOPBACK_HOSTS = new Set(['127.0.0.1', 'localhost', '[::1]'])

/**
 * The handshake line: `{"url":"http://127.0.0.1:<port>"}`. A url off the
 * loopback is refused, because the credential rides every request to it.
 */
export function parseHandshake(line: string): string | null {
  let parsed: unknown
  try {
    parsed = JSON.parse(line)
  } catch {
    return null
  }
  if (typeof parsed !== 'object' || parsed === null) return null
  const url = (parsed as { url?: unknown }).url
  if (typeof url !== 'string') return null
  let parsedUrl: URL
  try {
    parsedUrl = new URL(url)
  } catch {
    return null
  }
  if (parsedUrl.protocol !== 'http:' || !LOOPBACK_HOSTS.has(parsedUrl.hostname)) return null
  return url
}

export function stampPath(tandemHome: string): string {
  return join(tandemHome, 'harness', 'version')
}

/**
 * Where the CLI's rendezvous read looks for the embedded harness
 * (`rust/cli/src/main.rs::rendezvous_path`, TAN-276): the `event-stream`
 * record's `state/opencode/service.json` under the Tandem home. The file is
 * service-written, mode 0600, holding the handshake url and the loopback
 * credential as `{"url":..., "password":...}`; extra keys are ignored by the
 * reader. `serve --stdio` writes nothing, so the app writes it from the
 * handshake at start (TAN-296).
 */
export function rendezvousPath(tandemHome: string): string {
  return join(tandemHome, 'harness', 'state', 'opencode', 'service.json')
}

/** Write the rendezvous file the CLI reads: the url plus the credential, mode 0600. */
function writeRendezvous(tandemHome: string, url: string, password: string): void {
  const path = rendezvousPath(tandemHome)
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, JSON.stringify({ url, password }))
  chmodSync(path, 0o600)
}

function readStamp(tandemHome: string): string | null {
  try {
    const text = readFileSync(stampPath(tandemHome), 'utf8').trim()
    return text === '' ? null : text
  } catch {
    return null
  }
}

/**
 * The stamp's verdict before a launch (ADR 0038): a stamp newer than the
 * bundled binary refuses the start, because an older binary opening a newer
 * store is the failure mode; anything else launches and re-stamps.
 */
export function stampVerdict(stamp: string | null, bundled: string): 'refuse' | 'stamp' {
  if (stamp !== null && compareVersions(stamp, bundled) > 0) return 'refuse'
  return 'stamp'
}

export type HarnessServerDeps = {
  spawn: HarnessServerSpawn
  tandemHome: string
  binary: string
  baseEnv?: NodeJS.ProcessEnv
  timeoutMs?: number
}

/** What a started server hands back: the url only, never the credential. */
export type HarnessServerHandle = {
  url: string
}

const HANDSHAKE_TIMEOUT_MS = 30_000

/**
 * One serve run: the version stamp is checked before anything is spawned, the
 * database is backed up when the stamp moves, the handshake is read from
 * the first stdout line, and the rendezvous file is written from the
 * handshake plus the spawned credential so the CLI's no-flags
 * `session end --usage-from-harness` finds the loopback server (TAN-296).
 */
export class HarnessServer {
  private readonly deps: HarnessServerDeps
  private child: HarnessServerChild | null = null
  private started: Promise<HarnessServerHandle> | null = null
  private password: string | null = null
  /** The serve child's exit code, or null when it could not be started. */
  exited: Promise<number | null> | null = null

  constructor(deps: HarnessServerDeps) {
    this.deps = deps
  }

  private baseEnv(): Record<string, string> {
    const base: Record<string, string> = {}
    for (const [key, value] of Object.entries(this.deps.baseEnv ?? {})) {
      if (value !== undefined) base[key] = value
    }
    return base
  }

  private spawnEnv(password: string | null): Record<string, string> {
    // The sealed table is the closed `HarnessEnv` struct; the password is the
    // one key the struct does not name, so the spawn env is the wide record.
    const env: Record<string, string> = { ...this.baseEnv(), ...harnessEnv(this.deps.tandemHome) }
    if (password !== null) env['OPENCODE_SERVER_PASSWORD'] = password
    return env
  }

  /** The bundled binary's own version, asked once per start. */
  private probeVersion(): Promise<string> {
    return new Promise((resolve, reject) => {
      let child: HarnessServerChild
      try {
        ensureHarnessHome(this.deps.tandemHome)
        child = this.deps.spawn(this.deps.binary, versionArgs(), {
          env: this.spawnEnv(null),
          cwd: join(this.deps.tandemHome, 'harness'),
        })
      } catch (error) {
        reject(error instanceof Error ? error : new Error('the harness binary did not start'))
        return
      }
      let stdout = ''
      child.onStdout((chunk) => {
        stdout += chunk
      })
      child.onExit((code) => {
        const version = code === 0 ? parseHarnessVersion(stdout) : null
        if (version === null) {
          reject(new Error(`the harness binary did not report its version (exit ${code})`))
        } else {
          resolve(version)
        }
      })
    })
  }

  /** Back the database up and write the stamp before a new version's first launch. */
  private restamp(bundled: string): void {
    const root = join(this.deps.tandemHome, 'harness')
    const database = join(root, 'opencode.db')
    if (existsSync(database)) copyFileSync(database, join(root, `opencode.db.${bundled}`))
    writeFileSync(stampPath(this.deps.tandemHome), `${bundled}\n`)
  }

  private launch(): Promise<HarnessServerHandle> {
    return this.probeVersion().then((bundled) => {
      const stamp = readStamp(this.deps.tandemHome)
      if (stampVerdict(stamp, bundled) === 'refuse') {
        throw new Error(
          `the harness home was written by version ${stamp}, newer than the bundled ${bundled}; refusing to start`,
        )
      }
      if (stamp !== bundled) this.restamp(bundled)
      return this.serve()
    })
  }

  private serve(): Promise<HarnessServerHandle> {
    const password = randomBytes(24).toString('hex')
    this.password = password
    return new Promise((resolve, reject) => {
      let child: HarnessServerChild
      try {
        ensureHarnessHome(this.deps.tandemHome)
        child = this.deps.spawn(this.deps.binary, serveArgs(), {
          env: this.spawnEnv(password),
          cwd: join(this.deps.tandemHome, 'harness'),
        })
      } catch (error) {
        reject(error instanceof Error ? error : new Error('the harness binary did not start'))
        return
      }
      this.child = child
      this.exited = new Promise((resolveExit) => {
        child.onExit((code) => resolveExit(code))
      })

      let buffer = ''
      let settled = false
      const fail = (message: string) => {
        if (settled) return
        settled = true
        clearTimeout(timer)
        child.kill()
        reject(new Error(message))
      }
      const timer = setTimeout(() => {
        fail(`no handshake on stdout within ${this.deps.timeoutMs ?? HANDSHAKE_TIMEOUT_MS}ms`)
      }, this.deps.timeoutMs ?? HANDSHAKE_TIMEOUT_MS)

      child.onStdout((chunk) => {
        if (settled) return
        buffer += chunk
        let boundary = buffer.indexOf('\n')
        while (boundary >= 0) {
          const line = buffer.slice(0, boundary).replace(/\r$/, '')
          buffer = buffer.slice(boundary + 1)
          boundary = buffer.indexOf('\n')
          if (line.trim() === '') continue
          const url = parseHandshake(line)
          if (url === null) {
            fail(`the harness printed no handshake: ${line.slice(0, 120)}`)
            return
          }
          try {
            writeRendezvous(this.deps.tandemHome, url, password)
          } catch {
            fail('the harness rendezvous file could not be written')
            return
          }
          settled = true
          clearTimeout(timer)
          resolve({ url })
          return
        }
      })
      child.onExit((code) => {
        fail(`the harness exited with code ${code} before the handshake`)
      })
    })
  }

  /** Start the serve child once; the handle carries the url, never the credential. */
  start(): Promise<HarnessServerHandle> {
    if (this.started === null) this.started = this.launch()
    return this.started
  }

  /**
   * The `Authorization` header value for the main process's own requests:
   * `Basic base64("opencode:<password>")`. It stays in the main process; the
   * renderer talks to the app, not to the harness (ADR 0038).
   */
  authorization(): string {
    if (this.password === null) throw new Error('the harness server is not running')
    return `Basic ${Buffer.from(`opencode:${this.password}`).toString('base64')}`
  }

  /** End stdin: the serve child sees end of stream and exits 0 on its own. */
  close(): void {
    this.child?.endStdin()
  }
}
