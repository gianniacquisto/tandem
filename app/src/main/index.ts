import { join } from 'node:path'
import { spawn } from 'node:child_process'

import { app, BrowserWindow, ipcMain, session, shell as electronShell } from 'electron'

import { resolveBinary } from './binary'
import { answerHarness, HarnessController, isHarnessCall, tandemHomeDir } from './harness'
import { RpcClient } from './rpcClient'
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
let harness: HarnessController | null = null

function homeEnvironment(): Record<string, string> {
  const named = process.env['TANDEM_HOME']
  if (named !== undefined && named.trim() !== '') return { TANDEM_HOME: named }
  if (app.isPackaged) return {}
  return { TANDEM_HOME: join(app.getAppPath(), '..', 'Fixtures', 'port', 'Home') }
}

/**
 * The harness sign-in's controller, created once: every `harness.*` call runs
 * through it, so at most one device flow is ever pending. The Tandem home is
 * the shell's own rule, resolved here rather than in the renderer.
 */
function harnessController(): HarnessController {
  if (harness === null) {
    harness = new HarnessController({
      spawn: (binary, args, options) => {
        const child = spawn(binary, args, {
          env: options.env,
          cwd: options.cwd,
          stdio: ['ignore', 'pipe', 'pipe'],
          windowsHide: true,
        })
        return {
          onStdout: (listener) => {
            child.stdout?.on('data', (chunk: Buffer) => listener(chunk.toString()))
          },
          onStderr: (listener) => {
            child.stderr?.on('data', (chunk: Buffer) => listener(chunk.toString()))
          },
          onExit: (listener) => {
            child.on('exit', (code) => listener(code))
            child.on('error', () => listener(null))
          },
          kill: () => {
            child.kill()
          },
        }
      },
      openUrl: (url) => {
        void electronShell.openExternal(url)
      },
      tandemHome: tandemHomeDir({
        env: process.env,
        packaged: app.isPackaged,
        appPath: app.getAppPath(),
        homeDir: app.getPath('home'),
      }),
      env: process.env,
      baseEnv: process.env,
    })
  }
  return harness
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
  // The harness sign-in is answered here, never by the core: the renderer
  // reaches the harness home only through this door (TAN-265).
  if (isHarnessCall(call)) {
    return answerHarness(call, harnessController()).then(
      (reply) => ({ reply }),
      (error: unknown) => ({
        error: {
          code: 'harness',
          message: error instanceof Error ? error.message : 'The sign-in failed.',
        },
      }),
    )
  }
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
