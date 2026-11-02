import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

import { harnessEnv } from '../src/main/harness'
import {
  compareVersions,
  HarnessServer,
  parseHandshake,
  parseHarnessVersion,
  resolveServerBinary,
  serveArgs,
  stampPath,
  stampVerdict,
  versionArgs,
  type HarnessServerChild,
  type HarnessServerSpawn,
} from '../src/main/harnessServer'

/** A child the test drives: stdout on demand, exit on demand, stdin end noted. */
function fakeChild() {
  const stdout: Array<(chunk: string) => void> = []
  const stderr: Array<(chunk: string) => void> = []
  const exit: Array<(code: number | null) => void> = []
  let killed = false
  let stdinEnded = false
  const child: HarnessServerChild = {
    onStdout: (listener) => stdout.push(listener),
    onStderr: (listener) => stderr.push(listener),
    onExit: (listener) => exit.push(listener),
    kill: () => {
      killed = true
    },
    endStdin: () => {
      stdinEnded = true
    },
  }
  return {
    child,
    killed: () => killed,
    stdinEnded: () => stdinEnded,
    emitStdout: (chunk: string) => stdout.forEach((listener) => listener(chunk)),
    emitStderr: (chunk: string) => stderr.forEach((listener) => listener(chunk)),
    emitExit: (code: number | null) => exit.forEach((listener) => listener(code)),
  }
}

type Driven = { version: string; children: ReturnType<typeof fakeChild>[] }

/** A spawn that answers `--version` from `driven` and hands back live serve children. */
function fakeSpawn(driven: Driven) {
  const calls: Array<{ binary: string; args: string[]; env: Record<string, string> }> = []
  const spawn: HarnessServerSpawn = (binary, args, options) => {
    calls.push({ binary, args, env: options.env })
    if (args[0] === '--version') {
      const probe = fakeChild()
      queueMicrotask(() => {
        probe.emitStdout(`opencode v${driven.version}\n`)
        probe.emitExit(0)
      })
      return probe.child
    }
    const live = fakeChild()
    driven.children.push(live)
    return live.child
  }
  return { calls, spawn }
}

function makeServer(root: string, driven: Driven, overrides: Record<string, unknown> = {}) {
  const { calls, spawn } = fakeSpawn(driven)
  const server = new HarnessServer({
    spawn,
    tandemHome: root,
    binary: '/bundled/opencode',
    baseEnv: { PATH: '/usr/bin' },
    ...overrides,
  })
  return { server, calls }
}

/** Start a server and drive its serve child to the handshake. */
async function startAndHandshake(
  root: string,
  driven: Driven,
  url = 'http://127.0.0.1:41234',
  overrides: Record<string, unknown> = {},
) {
  const made = makeServer(root, driven, overrides)
  const started = made.server.start()
  await new Promise((resolve) => setTimeout(resolve, 0))
  driven.children[0]?.emitStdout(`${JSON.stringify({ url })}\n`)
  const handle = await started
  return { ...made, handle }
}

function scratchHome(): string {
  return mkdtempSync(join(tmpdir(), 'tandem-harness-server-'))
}

describe('the harness binary lookup', () => {
  it('prefers the override, then the packaged copy, then the staged dev copy, then PATH', () => {
    const source = { packaged: true, platform: 'linux' as const, appPath: '/x', resourcesPath: '/r' }
    expect(resolveServerBinary({ ...source, env: { TANDEM_OPENCODE_BIN: '/opt/opencode' } })).toBe(
      '/opt/opencode',
    )
    expect(resolveServerBinary({ ...source, env: {} })).toBe(join('/r', 'harness', 'opencode'))
    expect(
      resolveServerBinary({ ...source, platform: 'win32', env: {} }),
    ).toBe(join('/r', 'harness', 'opencode.exe'))
    expect(
      resolveServerBinary({
        ...source,
        packaged: false,
        appPath: '/repo/app',
        env: {},
        exists: () => true,
      }),
    ).toBe(join('/repo/app', 'build', 'harness', 'opencode'))
    expect(
      resolveServerBinary({
        ...source,
        packaged: false,
        appPath: '/repo/app',
        env: {},
        exists: () => false,
      }),
    ).toBe('opencode')
  })

  it('spawns serve with stdin as the lifetime handle and port 0, and probes --version', () => {
    expect(serveArgs()).toEqual(['serve', '--stdio', '--port', '0'])
    expect(versionArgs()).toEqual(['--version'])
  })
})

