import { join } from 'node:path'

import { _electron as electron, test as base, type Page } from '@playwright/test'

import { seedPath } from './home'
import { APP_ROOT, REPO_ROOT } from './roots'
import type { Scene } from './scenes'

/** The copy `globalSetup` seeds; the app never opens the committed home. */
const SEEDED_HOME = seedPath(REPO_ROOT)

function binaryPath(): string {
  return join(REPO_ROOT, 'rust', 'target', 'debug', process.platform === 'win32' ? 'tandem.exe' : 'tandem')
}

/**
 * A browser environment with the seed in it: the scratch home, the workspace
 * binary, no `TANDEM_SHOT` (the spec drives the capture), and UTC, so a scene
 * that prints a local time prints the same one on every machine.
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
  return env
}

/**
 * One built app per spec, at the Swift window's 760 by 560 floor: the suite
 * photographs the real shell over the real `rpc` child, not a mounted
 * component. A spec asserts its scene and then calls `capture`.
 */
export const test = base.extend<{ window: Page }>({
  window: async ({}, use) => {
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
      const window = await app.firstWindow()
      await app.evaluate(({ BrowserWindow }) => {
        BrowserWindow.getAllWindows()[0]?.setContentSize(760, 560)
      })
      await window.waitForSelector('.shell', { timeout: 30_000 })
      await use(window)
    } finally {
      await app.close()
    }
  },
})

/** One scene, one PNG: the file the manifest names, as the spec asserted it. */
export async function capture(window: Page, entry: Scene): Promise<void> {
  await window.screenshot({
    path: join(REPO_ROOT, 'docs', 'screenshots', entry.file),
    animations: 'disabled',
    caret: 'hide',
  })
}
