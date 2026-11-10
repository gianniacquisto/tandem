import { expect } from '@playwright/test'

import { capture, test } from '../../shots/app'
import { pngWidth, screenshotPath } from '../../shots/output'
import { REPO_ROOT } from '../../shots/roots'
import { scene } from '../../shots/scenes'
import { computed, token } from '../../shots/tokens'

/**
 * The shell a human opens: the wordmark and the four tabs, and no revision in
 * the header. The store revision the core answered over `rpc` reads in the
 * Settings face beside the doctor checks, at the Swift window's floor.
 *
 * The launch pins the pixel scale, so the picture is the window's CSS width at
 * 2x on any display: this spec asserts the pin at both ends, the page's
 * `devicePixelRatio` and the width the PNG's own header names.
 */
test('the shell at its 760 pt floor', async ({ app, window }) => {
  await expect(window.locator('.shell__wordmark')).toHaveText('Tandem')

  const tabs = window.locator('.tabs .tab')
  await expect(tabs).toHaveCount(4)
  await expect(tabs).toHaveText(['Tickets', 'Bikeshed', 'Usage', 'Settings'])
  await expect(window.locator('.tabs .tab[aria-selected="true"]')).toHaveText('Tickets')

  await expect(window.locator('.shell__revision')).toHaveCount(0)

  // The day palette is in effect, not merely written down: the page's paper and
  // the tab strip's hairline rule.
  expect(await computed(window, 'body', 'background-color')).toBe(token('paper'))
  expect(await computed(window, '.tabs', 'border-top-width')).toBe(token('hairline'))

  // The page's own scale, read as an expression because the node project
  // carries no DOM lib and the fixture's `window` is the Page.
  expect(await window.evaluate<number>('window.devicePixelRatio')).toBe(2)
  const cssWidth = await window.evaluate<number>('window.innerWidth')

  await capture(app, window, scene('shell'))

  expect(pngWidth(screenshotPath(process.env, REPO_ROOT, scene('shell')))).toBe(cssWidth * 2)
})
