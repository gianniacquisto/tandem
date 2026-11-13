import { spawn } from 'node:child_process'
import { cpSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { findPackagedApp } from './packaged.mjs'

/**
 * The artifact carries a core that serves the board: copy the fixture home to
 * a scratch directory, run the packaged `tandem serve` against it on an
 * ephemeral port, and require the board it answers to carry a revision.
 *
 * The Electron shell retires with the cut (ADR 0046), so this gate no longer
 * launches it: it proves the binary electron-builder put beside the resources,
 * which is the one core the artifact can reach.
 */
const appRoot = fileURLToPath(new URL('..', import.meta.url))
const app = findPackagedApp(join(appRoot, 'dist'), process.platform)

const home = mkdtempSync(join(tmpdir(), 'tandem-smoke-'))
cpSync(join(appRoot, '..', 'Fixtures', 'port', 'Home'), home, { recursive: true })

const child = spawn(app.binary, ['serve', '--port', '0'], {
  cwd: home,
  env: { ...process.env, TANDEM_HOME: home },
  stdio: ['ignore', 'pipe', 'pipe'],
})

let stdout = ''
let stderr = ''
child.stdout.setEncoding('utf8')
child.stdout.on('data', (chunk) => {
  stdout += chunk
  void check()
})
child.stderr.setEncoding('utf8')
child.stderr.on('data', (chunk) => {
  stderr += chunk
})

let settled = false

function finish(code, message) {
  if (settled) return
  settled = true
  clearTimeout(timer)
  child.kill('SIGKILL')
  rmSync(home, { recursive: true, force: true })
  if (code !== 0) {
    process.stderr.write(
      `the packaged core did not answer the board (exit ${code}), spawned ${app.binary}\n` +
        `stdout:\n${stdout}\nstderr:\n${stderr}\n${message ?? ''}`,
    )
    process.exit(1)
  }
  console.log(message)
}

async function check() {
  if (settled) return
  const listening = /listening on (http:\/\/[^\s]+)/.exec(stdout)
  if (listening === null) return
  settled = true
  try {
    const response = await fetch(`${listening[1]}/board`)
    const body = await response.text()
    const revision = /data-revision="(\d+)"/.exec(body)
    if (!response.ok || revision === null) {
      throw new Error(`the board answered ${response.status} without a revision`)
    }
    clearTimeout(timer)
    child.kill('SIGKILL')
    rmSync(home, { recursive: true, force: true })
    console.log(`the packaged core answered the board at revision ${revision[1]}`)
  } catch (error) {
    settled = false
    finish(1, `${error instanceof Error ? error.message : String(error)}\n`)
  }
}

const timer = setTimeout(() => finish(1, 'the board did not answer within 60 s\n'), 60_000)

child.on('error', (error) => finish(1, `${error.message}\n`))
child.on('close', (code) => finish(1, `the server exited ${code} before answering\n`))
