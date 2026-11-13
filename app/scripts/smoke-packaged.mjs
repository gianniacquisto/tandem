import { spawn } from 'node:child_process'
import { cpSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { findPackagedApp } from './packaged.mjs'

/**
 * The artifact launches and answers: copy the fixture home to a scratch
 * directory, run the packaged app against it with `TANDEM_SMOKE`, and require
 * the revision the board it serves drew.
 *
 * The app runs from `dist/`, so the only core it can reach is the one
 * electron-builder put beside its resources; the workspace build is not in
 * this lookup. The shell spawns that core's `serve` verb (ADR 0046), so this
 * gate exercises the shell and the packaged binary together. Linux needs a
 * display, so CI runs this under `xvfb-run`.
 */
const appRoot = fileURLToPath(new URL('..', import.meta.url))
const app = findPackagedApp(join(appRoot, 'dist'), process.platform)

const home = mkdtempSync(join(tmpdir(), 'tandem-smoke-'))
cpSync(join(appRoot, '..', 'Fixtures', 'port', 'Home'), home, { recursive: true })

const args = process.platform === 'linux' ? ['--no-sandbox'] : []
const child = spawn(app.executable, args, {
  cwd: home,
  env: { ...process.env, TANDEM_HOME: home, TANDEM_SMOKE: '1' },
  stdio: ['ignore', 'pipe', 'pipe'],
})

let stdout = ''
let stderr = ''
child.stdout.setEncoding('utf8')
child.stdout.on('data', (chunk) => {
  stdout += chunk
})
child.stderr.setEncoding('utf8')
child.stderr.on('data', (chunk) => {
  stderr += chunk
})

const timer = setTimeout(() => child.kill('SIGKILL'), 60_000)

child.on('close', (code) => {
  clearTimeout(timer)
  rmSync(home, { recursive: true, force: true })

  const revision = /^revision (\d+)$/m.exec(stdout)
  if (code !== 0 || revision === null) {
    process.stderr.write(
      `the packaged app did not answer the board (exit ${code}), spawned ${app.binary}\n` +
        `stdout:\n${stdout}\nstderr:\n${stderr}\n`,
    )
    process.exit(1)
  }

  console.log(`the packaged app answered the board at revision ${revision[1]}`)
})
