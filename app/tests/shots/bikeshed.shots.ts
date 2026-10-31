import { execFileSync } from 'node:child_process'
import { existsSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { expect } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { seedPath } from '../../shots/home'
import { REPO_ROOT } from '../../shots/roots'
import { scene } from '../../shots/scenes'

/**
 * The Bikeshed face a human writes in: the tree, the committed editor bundle in
 * its frame, the backlinks the note has, the health report over the vault, and
 * a draft an interrupted save left.
 *
 * The recovery is staged in the seed rather than the committed fixture, so the
 * picture proves the panel names it while the fixture stays content. The
 * editor's text is the fixture note's own, read through the bundle, which is
 * the proof the verbatim copy mounts and the bridge supplied its document.
 */
test('the bikeshed face, over the seeded vault', async ({ app, window }) => {
  writeFileSync(
    join(seedPath(), 'bikeshed', '.note-0000-0000-0000-000000000001.tmp'),
    'a draft from a save that was killed',
  )

  await openTab(window, 'bikeshed')

  await expect(window.locator('.face__title')).toHaveText('Bikeshed')
  await expect(window.locator('.tree__vault-name')).toHaveText('Bikeshed')
  await expect(window.locator('.tree__name')).toHaveCount(6)

  await window.locator('.tree__label[title="projects/fixture/fixture.md"]').click()

  const editor = window.frameLocator('.editor__frame')
  await expect(editor.locator('.cm-content')).toContainText('The project overview')
  await expect(window.locator('.editor__title')).toHaveText('fixture')
  await expect(window.locator('.editor__folder')).toHaveText('projects/fixture')

  await expect(window.locator('.rail__title').first()).toHaveText('Backlinks')
  await expect(window.locator('.rail__count').first()).toHaveText('1')
  await expect(window.locator('.rail__row-name').first()).toHaveText('Index')

  await expect(window.locator('.health__count')).toHaveText('1 orphan')

  await expect(window.locator('.rail__recovery')).toContainText(
    '.note-0000-0000-0000-000000000001.tmp',
  )
  await expect(window.locator('.rail__recovery')).toContainText('draft')

  await expect(window.locator('.banner')).toHaveCount(0)

  await capture(app, window, scene('bikeshed'))
})

/**
 * The done-when's lost-write test: a buffer is dirty, an agent writes the same
 * note through the CLI, and the autosave answers a conflict carrying both
 * bodies. Nothing is written until "Take theirs", which adopts the CLI's text.
 */
test('a concurrent CLI write is a conflict with both bodies, never a lost write', async ({
  window,
}) => {
  await openTab(window, 'bikeshed')
  await window.locator('.tree__label[title="projects/fixture/fixture.md"]').click()

  const editor = window.frameLocator('.editor__frame')
  await editor.locator('.cm-content').click()
  await window.keyboard.type(' and my unsaved words')

  const binary = join(REPO_ROOT, 'rust', 'target', 'debug', process.platform === 'win32' ? 'tandem.exe' : 'tandem')
  execFileSync(binary, [
    '--home',
    seedPath(),
    'note',
    'write',
    '--path',
    'projects/fixture/fixture.md',
    '--text',
    'the CLI wrote this while the editor was dirty',
  ])

  await expect(window.locator('.conflict')).toBeVisible()
  const bodies = window.locator('.conflict__text')
  await expect(bodies.nth(0)).toContainText('my unsaved words')
  await expect(bodies.nth(1)).toContainText('the CLI wrote this while the editor was dirty')

  await window.locator('.button', { hasText: 'Take theirs' }).click()
  await expect(window.locator('.conflict')).toHaveCount(0)
  await expect(editor.locator('.cm-content')).toContainText(
    'the CLI wrote this while the editor was dirty',
  )
})

/**
 * The drag that re-parents: a note row dropped on a folder row asks the core for
 * the same move `note move` performs, and the tree redraws from the result. The
 * gesture is the GUI's, so only a real drag proves the wiring; the rule itself
 * is `canReparent`'s and the move's own suite.
 */
test('a dropped note re-parents through the core move', async ({ window }) => {
  await openTab(window, 'bikeshed')

  const note = window.locator('.tree__row[data-kind="note"]').filter({ hasText: 'Index' })
  const folder = window.locator('.tree__row[data-kind="folder"]').filter({ hasText: 'daily' }).first()
  await note.dragTo(folder)

  await expect(
    window.locator('.tree__label[title="daily/Index.md"]'),
  ).toBeVisible()
  expect(existsSync(join(seedPath(), 'bikeshed', 'daily', 'Index.md'))).toBe(true)
  expect(existsSync(join(seedPath(), 'bikeshed', 'Index.md'))).toBe(false)
})
