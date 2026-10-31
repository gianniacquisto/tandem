import { writeFile } from 'node:fs/promises'
import { join } from 'node:path'

import { app, BrowserWindow, ipcMain, session } from 'electron'

import { resolveBinary } from './binary'
import { RpcClient } from './rpcClient'
import { contentSecurityPolicy } from '../shared/security'
import { CALL_CHANNEL, CHANGED_CHANNEL, type BridgeCall, type Call } from '../shared/bridge'
import { isFixtureHome } from './shot'
import { windowOptions } from './window'

/**
 * The shell's process: one window, one rpc child, and the policy around them.
 *
 * The child is spawned here and dies with the pipe, so the app and the core
 * cannot drift apart. Development reads the fixture home under `Fixtures/`
 * unless `TANDEM_HOME` names another, because the Swift app still owns the
 * real `~/.tandem` (`docs/port.md`, How the work lands).
 */
const devServerUrl: string | undefined = process.env['ELECTRON_RENDERER_URL']

let client: RpcClient | null = null
let shell: BrowserWindow | null = null

function homeEnvironment(): Record<string, string> {
  const named = process.env['TANDEM_HOME']
  if (named !== undefined && named.trim() !== '') return { TANDEM_HOME: named }
  if (app.isPackaged) return {}
  return { TANDEM_HOME: join(app.getAppPath(), '..', 'Fixtures', 'port', 'Home') }
}

function connect(): RpcClient {
  const command = resolveBinary({
    env: process.env,
    packaged: app.isPackaged,
    platform: process.platform,
    appPath: app.getAppPath(),
    resourcesPath: process.resourcesPath,
  })

  const spawned = RpcClient.spawn(command, ['rpc'], {
    cwd: app.getAppPath(),
    env: homeEnvironment(),
  })
  void spawned.exited.then((code) => {
    if (code !== 0 && code !== null) console.error(`The rpc child exited with ${code}.`)
  })
  return spawned.client
}

function replyTo(call: Call): Promise<BridgeCall> {
  if (client === null) {
    return Promise.resolve({ error: { code: 'transport', message: 'The rpc child is not running.' } })
  }

  return client.call(call).then(
    (reply) => ({ reply }),
    (error: unknown) => ({
      error: {
        code: error instanceof Error && 'code' in error ? String(error.code) : 'transport',
        message: error instanceof Error ? error.message : 'The rpc call failed.',
      },
    }),
  )
}

function sameOrigin(candidate: string, base: string): boolean {
  try {
    return new URL(candidate).origin === new URL(base).origin
  } catch {
    return false
  }
}

function installContentSecurityPolicy(): void {
  const development = devServerUrl !== undefined
  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    callback({
      responseHeaders: {
        ...details.responseHeaders,
        'Content-Security-Policy': [contentSecurityPolicy(development)],
      },
    })
  })
}

/**
 * The in-process photograph: `TANDEM_SHOT` writes one PNG after the first
 * render, on the fixture home only. It is the shell's own window, so no Screen
 * Recording permission is involved (`docs/screenshots/README.md`).
 */
function maybeCapture(win: BrowserWindow): void {
  const target = process.env['TANDEM_SHOT']
  if (target === undefined || target.trim() === '' || app.isPackaged) return

  const fixturesRoot = join(app.getAppPath(), '..', 'Fixtures')
  const home = process.env['TANDEM_HOME'] ?? join(fixturesRoot, 'port', 'Home')
  if (!isFixtureHome(home, fixturesRoot)) {
    console.error('TANDEM_SHOT refused: TANDEM_HOME must name a home under Fixtures/.')
    app.exit(1)
    return
  }

  win.webContents.once('did-finish-load', () => {
    setTimeout(() => {
      void win.webContents.capturePage().then(async (image) => {
        await writeFile(target, image.toPNG())
        app.exit(0)
      })
    }, 1500)
  })
}

function createWindow(): void {
  const win = new BrowserWindow(
    windowOptions({
      preload: join(__dirname, '../preload/index.js'),
      devServerUrl,
    }),
  )
  shell = win

  win.on('closed', () => {
    if (shell === win) shell = null
  })
  win.once('ready-to-show', () => win.show())
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
  win.webContents.on('will-navigate', (event, url) => {
    if (devServerUrl !== undefined && sameOrigin(url, devServerUrl)) return
    event.preventDefault()
  })

  maybeCapture(win)

  if (devServerUrl !== undefined) {
    void win.loadURL(devServerUrl)
  } else {
    void win.loadFile(join(__dirname, '../renderer/index.html'))
  }
}

app.whenReady().then(() => {
  installContentSecurityPolicy()

  client = connect()
  client.onChanged((revision) => {
    for (const win of BrowserWindow.getAllWindows()) {
      win.webContents.send(CHANGED_CHANNEL, revision)
    }
  })

  ipcMain.handle(CALL_CHANNEL, (_event, call: Call) => replyTo(call))

  app.on('web-contents-created', (_event, contents) => {
    contents.on('will-attach-webview', (event) => event.preventDefault())
  })

  createWindow()
})

app.on('window-all-closed', () => app.quit())

app.on('before-quit', () => {
  client?.close()
  client = null
})
