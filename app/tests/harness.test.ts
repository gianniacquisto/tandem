import { existsSync, mkdtempSync, rmSync, statSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

import {
  ensureHarnessHome,
  HarnessController,
  harnessEnv,
  loginArgs,
  logoutArgs,
  parseAuthList,
  parseLoginCode,
  parseLoginUrl,
  parseServeHandshake,
  resolveHarnessBinary,
  serveArgs,
  statusArgs,
  tandemHomeDir,
  type HarnessChild,
  type HarnessRequest,
  type HarnessSpawn,
} from '../src/main/harness'
import { harnessPhase, harnessRows } from '../renderer/src/shell/harnessAuth'
import type { HarnessStatusReply } from '../src/shared/bridge'

/** `auth list --format json`, as the binary prints it: ids, names and the
 * stored accounts, never a secret (`auth export` is the verb that prints one
 * and nothing here calls it). */
const AUTH_LIST = JSON.stringify([
  {
    id: 'opencode',
    name: 'OpenCode Console',
    connections: [{ type: 'credential', id: 'cred_1', label: 'Personal', method: 'oauth' }],
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    connections: [{ type: 'credential', id: 'cred_2', label: 'Key', method: 'key' }],
  },
])

/** The device flow's stdout, as `auth login` prints it before it waits. */
const LOGIN_OUT = [
  'Connect an integration',
  'Enter code: FQCF-TLPL',
  'https://opencode.ai/console/device?user_code=FQCF-TLPL&client_id=opencode-cli',
  'Waiting for authorization...',
].join('\n')

/** The served API's handshake, as `serve --stdio --port 0` prints it first. */
const SERVE_HANDSHAKE = '{"url":"http://127.0.0.1:4096"}'

/** One HTTP call the key sign-in made. */
type RequestCall = {
  url: string
  method: string
  headers: Record<string, string>
  body?: string
}

/** The request seam the key sign-in drives: calls recorded, answers in order. */
function fakeRequest(answers: Array<{ status: number; body: string }>) {
  const calls: RequestCall[] = []
  const request: HarnessRequest = (url, init) => {
    calls.push({ url, method: init.method, headers: init.headers, body: init.body })
    return Promise.resolve(answers.shift() ?? { status: 204, body: '' })
  }
  return { request, calls }
}

/** A controller over a fake spawn, with a request seam that answers nothing. */
function fakeController(
  driven: { emitAuthList: () => string; children: ReturnType<typeof fakeChild>[] },
  answers: Array<{ status: number; body: string }> = [],
) {
  const { spawn, calls: spawnCalls } = fakeSpawn(driven)
  const { request, calls: requestCalls } = fakeRequest(answers)
  const root = mkdtempSync(join(tmpdir(), 'tandem-harness-'))
  const controller = new HarnessController({
    spawn,
    openUrl: () => {},
    tandemHome: root,
    // The override pins the binary name, so the assertion holds on win32 too,
    // where the resolver would otherwise answer `opencode.exe`.
    env: { TANDEM_OPENCODE_BIN: 'opencode' },
    request,
  })
  return { controller, spawnCalls, requestCalls, root }
}

function statusReply(overrides: Partial<HarnessStatusReply> = {}): HarnessStatusReply {
  return {
    integrations: [],
    pending: null,
    failure: null,
    ...overrides,
  }
}

/** A child the test drives: stdout on demand, exit on demand. */
function fakeChild() {
  const stdout: Array<(chunk: string) => void> = []
  const stderr: Array<(chunk: string) => void> = []
  const exit: Array<(code: number | null) => void> = []
  let killed = false
  const child: HarnessChild = {
    onStdout: (listener) => stdout.push(listener),
    onStderr: (listener) => stderr.push(listener),
    onExit: (listener) => exit.push(listener),
    kill: () => {
      killed = true
    },
  }
  return {
    child,
    killed: () => killed,
    emitStdout: (chunk: string) => stdout.forEach((listener) => listener(chunk)),
    emitStderr: (chunk: string) => stderr.forEach((listener) => listener(chunk)),
    emitExit: (code: number | null) => exit.forEach((listener) => listener(code)),
  }
}

function fakeSpawn(driven: { emitAuthList: () => string; children: ReturnType<typeof fakeChild>[] }) {
  const calls: Array<{ binary: string; args: string[] }> = []
  const spawn: HarnessSpawn = (binary, args) => {
    calls.push({ binary, args })
    if (args[0] === 'auth' && args[1] === 'list') {
      const standalone = fakeChild()
      queueMicrotask(() => {
        standalone.emitStdout(driven.emitAuthList())
        standalone.emitExit(0)
      })
      return standalone.child
    }
    if (args[0] === 'auth' && args[1] === 'logout') {
      const done = fakeChild()
      queueMicrotask(() => done.emitExit(0))
      return done.child
    }
    const live = fakeChild()
    driven.children.push(live)
    return live.child
  }
  return { calls, spawn }
}

describe('the harness environment', () => {
  it('roots every path the binary reads in the harness home, per ADR 0038', () => {
    expect(harnessEnv('/tandem/here')).toEqual({
      XDG_DATA_HOME: join('/tandem/here', 'harness', 'data'),
      XDG_CACHE_HOME: join('/tandem/here', 'harness', 'cache'),
      XDG_CONFIG_HOME: join('/tandem/here', 'harness', 'config'),
      XDG_STATE_HOME: join('/tandem/here', 'harness', 'state'),
      TMPDIR: join('/tandem/here', 'harness', 'tmp'),
      OPENCODE_DB: join('/tandem/here', 'harness', 'opencode.db'),
      OPENCODE_CONFIG_DIR: join('/tandem/here', 'harness', 'config'),
    })
  })

  it('leaves HOME alone, so the harness keeps the human shell, git and keychain', () => {
    expect(harnessEnv('/tandem/here')).not.toHaveProperty('HOME')
  })

  it('creates every directory root the env names, and never the database file', () => {
    const root = mkdtempSync(join(tmpdir(), 'tandem-harness-home-'))
    try {
      ensureHarnessHome(root)
      const env = harnessEnv(root)
      for (const dir of [
        root,
        join(root, 'harness'),
        env.XDG_DATA_HOME,
        env.XDG_CACHE_HOME,
        env.XDG_CONFIG_HOME,
        env.XDG_STATE_HOME,
        env.TMPDIR,
      ]) {
        expect(statSync(dir).isDirectory(), dir).toBe(true)
      }
      // OPENCODE_DB is a file path in the env, not a root: creating it as a
      // directory would be the binary's job to fail on, not the app's.
      expect(existsSync(env.OPENCODE_DB)).toBe(false)
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('names the binary: override, then install locations, then the PATH', () => {
    const present = new Set(['/opt/homebrew/bin/opencode', '/usr/local/bin/opencode'])
    const exists = (path: string): boolean => present.has(path)
    // The platform is explicit, so the unix order holds on a Windows runner
    // too, where the default would be win32.
    expect(resolveHarnessBinary({ TANDEM_OPENCODE_BIN: '/opt/opencode' }, exists, 'darwin')).toBe(
      '/opt/opencode',
    )
    // The home candidate comes from `join`, like the resolver builds it, so
    // the order holds on win32 too (its separators, not posix ones).
    const homeCandidate = join('/Users/t', '.opencode', 'bin', 'opencode')
    expect(
      resolveHarnessBinary({ HOME: '/Users/t' }, (path) => path === homeCandidate, 'darwin'),
    ).toBe(homeCandidate)
    expect(resolveHarnessBinary({ HOME: '/Users/t' }, exists, 'darwin')).toBe(
      '/opt/homebrew/bin/opencode',
    )
    expect(resolveHarnessBinary({}, () => false, 'darwin')).toBe('opencode')
    expect(resolveHarnessBinary({ TANDEM_OPENCODE_BIN: '  ' }, () => false, 'darwin')).toBe(
      'opencode',
    )
    expect(resolveHarnessBinary({ HOME: '' }, () => false, 'darwin')).toBe('opencode')
  })

  it('names the .exe on win32: override, then the Windows installs, then the PATH', () => {
    const profile = join('C:', 'Users', 't')
    const env = {
      USERPROFILE: profile,
      LOCALAPPDATA: join(profile, 'AppData', 'Local'),
      APPDATA: join(profile, 'AppData', 'Roaming'),
    }
    // Each expectation comes from `join`, like the resolver builds it, so the
    // order holds on every runner (its separators, not posix ones).
    const homeBin = join(profile, '.opencode', 'bin', 'opencode.exe')
    const programBin = join(
      profile,
      'AppData',
      'Local',
      'Programs',
      'opencode',
      'opencode.exe',
    )
    const npmBin = join(
      profile,
      'AppData',
      'Roaming',
      'npm',
      'node_modules',
      'opencode-ai',
      'bin',
      'opencode.exe',
    )
    const present = (paths: string[]) => {
      const hits = new Set(paths)
      return (path: string): boolean => hits.has(path)
    }
    expect(
      resolveHarnessBinary(
        { ...env, TANDEM_OPENCODE_BIN: 'D:\\tools\\opencode.exe' },
        () => false,
        'win32',
      ),
    ).toBe('D:\\tools\\opencode.exe')
    expect(resolveHarnessBinary({ ...env, TANDEM_OPENCODE_BIN: '  ' }, present([homeBin]), 'win32')).toBe(
      homeBin,
    )
    expect(resolveHarnessBinary(env, present([homeBin, programBin, npmBin]), 'win32')).toBe(
      homeBin,
    )
    expect(resolveHarnessBinary(env, present([programBin, npmBin]), 'win32')).toBe(programBin)
    expect(resolveHarnessBinary(env, present([npmBin]), 'win32')).toBe(npmBin)
    expect(resolveHarnessBinary(env, () => false, 'win32')).toBe('opencode.exe')
    expect(
      resolveHarnessBinary({ USERPROFILE: '', LOCALAPPDATA: '', APPDATA: '' }, () => false, 'win32'),
    ).toBe('opencode.exe')
  })

  it('resolves the Tandem home the way the shell does', () => {
    const dev = { packaged: false, appPath: '/repo/app', homeDir: '/Users/tester' }
    expect(tandemHomeDir({ ...dev, env: {} })).toBe(
      join('/repo/app', '..', 'Fixtures', 'port', 'Home'),
    )
    expect(tandemHomeDir({ ...dev, env: { TANDEM_HOME: '/tmp/t' } })).toBe('/tmp/t')
    expect(
      tandemHomeDir({ packaged: true, appPath: '/x', homeDir: '/Users/tester', env: {} }),
    ).toBe(join('/Users/tester', '.tandem'))
  })
})

describe('the auth list read', () => {
  it('parses the per-integration rows, secrets never included', () => {
    const integrations = parseAuthList(AUTH_LIST)
    expect(integrations.map((row) => row.id)).toEqual(['opencode', 'openrouter'])
    expect(integrations[0]).toEqual({
      id: 'opencode',
      name: 'OpenCode Console',
      connections: [{ id: 'cred_1', label: 'Personal', method: 'oauth' }],
    })
  })

  it('reads an empty list as unsigned, and refuses a frame that is not a list', () => {
    expect(parseAuthList('[]')).toEqual([])
    expect(() => parseAuthList('not json')).toThrow()
    expect(() => parseAuthList('{"id": "opencode"}')).toThrow()
  })

  it('runs every read against the harness home, never the live service', () => {
    expect(statusArgs()).toEqual(['auth', 'list', '--format', 'json', '--standalone'])
    expect(loginArgs()).toEqual(['auth', 'login', 'opencode', '--method', 'device', '--standalone'])
    expect(loginArgs('mistral')).toEqual([
      'auth',
      'login',
      'mistral',
      '--method',
      'device',
      '--standalone',
    ])
    expect(serveArgs()).toEqual(['serve', '--stdio', '--port', '0'])
    expect(logoutArgs('opencode', 'cred_1')).toEqual([
      'auth',
      'logout',
      'opencode',
      'cred_1',
      '--standalone',
    ])
  })

  it('reads the served loopback url from the handshake line, per ADR 0038', () => {
    expect(parseServeHandshake(SERVE_HANDSHAKE)).toBe('http://127.0.0.1:4096')
    expect(() => parseServeHandshake('not json')).toThrow()
    expect(() => parseServeHandshake('{"port":4096}')).toThrow()
  })

  it('finds the device URL and the code in the login output', () => {
    expect(parseLoginUrl(LOGIN_OUT)).toBe(
      'https://opencode.ai/console/device?user_code=FQCF-TLPL&client_id=opencode-cli',
    )
    expect(parseLoginCode(LOGIN_OUT)).toBe('FQCF-TLPL')
    expect(parseLoginUrl('nothing yet')).toBeNull()
    expect(parseLoginCode('nothing yet')).toBeNull()
  })
})

describe('the sign-in lifecycle', () => {
  it('opens the device URL and reports the pending attempt until it lands', async () => {
    const driven = { emitAuthList: () => '[]', children: [] as ReturnType<typeof fakeChild>[] }
    const { spawn, calls } = fakeSpawn(driven)
    const opened: string[] = []
    const root = mkdtempSync(join(tmpdir(), 'tandem-harness-'))
    try {
      const controller = new HarnessController({
        spawn,
        openUrl: (url) => opened.push(url),
        tandemHome: root,
        env: { TANDEM_OPENCODE_BIN: 'opencode' },
        request: () => Promise.resolve({ status: 204, body: '' }),
      })

      const started = controller.login()
      await new Promise((resolve) => setTimeout(resolve, 0))
      driven.children[0]?.emitStdout(LOGIN_OUT)
      const attempt = await started
      if (!('url' in attempt)) throw new Error('the device flow answers a URL, not a receipt')

      expect(attempt.url).toContain('https://opencode.ai/console/device?')
      expect(attempt.code).toBe('FQCF-TLPL')
      expect(opened).toEqual([attempt.url])
      expect(calls[0]).toMatchObject({ binary: 'opencode', args: loginArgs() })

      const pending = await controller.status()
      expect(pending.pending?.url).toBe(attempt.url)
      expect(pending.integrations).toEqual([])

      driven.emitAuthList = () => AUTH_LIST
      driven.children[0]?.emitExit(0)
      const stored = await controller.status()
      expect(stored.pending).toBeNull()
      expect(stored.integrations.map((row) => row.id)).toEqual(['opencode', 'openrouter'])
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('cancels the pending attempt and leaves no partial credential', async () => {
    const driven = { emitAuthList: () => '[]', children: [] as ReturnType<typeof fakeChild>[] }
    const { spawn } = fakeSpawn(driven)
    const root = mkdtempSync(join(tmpdir(), 'tandem-harness-'))
    try {
      const controller = new HarnessController({
        spawn,
        openUrl: () => {},
        tandemHome: root,
        env: {},
        request: () => Promise.resolve({ status: 204, body: '' }),
      })

      const started = controller.login()
      await new Promise((resolve) => setTimeout(resolve, 0))
      driven.children[0]?.emitStdout(LOGIN_OUT)
      await started

      const cancelled = await controller.cancel()
      expect(cancelled).toEqual({ cancelled: true })
      expect(driven.children[0]?.killed()).toBe(true)

      const after = await controller.status()
      expect(after.pending).toBeNull()
      expect(after.integrations).toEqual([])
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('reports a login that exits before a URL as a failure, not a hang', async () => {
    const driven = { emitAuthList: () => '[]', children: [] as ReturnType<typeof fakeChild>[] }
    const { spawn } = fakeSpawn(driven)
    const root = mkdtempSync(join(tmpdir(), 'tandem-harness-'))
    try {
      const controller = new HarnessController({
        spawn,
        openUrl: () => {},
        tandemHome: root,
        env: {},
        request: () => Promise.resolve({ status: 204, body: '' }),
      })

      const started = controller.login()
      await new Promise((resolve) => setTimeout(resolve, 0))
      driven.children[0]?.emitStderr('no such integration')
      driven.children[0]?.emitExit(1)
      await expect(started).rejects.toThrow('no such integration')
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('logs out of the Console integration in the harness home only', async () => {
    const driven = { emitAuthList: () => AUTH_LIST, children: [] as ReturnType<typeof fakeChild>[] }
    const { spawn, calls } = fakeSpawn(driven)
    const root = mkdtempSync(join(tmpdir(), 'tandem-harness-'))
    try {
      const controller = new HarnessController({
        spawn,
        openUrl: () => {},
        tandemHome: root,
        env: {},
        request: () => Promise.resolve({ status: 204, body: '' }),
      })

      const receipt = await controller.logout('opencode')
      expect(receipt).toEqual({ integration: 'opencode' })
      expect(calls.map((call) => call.args)).toContainEqual(
        logoutArgs('opencode', 'cred_1'),
      )
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })
})

describe('the sign-in phase', () => {
  it('is signed out with no Console row, pending while the browser waits', () => {
    expect(harnessPhase(statusReply())).toBe('signed-out')
    expect(
      harnessPhase(
        statusReply({ pending: { url: 'https://example.test/device', code: 'AAAA-BBBB' } }),
      ),
    ).toBe('pending')
  })

  it('is stored when the Console row lands, a failure when the attempt dies', () => {
    expect(harnessPhase(statusReply({ integrations: parseAuthList(AUTH_LIST) }))).toBe('stored')
    expect(harnessPhase(statusReply({ failure: 'no such integration' }))).toBe('failure')
  })

  it('is stored when any integration holds an account, not only the Console', () => {
    const keyOnly = JSON.stringify([
      { id: 'mistral', name: 'Mistral', connections: [{ id: 'cred_9', label: 'Mistral', method: 'key' }] },
    ])
    expect(harnessPhase(statusReply({ integrations: parseAuthList(keyOnly) }))).toBe('stored')
  })

  it('draws one row per integration, stored or not', () => {
    const rows = harnessRows(statusReply({ integrations: parseAuthList(AUTH_LIST) }))
    expect(rows.map((row) => row.id)).toEqual(['opencode', 'openrouter'])
    expect(rows[0]).toEqual({
      id: 'opencode',
      name: 'OpenCode Console',
      detail: 'Personal (oauth)',
      stored: true,
    })
    expect(harnessRows(statusReply())).toEqual([])
  })
})

describe('the key sign-in over the served loopback API', () => {
  it('spawns serve, warms the catalog, then posts the key to connect/key', async () => {
    const driven = { emitAuthList: () => '[]', children: [] as ReturnType<typeof fakeChild>[] }
    const { controller, spawnCalls, requestCalls, root } = fakeController(driven, [
      { status: 200, body: '[]' },
      { status: 204, body: '' },
    ])
    try {
      const started = controller.login({ integration: 'mistral', key: 'sk-test-secret' })
      await new Promise((resolve) => setTimeout(resolve, 0))
      expect(spawnCalls[0]).toMatchObject({ binary: 'opencode', args: serveArgs() })

      driven.children[0]?.emitStdout(`${SERVE_HANDSHAKE}\n`)
      const receipt = await started
      expect(receipt).toEqual({ stored: 'mistral' })

      expect(requestCalls.map((call) => `${call.method} ${call.url}`)).toEqual([
        'GET http://127.0.0.1:4096/api/integration',
        'POST http://127.0.0.1:4096/api/integration/mistral/connect/key',
      ])
      const post = requestCalls[1]
      expect(post?.method).toBe('POST')
      expect(JSON.parse(post?.body ?? '')).toEqual({ key: 'sk-test-secret' })
      expect(post?.headers['Authorization']).toMatch(/^Basic /)
      expect(driven.children[0]?.killed()).toBe(true)

      const status = await controller.status()
      expect(status.pending).toBeNull()
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('refuses a key the server did not store, naming the status and never the key', async () => {
    const driven = { emitAuthList: () => '[]', children: [] as ReturnType<typeof fakeChild>[] }
    const { controller, requestCalls, root } = fakeController(driven, [
      { status: 200, body: '[]' },
      { status: 404, body: 'Integration not found: mistral' },
    ])
    try {
      const started = controller.login({ integration: 'mistral', key: 'sk-test-secret' })
      await new Promise((resolve) => setTimeout(resolve, 0))
      driven.children[0]?.emitStdout(`${SERVE_HANDSHAKE}\n`)
      await expect(started).rejects.toThrow('404')
      await started.catch((error: unknown) => {
        expect(String(error)).not.toContain('sk-test-secret')
      })
      expect(requestCalls).toHaveLength(2)
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('cancels a key sign-in before the handshake by killing the server child', async () => {
    const driven = { emitAuthList: () => '[]', children: [] as ReturnType<typeof fakeChild>[] }
    const { controller, root } = fakeController(driven)
    try {
      const started = controller.login({ integration: 'mistral', key: 'sk-test-secret' })
      await new Promise((resolve) => setTimeout(resolve, 0))
      const cancelled = await controller.cancel()
      expect(cancelled).toEqual({ cancelled: true })
      expect(driven.children[0]?.killed()).toBe(true)
      void started.catch(() => {})
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })
})
