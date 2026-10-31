import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { expect } from '@playwright/test'

import { capture, test } from '../../shots/app'
import { REPO_ROOT } from '../../shots/roots'
import { scene } from '../../shots/scenes'

/** `#F1E9D7` as the `rgb(...)` a computed style reports. */
function rgb(hex: string): string {
  const value = parseInt(hex.slice(1), 16)
  return `rgb(${(value >> 16) & 255}, ${(value >> 8) & 255}, ${value & 255})`
}

/** The day `--paper` in tokens.css, read rather than restated. */
function paper(): string {
  const css = readFileSync(join(REPO_ROOT, 'app', 'renderer', 'src', 'tokens.css'), 'utf8')
  const hex = css.match(/:root\s*\{[\s\S]*?--paper:\s*(#[0-9A-Fa-f]{6})/)?.[1]
  if (hex === undefined) throw new Error('no day --paper in tokens.css')
  return hex
}

/**
 * The shell a human opens: the wordmark, the four kept tabs and the store
 * revision the core answered over `rpc`, at the Swift window's floor.
 */
test('the shell at its 760 pt floor', async ({ app, window }) => {
  await expect(window.locator('.shell__wordmark')).toHaveText('Tandem')

  const tabs = window.locator('.tabs .tab')
  await expect(tabs).toHaveCount(4)
  await expect(tabs).toHaveText(['Tickets', 'Bikeshed', 'Usage', 'Settings'])
  await expect(window.locator('.tabs .tab[aria-selected="true"]')).toHaveText('Tickets')

  await expect(window.locator('.shell__revision')).toHaveText(/^rev \d+$/)

  // The day palette is in effect, not merely written down.
  const background = await window.evaluate<string>('getComputedStyle(document.body).backgroundColor')
  expect(background).toBe(rgb(paper()))

  await capture(app, window, scene('shell'))
})