describe('the version stamp', () => {
  it('reads `opencode v2.2.5` as 2.2.5 and nothing else', () => {
    expect(parseHarnessVersion('opencode v2.2.5\n')).toBe('2.2.5')
    expect(parseHarnessVersion('2.2.5')).toBeNull()
    expect(parseHarnessVersion('opencode v2.2')).toBeNull()
    expect(parseHarnessVersion('')).toBeNull()
  })

  it('compares x.y.z tuples, so 2.2.10 is newer than 2.2.5', () => {
    expect(compareVersions('2.2.5', '2.2.5')).toBe(0)
    expect(compareVersions('2.2.4', '2.2.5')).toBeLessThan(0)
    expect(compareVersions('2.2.5', '2.2.4')).toBeGreaterThan(0)
    expect(compareVersions('2.2.10', '2.2.5')).toBeGreaterThan(0)
    expect(compareVersions('2.10.0', '2.9.9')).toBeGreaterThan(0)
  })

  it('refuses only when the stamp is newer than the bundled binary', () => {
    expect(stampVerdict('9.9.9', '2.2.5')).toBe('refuse')
    expect(stampVerdict('2.2.5', '2.2.5')).toBe('stamp')
    expect(stampVerdict('2.2.4', '2.2.5')).toBe('stamp')
    expect(stampVerdict(null, '2.2.5')).toBe('stamp')
  })
})

describe('the handshake', () => {
  it('takes the loopback url from the first line', () => {
    expect(parseHandshake('{"url":"http://127.0.0.1:4096"}')).toBe('http://127.0.0.1:4096')
    expect(parseHandshake('{"url":"http://localhost:4096"}')).toBe('http://localhost:4096')
    expect(parseHandshake('{"url":"http://[::1]:4096"}')).toBe('http://[::1]:4096')
  })

  it('refuses anything off the loopback or not the handshake', () => {
    expect(parseHandshake('{"url":"http://0.0.0.0:4096"}')).toBeNull()
    expect(parseHandshake('{"url":"https://127.0.0.1:4096"}')).toBeNull()
    expect(parseHandshake('{"url":"http://example.test:4096"}')).toBeNull()
    expect(parseHandshake('{"url":42}')).toBeNull()
    expect(parseHandshake('starting up')).toBeNull()
  })
})

