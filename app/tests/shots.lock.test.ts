import { spawn, type ChildProcessByStdio } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { hostname, tmpdir } from 'node:os'
import { join } from 'node:path'
import { createInterface } from 'node:readline'
import type { Readable } from 'node:stream'
import { fileURLToPath } from 'node:url'

import { ModuleKind, ScriptTarget, transpileModule } from 'typescript'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { acquireSeedLock, releaseSeedLock } from '../shots/lock'

const scratch: string[] = []

function scratchRoot(): string {
  const root = mkdtempSync(join(tmpdir(), 'tandem-shots-lock-'))
  scratch.push(root)
  return root
}

afterEach(() => {
  vi.restoreAllMocks()
  for (const root of scratch.splice(0)) rmSync(root, { recursive: true, force: true })
})

/** The module under test, transpiled so a child can run the real source. */
const LOCK_SOURCE = fileURLToPath(new URL('../shots/lock.ts', import.meta.url))

const HOLDER = `
import { acquireSeedLock, releaseSeedLock } from './lock.mjs'

const [root, label, holdMs] = process.argv.slice(2)
const lock = await acquireSeedLock({ root, waitBoundMs: 10_000 })
console.log('in:' + label)
await new Promise((resolve) => setTimeout(resolve, Number(holdMs)))
console.log('out:' + label)
releaseSeedLock(lock)
`

interface Holder {
  child: ChildProcessByStdio<null, Readable, Readable>
  log: string[]
  exited: Promise<number | null>
  stderr: () => string
}

/** One child run of the real lock module, holding the seed for `holdMs`. */
function spawnHolder(root: string, label: string, holdMs: number, log: string[] = []): Holder {
  const dir = scratchRoot()
  const { outputText } = transpileModule(readFileSync(LOCK_SOURCE, 'utf8'), {
    compilerOptions: { module: ModuleKind.ESNext, target: ScriptTarget.ES2022 },
  })
  writeFileSync(join(dir, 'lock.mjs'), outputText)
  writeFileSync(join(dir, 'hold.mjs'), HOLDER)

  const child = spawn(process.execPath, [join(dir, 'hold.mjs'), root, label, String(holdMs)], {
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  let stderr = ''
  child.stderr.on('data', (chunk: Buffer) => {
    stderr += chunk.toString()
  })
  createInterface({ input: child.stdout }).on('line', (line) => log.push(line))
  const exited = new Promise<number | null>((resolve) => child.on('exit', (code) => resolve(code)))
  return { child, log, exited, stderr: () => stderr }
}

function waitForLine(log: string[], line: string, timeoutMs = 5000): Promise<void> {
  return new Promise((resolve, reject) => {
    const started = Date.now()
    const timer = setInterval(() => {
      if (log.includes(line)) {
        clearInterval(timer)
        resolve()
      } else if (Date.now() - started > timeoutMs) {
        clearInterval(timer)
        reject(new Error(`no child logged ${line}; the log holds ${JSON.stringify(log)}`))
      }
    }, 10)
  })
}

function writeLock(root: string, pid: number, token: string): void {
  const path = join(root, '.lock')
  mkdirSync(path, { recursive: true })
  writeFileSync(
    join(path, 'owner.json'),
    `${JSON.stringify({ pid, host: hostname(), started: '2026-01-01T00:00:00.000Z', token })}\n`,
  )
}

function readOwner(root: string): { pid: number; host: string; started: string; token: string } {
  return JSON.parse(readFileSync(join(root, '.lock', 'owner.json'), 'utf8'))
}

describe('the shots seed lock', () => {
  it('two runs never hold the seed at once', async () => {
    const root = scratchRoot()
    const log: string[] = []

    const first = spawnHolder(root, 'A', 200, log)
    const second = spawnHolder(root, 'B', 200, log)

    expect(await first.exited).toBe(0)
    expect(await second.exited).toBe(0)
    expect([
      ['in:A', 'out:A', 'in:B', 'out:B'],
      ['in:B', 'out:B', 'in:A', 'out:A'],
    ]).toContainEqual(log)
  })

  it('a waiter is told the holder once and then takes the seed', async () => {
    const root = scratchRoot()
    const holder = spawnHolder(root, 'A', 300)
    await waitForLine(holder.log, 'in:A')

    const notices: string[] = []
    vi.spyOn(console, 'error').mockImplementation((...args: unknown[]) => {
      notices.push(args.map(String).join(' '))
    })

    const lock = await acquireSeedLock({ root, waitBoundMs: 10_000 })

    expect(holder.log).toContain('out:A')
    expect(notices).toHaveLength(1)
    expect(notices[0]).toContain(`pid ${holder.child.pid}`)
    expect(notices[0]).toContain(root)
    releaseSeedLock(lock)
    expect(await holder.exited).toBe(0)
  })

  it("a dead holder's lock is broken by the next run", async () => {
    const root = scratchRoot()
    const dead = spawn(process.execPath, ['-e', ''])
    const deadPid = dead.pid
    await new Promise((resolve) => dead.on('exit', resolve))
    expect(deadPid).toBeTypeOf('number')
    writeLock(root, deadPid as number, 'stale')

    const notices: string[] = []
    vi.spyOn(console, 'error').mockImplementation((...args: unknown[]) => {
      notices.push(args.map(String).join(' '))
    })

    const lock = await acquireSeedLock({ root, waitBoundMs: 1000 })

    expect(notices.join('\n')).toContain(`pid ${deadPid}`)
    expect(readOwner(root).pid).toBe(process.pid)
    expect(readOwner(root).token).toBe(lock.token)
    releaseSeedLock(lock)
  })

  it('a live holder is never broken and a stuck wait fails by name', async () => {
    const root = scratchRoot()
    writeLock(root, process.pid, 'live')

    await expect(acquireSeedLock({ root, waitBoundMs: 60 })).rejects.toThrow(new RegExp(`pid ${process.pid}`))

    expect(existsSync(join(root, '.lock', 'owner.json'))).toBe(true)
    expect(readOwner(root).token).toBe('live')
  })

  it('a release without the lock leaves it alone', async () => {
    const root = scratchRoot()
    const first = await acquireSeedLock({ root })
    rmSync(join(root, '.lock'), { recursive: true, force: true })
    const second = await acquireSeedLock({ root })

    releaseSeedLock(first)

    expect(readOwner(root).token).toBe(second.token)
    releaseSeedLock(second)
    expect(existsSync(join(root, '.lock'))).toBe(false)
  })
})
