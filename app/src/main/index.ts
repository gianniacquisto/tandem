import { createReadStream } from 'node:fs'
import { join } from 'node:path'
import { Readable } from 'node:stream'

import { app, BrowserWindow, ipcMain, protocol, session } from 'electron'

import { resolveBinary } from './binary'
import { RpcClient } from './rpcClient'
import { assetInitiator, respondToAsset, type AssetAnswer } from './assets'
import { contentSecurityPolicy } from '../shared/security'
import { CALL_CHANNEL, CHANGED_CHANNEL, type BridgeCall, type Call } from '../shared/bridge'
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

/**
 * The image transport's scheme privileges, registered before the app is ready.
 *
 * `standard: false` is load-bearing: the committed editor bundle spells the
 * target `tandem:<path>` with no authority, and a standard scheme would parse
 * the first segment as a host and lowercase it, breaking the vault's
 * case-sensitive paths. `secure: true` keeps the scheme out of mixed-content
 * refusals on the loopback dev page. This is an in-app scheme, never an OS URL
 * handler: `electron-builder.yml` declares no protocol for it.
 */
protocol.registerSchemesAsPrivileged([
  { scheme: 'tandem', privileges: { standard: false, secure: true } },
])

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

  // No cwd: the core finds the home through TANDEM_HOME, and a packaged
  // app's `app.getAppPath()` names `app.asar`, which is not a directory.
  const spawned = RpcClient.spawn(command, ['rpc'], {
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

/**
 * `TANDEM_SMOKE`: the packaged app's own proof that it carries a core
 * (`scripts/smoke-packaged.mjs`). It spawns the bundled binary, asks
 * `ticket.list` against the home `TANDEM_HOME` names, prints the revision and
 * exits, without opening a window.
 */
function smoke(): void {
  client = connect()
  client
    .call({
      method: 'ticket.list',
      params: {
        filter: null,
        text: null,
        status: null,
        assignee: null,
        lane: null,
        archived: false,
        blocked: false,
      },
    })
    .then(
      (reply) => {
        if (!('revision' in reply)) throw new Error('ticket.list answered without a revision')
        process.stdout.write(`revision ${reply.revision}\n`, () => app.exit(0))
      },
      (error: unknown) => {
        console.error(`ticket.list failed: ${error instanceof Error ? error.message : String(error)}`)
        app.exit(1)
      },
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
 * The `tandem:` handler on the session the window uses. The window is created
 * with no `partition`, so `session.defaultSession` is its session; the handler
 * asks the core for the confined file, its type and its size, and streams the
 * bytes from the main process.
 */
function installTandemProtocol(): void {
  session.defaultSession.protocol.handle('tandem', (request) =>
    respondToAsset(
      {
        url: request.url,
        initiatorOrigin:
          (request as Request & { initiatorOrigin?: string | null }).initiatorOrigin ?? null,
      },
      {
        allowedInitiator: assetInitiator(devServerUrl),
        call: async (path): Promise<AssetAnswer> => {
          const answer = await replyTo({ method: 'note.asset', params: { path } })
          return 'reply' in answer ? (answer.reply as AssetAnswer) : answer
        },
        readFile: async (file) =>
          Readable.toWeb(createReadStream(file)) as ReadableStream<Uint8Array>,
      },
    ),
  )
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

  if (devServerUrl !== undefined) {
    void win.loadURL(devServerUrl)
  } else {
    void win.loadFile(join(__dirname, '../renderer/index.html'))
  }
}

app.whenReady().then(() => {
  installContentSecurityPolicy()
  installTandemProtocol()

  if (process.env['TANDEM_SMOKE'] !== undefined) {
    smoke()
    return
  }

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
