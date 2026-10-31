import { expect } from '@playwright/test'

import { capture, test } from '../../shots/app'
import { scene } from '../../shots/scenes'

/**
 * The shell a human opens: the wordmark, the four kept tabs and the store
 * revision the core answered over `rpc`, at the Swift window's floor.
 */
test('the shell at its 760 pt floor', async ({ window }) => {
  await expect(window.locator('.shell__wordmarks')).toHaveText('Tandem')

  const tabs = window.locator('.tabs .tab')
  await expect(tabs).toHaveCount(4)
  await expect(tabs).toHaveText(['Tickets', 'Bikeshed', 'Usage', 'Settings'])
  await expect(window.locator('.tabs .tab[aria-selected="true"]')).toHaveText('Tickets')

  await expect(window.locator('.shell__revision')).toHaveText(/^rev \d+$/)

  await capture(window, scene('shell'))
})
