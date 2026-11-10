import { randomUUID } from 'node:crypto'
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { hostname } from 'node:os'
import { join } from 'node:path'

/**
 * The one owner of a shots run's scratch seed.
 *
 * Every run reads the same fixed scratch path (`home.ts`), because the Settings
 * face prints that path and the picture is committed. One fixed path means two
 * runs on one machine would copy over each other's seed mid-flight, so a run
 * takes an exclusive lock on the seed root and a second run waits for it rather
 * than reading or writing the same seed (`docs/screenshots/README.md`).
 *
 * The lock is a directory: `mkdirSync` is the take, and `EEXIST` is "somebody
 * holds it". A directory rather than a file because creating it is atomic on
 * every platform and no content write can race the take. The owner record is
 * written inside it for the waiter to name and for staleness to be decided.
 *
 * This module imports node builtins only and takes its root as an argument, so
 * a test can spawn a child that runs the real module (`shots.lock.test.ts`) and
 * no machine-wide path is touched.
 */

/** The lock directory's name under a seed root. */
export const LOCK_DIR = '.lock'

/** The owner record's name inside the lock directory. */
export const OWNER_FILE = 'owner.json'

/** How long a run waits for a busy seed before it gives up. */
export const WAIT_BOUND_MS = 45 * 60_000

/** How often a waiting run looks again. */
const POLL_MS = 50

/** How often a waiting run repeats its own progress. */
const NOTICE_MS = 60_000

export interface SeedLock {
  readonly root: string
  readonly path: string
  readonly token: string
}

interface Owner {
  pid: number
  host: string
  started: string
  token: string
}

export interface AcquireOptions {
  /** The seed root the lock guards; created if missing. */
  root: string
  /** How long to wait for a live holder before failing; a test shortens it. */
  waitBoundMs?: number
}

/**
 * Take the seed lock, waiting while a live holder has it. Returns the handle
 * `releaseSeedLock` matches by token.
 *
 * Contention waits: a builder's `npm run shots` must not fail because another
 * builder's is running. The waiter names the holder once, repeats its own
 * elapsed wait once a minute, and gives up at the bound with an error naming
 * the holder. A holder on this machine whose pid is gone is stale: it is
 * broken with a notice and the take retried, so a killed run costs the next
 * run a printed line rather than a seed read across two runs.
 */
export async function acquireSeedLock(options: AcquireOptions): Promise<SeedLock> {
  const root = options.root
  const bound = options.waitBoundMs ?? WAIT_BOUND_MS
  mkdirSync(root, { recursive: true })

  const path = join(root, LOCK_DIR)
  const owner: Owner = {
    pid: process.pid,
    host: hostname(),
    started: new Date().toISOString(),
    token: randomUUID(),
  }
  const waitStart = Date.now()
  let announced = false
  let lastNotice = waitStart

  for (;;) {
    if (take(path, owner)) return { root, path, token: owner.token }

    const holder = readOwner(path)
    if (holder !== null && isStale(holder)) {
      breakStale(path, holder)
      continue
    }

    const elapsed = Date.now() - waitStart
    if (elapsed >= bound) {
      throw new Error(
        `Timed out after ${formatWait(elapsed)} waiting for the shots seed lock at ${root}, held by ${describe(holder)}.`,
      )
    }

    if (!announced) {
      console.error(`Waiting for the shots seed lock at ${root}: held by ${describe(holder)}.`)
      announced = true
      lastNotice = Date.now()
    } else if (Date.now() - lastNotice >= NOTICE_MS) {
      console.error(
        `Still waiting for the shots seed lock at ${root} after ${formatWait(elapsed)}: held by ${describe(holder)}.`,
      )
      lastNotice = Date.now()
    }

    await delay(Math.min(POLL_MS, Math.max(1, bound - elapsed)))
  }
}

/**
 * Release the lock this handle took: matched by the token it wrote, idempotent,
 * and a handle that acquired nothing releases nothing. A lock somebody else
 * took (a stale break, a second run) is left alone.
 */
export function releaseSeedLock(lock: SeedLock | null | undefined): void {
  if (lock === null || lock === undefined) return
  const current = readOwner(lock.path)
  if (current === null || current.token !== lock.token) return
  rmSync(lock.path, { recursive: true, force: true })
}

/** Take the lock by creating its directory, then write the owner record. */
function take(path: string, owner: Owner): boolean {
  try {
    mkdirSync(path)
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'EEXIST') return false
    throw error
  }

  try {
    writeFileSync(join(path, OWNER_FILE), `${JSON.stringify(owner)}\n`)
  } catch (error) {
    rmSync(path, { recursive: true, force: true })
    throw error
  }
  return true
}

function readOwner(path: string): Owner | null {
  try {
    const value: unknown = JSON.parse(readFileSync(join(path, OWNER_FILE), 'utf8'))
    return isOwner(value) ? value : null
  } catch {
    return null
  }
}

function isOwner(value: unknown): value is Owner {
  if (typeof value !== 'object' || value === null) return false
  const record = value as Record<string, unknown>
  return (
    typeof record['pid'] === 'number' &&
    Number.isInteger(record['pid']) &&
    record['pid'] > 0 &&
    typeof record['host'] === 'string' &&
    typeof record['started'] === 'string' &&
    typeof record['token'] === 'string'
  )
}

/** Stale means a lock on this machine whose pid is no longer running. */
function isStale(owner: Owner): boolean {
  return owner.host === hostname() && !isAlive(owner.pid)
}

function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0)
    return true
  } catch (error) {
    // EPERM means the process exists and belongs to somebody else.
    return (error as NodeJS.ErrnoException).code !== 'ESRCH'
  }
}

/**
 * Break a stale lock. The path removed is the lock path this module built from
 * the root, never one read out of the record; the record only decides whether
 * the break is allowed. Re-read just before removing, so a lock another waiter
 * already replaced is left alone.
 */
function breakStale(path: string, stale: Owner): void {
  const current = readOwner(path)
  if (current === null || current.token !== stale.token) return
  rmSync(path, { recursive: true, force: true })
  console.error(
    `Broke the stale shots seed lock at ${path}: pid ${stale.pid} on ${stale.host} is not running (started ${stale.started}).`,
  )
}

function describe(owner: Owner | null): string {
  if (owner === null) return 'an unknown owner (its record is not readable yet)'
  return `pid ${owner.pid} on ${owner.host} since ${owner.started}`
}

function formatWait(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  const seconds = Math.round(ms / 1000)
  if (seconds < 60) return `${seconds}s`
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}
