import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'

import { RpcError, type Call, type Reply } from '../shared/bridge'

type Pending = {
  resolve: (reply: Reply) => void
  reject: (error: Error) => void
}

export type SpawnedClient = {
  client: RpcClient
  /** The child's exit code, or null when it could not be started. */
  exited: Promise<number | null>
}

function isEvent(frame: unknown): frame is { event: 'changed'; revision: number } {
  return (
    typeof frame === 'object' &&
    frame !== null &&
    (frame as { event?: unknown }).event === 'changed' &&
    typeof (frame as { revision?: unknown }).revision === 'number'
  )
}

type Answer = { id: number | null } & (
  | { result: Reply }
  | { error: { code: string; message: string } }
)

function isAnswer(frame: unknown): frame is Answer {
  if (typeof frame !== 'object' || frame === null) return false
  const candidate = frame as { id?: unknown; result?: unknown; error?: unknown }
  if (typeof candidate.id !== 'number' && candidate.id !== null) return false
  return 'result' in candidate || 'error' in candidate
}

/**
 * The app's end of `tandem rpc`: one JSON frame per line, calls correlated by
 * id, change events pushed whenever the store moves.
 *
 * The child is spawned with piped stdio and dies with the pipe: closing the
 * client ends stdin, and the Rust side treats end of stream as a normal exit
 * (PRT-11). Nothing outlives the window.
 */
export class RpcClient {
  private readonly child: ChildProcessWithoutNullStreams
  private readonly pending = new Map<number, Pending>()
  private readonly listeners = new Set<(revision: number) => void>()
  private buffer = ''
  private nextId = 1
  private closed = false

  private constructor(child: ChildProcessWithoutNullStreams) {
    this.child = child
    child.stdout.setEncoding('utf8')
    child.stdout.on('data', (chunk: string) => this.receive(chunk))
    child.on('close', () => this.failPending('The rpc stream closed.'))
    child.stdin.on('error', () => this.failPending('The rpc stream is not writable.'))
  }

  static spawn(
    command: string,
    args: string[],
    options: { env?: Record<string, string | undefined>; cwd?: string } = {},
  ): SpawnedClient {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: { ...process.env, ...options.env },
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true,
    })

    const exited = new Promise<number | null>((resolve) => {
      child.once('exit', (code) => resolve(code))
      child.once('error', () => resolve(null))
    })

    return { client: new RpcClient(child), exited }
  }

  call(call: Call): Promise<Reply> {
    if (this.closed) {
      return Promise.reject(
        new RpcError({ code: 'transport', message: 'The rpc stream is closed.' }),
      )
    }

    const id = this.nextId
    this.nextId += 1

    return new Promise<Reply>((resolve, reject) => {
      this.pending.set(id, { resolve, reject })
      this.child.stdin.write(`${JSON.stringify({ id, ...call })}\n`, (error) => {
        if (error === null || error === undefined) return
        this.pending.delete(id)
        reject(new RpcError({ code: 'transport', message: error.message }))
      })
    })
  }

  onChanged(listener: (revision: number) => void): () => void {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  listenerCount(): number {
    return this.listeners.size
  }

  /** End stdin: the child sees end of stream and exits on its own. */
  close(): void {
    if (this.closed) return
    this.closed = true
    this.child.stdin.end()
    this.failPending('The rpc stream is closed.')
  }

  private receive(chunk: string): void {
    this.buffer += chunk
    let boundary = this.buffer.indexOf('\n')
    while (boundary >= 0) {
      const line = this.buffer.slice(0, boundary).replace(/\r$/, '')
      this.buffer = this.buffer.slice(boundary + 1)
      if (line.trim() !== '') this.frame(line)
      boundary = this.buffer.indexOf('\n')
    }
  }

  /** One frame: an event, an answer, or a line this build cannot read. */
  private frame(line: string): void {
    let parsed: unknown
    try {
      parsed = JSON.parse(line)
    } catch {
      // A malformed line is the child's bug, not a reason to drop every call
      // waiting on the stream. It is skipped; the next frame is read normally.
      return
    }

    if (isEvent(parsed)) {
      for (const listener of this.listeners) listener(parsed.revision)
      return
    }
    if (!isAnswer(parsed)) return

    const pending = parsed.id === null ? undefined : this.pending.get(parsed.id)
    if (pending === undefined || parsed.id === null) return
    this.pending.delete(parsed.id)

    if ('error' in parsed) {
      pending.reject(new RpcError(parsed.error))
    } else {
      pending.resolve(parsed.result)
    }
  }

  private failPending(message: string): void {
    for (const pending of this.pending.values()) {
      pending.reject(new RpcError({ code: 'transport', message }))
    }
    this.pending.clear()
  }
}
