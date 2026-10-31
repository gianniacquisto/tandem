import { join } from 'node:path'

import {
  _electron as electron,
  test as base,
  type ElectronApplication,
  type Page,
} from '@playwright/test'

import { clockScript } from './clock'
import { seedPath, seedUserPath } from './home'
import { WINDOW_FLOOR } from '../src/main/window'
import { APP_ROOT, REPO_ROOT } from './roots'
import type { Scene } from './scenes'

/** The copies `globalSetup` seeds; the app never opens the committed home. */
const SEEDED_HOME = seedPath()
const SEEDED_USER = seedUserPath()

function binaryPath(): string {
  return join(REPO_ROOT, 'rust', 'target', 'debug', process.platform === 'win32' ? 'tandem.exe' : 'tandem')
}

/**
 * A browser environment with the seed in it: the scratch homes, the workspace
 * binary, no `TANDEM_SHOT` (the spec drives the capture), and UTC, so a scene
 * that prints a local time prints the same one on every machine.
 *
 * `HOME` and `XDG_CONFIG_HOME` are the seeded user home, because the setup
 * report prints the agent hosts' roots from them: a Settings picture then names
 * the seed's paths, never a human's `~/.config/opencode`.
 */
function seededEnv(): Record<string, string> {
  const env: Record<string, string> = {}
  for (const [key, value] of Object.entries(process.env)) {
    if (value !== undefined) env[key] = value
  }
  delete env['ELECTRON_RENDERER_URL']
  env['TANDEM_HOME'] = SEEDED_HOME
  env['TANDEM_BIN'] = binaryPath()
  env['TANDEM_SHOT'] = ''
  env['TZ'] = 'UTC'
  env['HOME'] = SEEDED_USER
  env['USERPROFILE'] = SEEDED_USER
  env['XDG_CONFIG_HOME'] = join(SEEDED_USER, '.config')
  return env
}

/**
 * One built app per spec, at the Swift window's 760 by 560 floor: the suite
 * photographs the real shell over the real `rpc` child, not a mounted
 * component. The clock is pinned before the first application script runs, so
 * a scene that renders a now-relative time renders one instant. A spec asserts
 * its scene and then calls `capture`.
 */
export const test = base.extend<{ app: ElectronApplication; window: Page }>({
  app: async ({}, use) => {
    const app = await electron.launch({
      args: [
        join(APP_ROOT, 'out', 'main', 'index.js'),
        // The runner's container has no usable Chromium sandbox (app.yml).
        ...(process.platform === 'linux' ? ['--no-sandbox'] : []),
      ],
      cwd: APP_ROOT,
      env: seededEnv(),
    })

    try {
      await use(app)
    } finally {
      await app.close()
    }
  },
  window: async ({ app }, use) => {
    const window = await app.firstWindow()
    await app.evaluate(({ BrowserWindow }) => {
      BrowserWindow.getAllWindows()[0]?.setContentSize(760, 560)
    })
    await window.addInitScript({ content: clockScript() })
    await window.reload()
    await window.waitForSelector('.shell', { timeout: 30_000 })
    await use(window)
  },
})

/**
 * The face a scene photographs: the shell reads its route from the hash, so a
 * scene opens one by loading the same document at `#/<tab>`.
 */
export async function openTab(page: Page, tab: string): Promise<void> {
  const [document] = page.url().split('#')
  await page.goto(`${document}#/${tab}`)
  await page.waitForSelector('.face__title')
}

/**
 * One scene, one PNG: the file the manifest names, as the spec asserted it, at
 * the window's width. The window grows to the face's own content height (never
 * below the floor), so a face taller than the window is shot whole rather than
 * clipped: a picture of half a table is a picture of no table.
 */
/**
 * A face taller than the window is shot whole by lifting the scroll
 * constraint, not by resizing the window: the OS clamps a window to the
 * display, so a resize would clip a long face anyway. The document lays itself
 * out at its own height, the window keeps its 760 width and its floor, and
 * `fullPage` captures the lot. Every value is the app's own CSS.
 */
const FULL_HEIGHT = `html, body, #root { height: auto } .shell { height: auto; min-height: ${WINDOW_FLOOR.height}px } .shell__body { overflow: visible }`

export async function capture(app: ElectronApplication, window: Page, entry: Scene): Promise<void> {
  await app.evaluate(({ BrowserWindow }) => {
    BrowserWindow.getAllWindows()[0]?.setContentSize(760, 560)
  })
  await window.addStyleTag({ content: FULL_HEIGHT })
  await window.screenshot({
    path: join(REPO_ROOT, 'docs', 'screenshots', entry.file),
    animations: 'disabled',
    caret: 'hide',
    fullPage: true,
  })
}
