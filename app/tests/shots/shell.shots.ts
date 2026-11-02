import { expect } from '@playwright/test'

import { capture, test } from '../../shots/app'
import { scene } from '../../shots/scenes'
import { computed, token } from '../../shots/tokens'

/**
 * The shell a human opens: the wordmark, the five tabs and the store
 * revision the core answered over `rpc`, at the Swift window's floor.
 */
test('the shell at its 760 pt floor', async ({ app, window }) => {
  await expect(window.locator('.shell__wordmark')).toHaveText('Tandem')

  const tabs = window.locator('.tabs .tab')
  await expect(tabs).toHaveCount(5)
  await expect(tabs).toHaveText(['Tickets', 'Bikeshed', 'Usage', 'Workflow', 'Settings'])
  await expect(window.locator('.tabs .tab[aria-selected="true"]')).toHaveText('Tickets')

  await expect(window.locator('.shell__revision')).toHaveText(/^rev \d+$/)

  // The day palette is in effect, not merely written down: the page's paper and
  // the tab strip's hairline rule.
  expect(await computed(window, 'body', 'background-color')).toBe(token('paper'))
  expect(await computed(window, '.tabs', 'border-top-width')).toBe(token('hairline'))

  await capture(app, window, scene('shell'))
})