describe('the harness server', () => {
  it('spawns serve sealed in the harness home and answers with the handshake url only', async () => {
    const root = scratchHome()
    try {
      const driven: Driven = { version: '2.2.5', children: [] }
      const { server, calls, handle } = await startAndHandshake(root, driven)

      expect(handle).toEqual({ url: 'http://127.0.0.1:41234' })

      const serve = calls.find((call) => call.args[0] === 'serve')
      expect(serve?.binary).toBe('/bundled/opencode')
      expect(serve?.args).toEqual(serveArgs())
      expect(serve?.env).toMatchObject(harnessEnv(root))
      expect(serve?.env['OPENCODE_SERVER_PASSWORD']).toMatch(/^[0-9a-f]{48}$/)
      // The base environment carries no HOME here, and the seal adds none:
      // the harness runs tools in the human's name.
      expect(serve?.env).not.toHaveProperty('HOME')

      // The credential is held in the main process only: the header signs with
      // the spawned password, and the handle never carried it.
      const password = serve?.env['OPENCODE_SERVER_PASSWORD'] ?? ''
      expect(server.authorization()).toBe(
        `Basic ${Buffer.from(`opencode:${password}`).toString('base64')}`,
      )
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('mints a fresh credential per server', async () => {
    const first = scratchHome()
    const second = scratchHome()
    try {
      const drivenA: Driven = { version: '2.2.5', children: [] }
      const drivenB: Driven = { version: '2.2.5', children: [] }
      const a = await startAndHandshake(first, drivenA)
      const b = await startAndHandshake(second, drivenB)
      const passwordA = a.calls.find((call) => call.args[0] === 'serve')?.env[
        'OPENCODE_SERVER_PASSWORD'
      ]
      const passwordB = b.calls.find((call) => call.args[0] === 'serve')?.env[
        'OPENCODE_SERVER_PASSWORD'
      ]
      expect(passwordA).toMatch(/^[0-9a-f]{48}$/)
      expect(passwordA).not.toBe(passwordB)
    } finally {
      rmSync(first, { recursive: true, force: true })
      rmSync(second, { recursive: true, force: true })
    }
  })

  it('writes the version stamp on the first launch', async () => {
    const root = scratchHome()
    try {
      const driven: Driven = { version: '2.2.5', children: [] }
      await startAndHandshake(root, driven)
      expect(readFileSync(stampPath(root), 'utf8').trim()).toBe('2.2.5')
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('backs opencode.db up to opencode.db.<version> before a new version launches', async () => {
    const root = scratchHome()
    try {
      mkdirSync(join(root, 'harness'), { recursive: true })
      writeFileSync(join(root, 'harness', 'opencode.db'), 'db-bytes')
      writeFileSync(stampPath(root), '2.2.4\n')

      const driven: Driven = { version: '2.2.5', children: [] }
      await startAndHandshake(root, driven)

      expect(readFileSync(join(root, 'harness', 'opencode.db.2.2.5'), 'utf8')).toBe('db-bytes')
      expect(readFileSync(join(root, 'harness', 'opencode.db'), 'utf8')).toBe('db-bytes')
      expect(readFileSync(stampPath(root), 'utf8').trim()).toBe('2.2.5')
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('refuses to start when the stamp is newer than the bundled binary, spawning nothing', async () => {
    const root = scratchHome()
    try {
      mkdirSync(join(root, 'harness'), { recursive: true })
      writeFileSync(stampPath(root), '9.9.9\n')

      const driven: Driven = { version: '2.2.5', children: [] }
      const { server, calls } = makeServer(root, driven)
      await expect(server.start()).rejects.toThrow('9.9.9')

      expect(calls.filter((call) => call.args[0] === 'serve')).toHaveLength(0)
      expect(readFileSync(stampPath(root), 'utf8').trim()).toBe('9.9.9')
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('dies with its stdin: close ends the pipe and the exit code follows', async () => {
    const root = scratchHome()
    try {
      const driven: Driven = { version: '2.2.5', children: [] }
      const { server } = await startAndHandshake(root, driven)

      server.close()
      expect(driven.children[0]?.stdinEnded()).toBe(true)
      driven.children[0]?.emitExit(0)
      await expect(server.exited).resolves.toBe(0)
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('rejects when the child exits before the handshake', async () => {
    const root = scratchHome()
    try {
      const driven: Driven = { version: '2.2.5', children: [] }
      const { server } = makeServer(root, driven)
      const started = server.start()
      await new Promise((resolve) => setTimeout(resolve, 0))
      driven.children[0]?.emitExit(1)
      await expect(started).rejects.toThrow('before the handshake')
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('rejects and kills a handshake that is not loopback', async () => {
    const root = scratchHome()
    try {
      const driven: Driven = { version: '2.2.5', children: [] }
      const { server } = makeServer(root, driven)
      const started = server.start()
      await new Promise((resolve) => setTimeout(resolve, 0))
      driven.children[0]?.emitStdout('{"url":"http://0.0.0.0:4096"}\n')
      await expect(started).rejects.toThrow('no handshake')
      expect(driven.children[0]?.killed()).toBe(true)
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('gives up when no handshake arrives within the timeout', async () => {
    const root = scratchHome()
    try {
      const driven: Driven = { version: '2.2.5', children: [] }
      const { server } = makeServer(root, driven, { timeoutMs: 5 })
      await expect(server.start()).rejects.toThrow('no handshake')
      expect(driven.children[0]?.killed()).toBe(true)
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })
})
