import { spawn, type ChildProcess } from 'node:child_process'
import { join } from 'node:path'

import { app, BrowserWindow, session } from 'electron'

import { resolveBinary } from './binary'
import { contentSecurityPolicy } from '../shared/security'
import { windowOptions } from './window'

/**
 * The shell's process: one window and one `tandem serve` child behind it.
 *
 * The child is spawned here and dies with the app, so the shell and the core
 * cannot drift apart. The window loads the board the child serves, because
 * `tandem rpc` retired with the cut (ADR 0046) and `serve` is the core's only
 * UI door; a later cut deletes this shell and the renderer with it.
 *
 * Development reads the fixture home under `Fixtures/` unless `TANDEM_HOME`
 * names another, because the Swift app still owns the real `~/.tandem`
 * (`docs/port.md`, How the work lands).
 */
let serving: ChildProcess | null = null
let shell: BrowserWindow | null = null

function homeEnvironment(): Record<string, string> {
  const named = process.env['TANDEM_HOME']
  if (named !== undefined && named.trim() !== '') return { TANDEM_HOME: named }
  if (app.isPackaged) return {}
  return { TANDEM_HOME: join(app.getAppPath(), '..', 'Fixtures', 'port', 'Home') }
}

/**
 * `tandem serve --port 0`: the core binds an ephemeral loopback port and prints
 * the address it actually bound, which is the URL the window loads.
 */
function serve(): Promise<string> {
  const command = resolveBinary({
    env: process.env,
    packaged: app.isPackaged,
    platform: process.platform,
    appPath: app.getAppPath(),
    resourcesPath: process.resourcesPath,
  })

  // No cwd: the core finds the home through TANDEM_HOME, and a packaged app's
  // `app.getAppPath()` names `app.asar`, which is not a directory.
  const child = spawn(command, ['serve', '--port', '0'], {
    env: { ...process.env, ...homeEnvironment() },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  })
  serving = child
  child.stderr.setEncoding('utf8')
  child.stderr.on('data', (chunk: string) => process.stderr.write(chunk))

  return new Promise((resolve, reject) => {
    let output = ''
    child.stdout.setEncoding('utf8')
    child.stdout.on('data', (chunk: string) => {
      output += chunk
      const [, url] = /listening on (http:\/\/\S+)/.exec(output) ?? []
      if (url !== undefined) resolve(url)
    })
    child.once('error', reject)
    child.once('exit', (code) => {
      if (code !== 0 && code !== null) {
        reject(new Error(`The serve child exited with ${code}.`))
      }
    })
  })
}

/**
 * `TANDEM_SMOKE`: the packaged app's own proof that it carries a core
 * (`scripts/smoke-packaged.mjs`). It starts the child, asks the board it serves
 * for the revision it drew, prints it and exits, without opening a window.
 */
async function smoke(url: string): Promise<void> {
  const response = await fetch(`${url}/board`)
  const body = await response.text()
  const revision = /data-revision="(\d+)"/.exec(body)
  if (!response.ok || revision === null) {
    throw new Error(`the board answered ${response.status} without a revision`)
  }
  process.stdout.write(`revision ${revision[1]}\n`, () => app.exit(0))
}

function sameOrigin(candidate: string, base: string): boolean {
  try {
    return new URL(candidate).origin === new URL(base).origin
  } catch {
    return false
  }
}

function installContentSecurityPolicy(): void {
  // The served HTML sets its own CSP; this keeps the shell's floor if a page
  // ever answers without one.
  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    callback({
      responseHeaders: {
        ...details.responseHeaders,
        'Content-Security-Policy': [contentSecurityPolicy(false)],
      },
    })
  })
}

function createWindow(url: string): void {
  const win = new BrowserWindow(
    windowOptions({
      preload: join(__dirname, '../preload/index.js'),
      devServerUrl: url,
    }),
  )
  shell = win

  win.on('closed', () => {
    if (shell === win) shell = null
  })
  win.once('ready-to-show', () => win.show())
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
  win.webContents.on('will-navigate', (event, candidate) => {
    if (!sameOrigin(candidate, url)) event.preventDefault()
  })

  void win.loadURL(url)
}

app.whenReady().then(async () => {
  installContentSecurityPolicy()

  try {
    const url = await serve()
    if (process.env['TANDEM_SMOKE'] !== undefined) {
      await smoke(url)
      return
    }
    createWindow(url)
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    app.exit(1)
  }
})

app.on('web-contents-created', (_event, contents) => {
  contents.on('will-attach-webview', (event) => event.preventDefault())
})

app.on('window-all-closed', () => app.quit())

app.on('before-quit', () => {
  serving?.kill()
  serving = null
})
