import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

import {
  HarnessController,
  harnessEnv,
  loginArgs,
  logoutArgs,
  parseAuthList,
  parseLoginCode,
  parseLoginUrl,
  resolveHarnessBinary,
  statusArgs,
  tandemHomeDir,
  type HarnessChild,
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

  it('lets TANDEM_OPENCODE_BIN name the binary, defaulting to PATH', () => {
    expect(resolveHarnessBinary({ TANDEM_OPENCODE_BIN: '/opt/opencode' })).toBe('/opt/opencode')
    expect(resolveHarnessBinary({ TANDEM_OPENCODE_BIN: '  ' })).toBe('opencode')
    expect(resolveHarnessBinary({})).toBe('opencode')
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
    expect(logoutArgs('opencode', 'cred_1')).toEqual([
      'auth',
      'logout',
      'opencode',
      'cred_1',
      '--standalone',
    ])
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
        env: {},
      })

      const started = controller.login()
      await new Promise((resolve) => setTimeout(resolve, 0))
      driven.children[0]?.emitStdout(LOGIN_OUT)
      const attempt = await started

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
