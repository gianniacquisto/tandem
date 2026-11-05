import { execFileSync } from 'node:child_process'
import { existsSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { expect } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { seedPath } from '../../shots/home'
import { REPO_ROOT } from '../../shots/roots'
import { scene } from '../../shots/scenes'
import { computed, token } from '../../shots/tokens'

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

  await expect(window.locator('.bikeshed-face')).toBeVisible()
  await expect(window.locator('.bikeshed-face .face__title')).toHaveCount(0)
  await expect(window.locator('.tree__vault-name')).toHaveText('Bikeshed')
  await expect(window.locator('.tree__action', { hasText: "Today's note" })).toBeVisible()
  await expect(window.locator('.tree__new--note')).toHaveText('New note')
  await expect(window.locator('.tree__new--folder')).toHaveText('New folder')
  await expect(window.locator('.tree__name')).toHaveCount(6)

  await window.locator('.tree__label[title="projects/fixture/fixture.md"]').click()

  const editor = window.frameLocator('.editor__frame')
  await expect(editor.locator('.cm-content')).toContainText('The project overview')
  await expect(window.locator('.editor__title')).toHaveText('fixture')
  await expect(window.locator('.editor__folder')).toHaveText('projects/fixture')

  // The editor frame's titlebar rule is the palette's sand, not a UA border.
  expect(await computed(window, '.editor__titlebar', 'border-bottom-color')).toBe(token('sand'))

  await expect(window.locator('.rail__title').first()).toHaveText('Backlinks')
  await expect(window.locator('.rail__count').first()).toHaveText('1')
  await expect(window.locator('.rail__row-name').first()).toHaveText('Index')

  await expect(window.locator('.health__count')).toHaveText('1 orphan')

  // The side rail collapses into the titlebar toggle: one click hides the
  // links and the editor keeps the full height, one more brings them back.
  const toggle = window.locator('.editor__rail-toggle')
  await expect(toggle).toHaveText('Hide links')
  await toggle.click()
  await expect(window.locator('#note-rail')).toHaveCount(0)
  await expect(toggle).toContainText('Links')
  await toggle.click()
  await expect(window.locator('#note-rail')).toBeVisible()
  await expect(toggle).toHaveText('Hide links')

  await expect(window.locator('.rail__recovery')).toContainText(
    'Unsaved draft kept from an interrupted save',
  )
  await expect(window.locator('.rail__recovery')).toContainText('draft')
  await expect(window.locator('.rail__recovery')).not.toContainText('.note-')
  await expect(window.locator('.rail__recovery')).not.toContainText('.tmp')

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
 * The creation the face was missing: the tree's New note control asks for a
 * name, the core's `note.create` writes it, and the face opens it ready to
 * type. The default location is the vault root when nothing is open.
 */
test('a new note through the tree control opens in the editor ready to type', async ({
  window,
}) => {
  await openTab(window, 'bikeshed')

  await window.locator('.tree__new--note').click()
  const dialog = window.getByRole('dialog', { name: 'New note' })
  await expect(dialog).toBeVisible()
  await dialog.getByLabel('Name').fill('from-ui')
  await dialog.getByRole('button', { name: 'Create' }).click()

  await expect(window.locator('.tree__label[title="from-ui.md"]')).toBeVisible()
  await expect(window.locator('.editor__title')).toHaveText('from-ui')
  await expect(window.locator('.editor__folder')).toHaveText('Bikeshed')
  await expect(window.locator('.banner')).toHaveCount(0)

  const editor = window.frameLocator('.editor__frame')
  await editor.locator('.cm-content').click()
  await window.keyboard.type('hello from UI-1')
  await expect(editor.locator('.cm-content')).toContainText('hello from UI-1')
  expect(existsSync(join(seedPath(), 'bikeshed', 'from-ui.md'))).toBe(true)
})

/**
 * The folder half of the same affordance: the tree's New folder control asks
 * for a name, the core's `note.createFolder` writes it, and the tree draws it
 * expanded.
 */
test('a new folder through the tree control appears expanded', async ({ window }) => {
  await openTab(window, 'bikeshed')

  await window.locator('.tree__new--folder').click()
  const dialog = window.getByRole('dialog', { name: 'New folder' })
  await expect(dialog).toBeVisible()
  await dialog.getByLabel('Name').fill('new-folder')
  await dialog.getByRole('button', { name: 'Create' }).click()

  const row = window.locator('.tree__row[data-kind="folder"]').filter({ hasText: 'new-folder' })
  await expect(row).toBeVisible()
  await expect(row).toHaveAttribute('aria-expanded', 'true')
  await expect(window.locator('.banner')).toHaveCount(0)
  expect(existsSync(join(seedPath(), 'bikeshed', 'new-folder'))).toBe(true)
})

/**
 * Today's note: one click opens-or-creates `daily/<today>.md` and opens it
 * ready to type; a second click the same day returns the same file with staged
 * content intact.
 */
test("today's note opens-or-creates and keeps staged content", async ({ window }) => {
  await openTab(window, 'bikeshed')

  const action = window.locator('.tree__action', { hasText: "Today's note" })
  await expect(action).toBeVisible()
  await action.click()

  const now = new Date()
  const day = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
  const title = `daily/${day}.md`

  await expect(window.locator(`.tree__label[title="${title}"]`)).toBeVisible()
  await expect(window.locator('.editor__title')).toHaveText(day)
  await expect(window.locator('.editor__folder')).toHaveText('daily')

  const editor = window.frameLocator('.editor__frame')
  await expect(editor.locator('.cm-content')).toBeVisible()
  await editor.locator('.cm-content').click()
  await window.keyboard.type(' words for today')
  await expect(editor.locator('.cm-content')).toContainText('words for today')

  await action.click()
  await expect(window.locator(`.tree__label[title="${title}"]`)).toBeVisible()
  await expect(editor.locator('.cm-content')).toContainText('words for today')
  await expect(window.locator('.banner')).toHaveCount(0)
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

/**
 * The watcher the port does not have: a focus re-read is it (ADR 0004). A dirty
 * buffer learns of the CLI's write as a conflict immediately, without waiting
 * for the autosave, and the conflict survives a tab switch because the unmount
 * keeps the buffer as a draft.
 */
test('an external write on focus is a conflict, and the tab switch keeps the draft', async ({
  window,
}) => {
  await openTab(window, 'bikeshed')
  await window.locator('.tree__label[title="projects/fixture/fixture.md"]').click()

  const editor = window.frameLocator('.editor__frame')
  await editor.locator('.cm-content').click()
  await window.keyboard.type(' and words no save has seen')
  await expect(editor.locator('.cm-content')).toContainText('words no save has seen')
  // The dirty message crosses the frame's postMessage before the face has it;
  // the focus must see the buffer, not the keystroke before it.
  await window.waitForTimeout(300)

  const binary = join(REPO_ROOT, 'rust', 'target', 'debug', process.platform === 'win32' ? 'tandem.exe' : 'tandem')
  execFileSync(binary, [
    '--home',
    seedPath(),
    'note',
    'write',
    '--path',
    'projects/fixture/fixture.md',
    '--text',
    'written behind the open editor',
  ])

  // The watcher's stand-in: the window's own focus event.
  await window.evaluate("window.dispatchEvent(new Event('focus'))")
  await expect(window.locator('.conflict')).toBeVisible()
  await expect(window.locator('.conflict__text').nth(0)).toContainText('words no save has seen')
  await expect(window.locator('.conflict__text').nth(1)).toContainText(
    'written behind the open editor',
  )

  // Unmount the face and come back: the dirty buffer is a draft, not a loss.
  await window.locator('.tab', { hasText: 'Tickets' }).click()
  await expect(window.locator('.tabs .tab[aria-selected="true"]')).toHaveText('Tickets')
  await window.waitForTimeout(500)
  await window.locator('.tab', { hasText: 'Bikeshed' }).click()
  await expect(window.locator('.bikeshed-face')).toBeVisible()

  await expect(editor.locator('.cm-content')).toContainText('words no save has seen')
  await expect(window.locator('.banner--notice')).toContainText('draft')
  await expect(window.locator('.conflict')).toBeVisible()
})

/**
 * The editor's own send: the bundle's Mod-Shift-t posts the selection as
 * `send-to-todos`, and the face files it as a ticket linked to the note. The
 * shortcut is the bundle's (its own suite covers it); the message is the seam
 * the port must answer, so this posts the exact message and checks the store.
 */
test('the editor send files a ticket linked to the note', async ({ window }) => {
  await openTab(window, 'bikeshed')
  await window.locator('.tree__label[title="projects/fixture/fixture.md"]').click()

  const editor = window.frameLocator('.editor__frame')
  await expect(editor.locator('.cm-content')).toBeVisible()

  const frame = window.frames().find((candidate) => candidate.url().includes('editor-frame.html'))
  expect(frame).toBeDefined()
  const message = JSON.stringify({
    type: 'send-to-todos',
    text: '- [ ] File this from the note',
  })
  await frame?.evaluate(
    `window.webkit.messageHandlers.tandem.postMessage(${JSON.stringify(message)})`,
  )

  const tickets = window.locator('.rail__section').filter({ hasText: 'Tickets' })
  await expect(tickets.locator('.rail__title')).toHaveText('Tickets')
  await expect(tickets.locator('.rail__row-detail')).toContainText('File this from the note')

  const binary = join(REPO_ROOT, 'rust', 'target', 'debug', process.platform === 'win32' ? 'tandem.exe' : 'tandem')
  const listed = JSON.parse(
    execFileSync(binary, ['--home', seedPath(), 'ticket', 'list'], { encoding: 'utf8' }),
  ) as { tickets: Array<{ title: string; link: string | null }> }
  expect(
    listed.tickets.some(
      (ticket) =>
        ticket.title === 'File this from the note' &&
        ticket.link === 'projects/fixture/fixture.md',
    ),
  ).toBe(true)
})
