import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { WINDOW_FLOOR, windowOptions } from '../src/main/window'

const appRoot = fileURLToPath(new URL('..', import.meta.url))

function sourcesUnder(directory: string): string[] {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry)
    return statSync(path).isDirectory()
      ? sourcesUnder(path)
      : path.endsWith('.ts')
        ? [path]
        : []
  })
}

describe('the main window', () => {
  it('keeps the 760 pt floor the Swift window has', () => {
    expect(WINDOW_FLOOR).toEqual({ width: 760, height: 560 })

    const options = windowOptions({ preload: '/tmp/preload.js' })
    expect(options.minWidth).toBe(WINDOW_FLOOR.width)
    expect(options.minHeight).toBe(WINDOW_FLOOR.height)
  })

  it('isolates the renderer: no Node, no remote page, a sandbox', () => {
    const { webPreferences } = windowOptions({ preload: '/tmp/preload.js' })

    expect(webPreferences).toMatchObject({
      preload: '/tmp/preload.js',
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      nodeIntegrationInWorker: false,
      nodeIntegrationInSubFrames: false,
      webSecurity: true,
      allowRunningInsecureContent: false,
      webviewTag: false,
    })
  })

  it('is window-first: nothing creates a tray or a menu-bar extra', () => {
    for (const path of sourcesUnder(join(appRoot, 'src'))) {
      expect(readFileSync(path, 'utf8'), `${path} creates a tray`).not.toMatch(/\bTray\b/)
    }
  })

  it('accepts a loopback dev server and refuses any other origin', () => {
    const dev = windowOptions({ preload: '/tmp/preload.js', devServerUrl: 'http://localhost:5173' })
    expect(dev.webPreferences?.sandbox).toBe(true)

    expect(() =>
      windowOptions({ preload: '/tmp/preload.js', devServerUrl: 'https://example.com' }),
    ).toThrow(/loopback/)
    expect(() =>
      windowOptions({ preload: '/tmp/preload.js', devServerUrl: 'http://127.0.0.1:5173' }),
    ).not.toThrow()
  })
})
