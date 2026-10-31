/**
 * The main window's shape and its isolation.
 *
 * The floor is the Swift window's (760 by 560 points, `MainView.swift`), and
 * the web preferences are the security lens in one place: context isolation
 * on, the renderer sandboxed, no Node in any frame, no embedded webview, and
 * no second origin parked in the window.
 */
export const WINDOW_FLOOR = { width: 760, height: 560 } as const

const LOOPBACK_HOSTS = new Set(['localhost', '127.0.0.1', '[::1]', '::1'])

export type WindowRequest = {
  preload: string
  devServerUrl?: string | undefined
}

export type ShellWindowOptions = {
  width: number
  height: number
  minWidth: number
  minHeight: number
  show: boolean
  title: string
  autoHideMenuBar: boolean
  webPreferences: {
    preload: string
    contextIsolation: boolean
    sandbox: boolean
    nodeIntegration: boolean
    nodeIntegrationInWorker: boolean
    nodeIntegrationInSubFrames: boolean
    webSecurity: boolean
    allowRunningInsecureContent: boolean
    webviewTag: boolean
    spellcheck: boolean
  }
}

function assertLoopback(url: string): void {
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    throw new Error(`The renderer url is not a url: ${url}`)
  }

  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(`The renderer url must be loopback http, not ${parsed.protocol}`)
  }
  if (!LOOPBACK_HOSTS.has(parsed.hostname)) {
    throw new Error(`The renderer url must be a loopback host, not ${parsed.hostname}`)
  }
}

export function windowOptions(request: WindowRequest): ShellWindowOptions {
  if (request.devServerUrl !== undefined) assertLoopback(request.devServerUrl)

  return {
    width: WINDOW_FLOOR.width,
    height: WINDOW_FLOOR.height,
    minWidth: WINDOW_FLOOR.width,
    minHeight: WINDOW_FLOOR.height,
    show: false,
    title: 'Tandem',
    autoHideMenuBar: true,
    webPreferences: {
      preload: request.preload,
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      nodeIntegrationInWorker: false,
      nodeIntegrationInSubFrames: false,
      webSecurity: true,
      allowRunningInsecureContent: false,
      webviewTag: false,
      spellcheck: false,
    },
  }
}
