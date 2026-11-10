import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { expect, type Page } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { seedPath } from '../../shots/home'
import { REPO_ROOT } from '../../shots/roots'
import { scene } from '../../shots/scenes'
import { computed, token } from '../../shots/tokens'

/**
 * The Bikeshed face a human writes in: a 260 px tree column on parchment and a
 * document column on paper, with the committed editor bundle in its frame, a
 * sticky breadcrumb above the note, and the note's links in a footer after it.
 *
 * The recovery is staged in the seed rather than the committed fixture, so the
 * picture proves the panel names it while the fixture stays content. The
 * editor's text is the fixture note's own, read through the bundle, which is
 * the proof the verbatim copy mounts and the bridge supplied its document.
 */
test('the bikeshed face, over the seeded vault', async ({ window }) => {
  writeFileSync(
    join(seedPath(), 'bikeshed', '.note-0000-0000-0000-000000000001.tmp'),
    'a draft from a save that was killed',
  )

  await openTab(window, 'bikeshed')

  await expect(window.locator('.bikeshed-face')).toBeVisible()
  await expect(window.locator('.bikeshed-face .face__title')).toHaveCount(0)

  // Two columns: the tree is a fixed 260 wide, the document starts after it.
  const tree = window.locator('.bikeshed__tree')
  await expect(tree).toBeVisible()
  const treeBox = await tree.boundingBox()
  expect(treeBox?.width).toBe(260)
  const document = window.locator('.bikeshed__document')
  await expect(document).toBeVisible()
  const documentBox = await document.boundingBox()
  expect(documentBox?.x).toBe(260)

  await expect(window.locator('.tree__vault-name')).toHaveText('Bikeshed')
  await expect(window.locator('.tree__action', { hasText: "Today's note" })).toBeVisible()
  await expect(window.locator('.tree__new--note')).toHaveText('New note')
  await expect(window.locator('.tree__new--folder')).toHaveText('New folder')
  await expect(window.locator('.tree__name')).toHaveCount(6)

  // The focus control decides what the tree draws, so it sits at its top.
  await expect(window.locator('.focus')).toBeVisible()
  await expect(tree.locator('.focus__picker')).toBeVisible()

  await expect(window.locator('.health__count')).toHaveText('1 orphan')
})

/**
 * The breadcrumb: the note's folder crumbs then its title as the current crumb,
 * plain text, above a sticky header the mode control and the saving word share.
 */
test('the breadcrumb names the note', async ({ window }) => {
  await openTab(window, 'bikeshed')
  await window.locator('.tree__label[title="projects/fixture/fixture.md"]').click()

  const breadcrumb = window.locator('.bikeshed__breadcrumb')
  await expect(breadcrumb).toBeVisible()
  await expect(breadcrumb.locator('.bikeshed__crumb')).toHaveText([
    'projects',
    'fixture',
    'fixture',
  ])
  await expect(breadcrumb.locator('.bikeshed__crumb--current')).toHaveText('fixture')

  // A note with content opens in Write: the block surface is the preview.
  await expect(window.locator('.blocks')).toContainText('The project overview')

  // The breadcrumb's rule is the palette's sand, not a UA border.
  expect(await computed(window, '.bikeshed__breadcrumb', 'border-bottom-color')).toBe(
    token('sand'),
  )

  await expect(window.locator('.banner')).toHaveCount(0)
})

/**
 * The page: the document column scrolls, the breadcrumb sticks to its top, and
 * the note's links are its footer, reached at the end of the note rather than
 * docked to a rail. The picture is the Write surface over the fixture note.
 */
test('the breadcrumb sticks and the footer is reachable', async ({ app, window }) => {
  writeFileSync(
    join(seedPath(), 'bikeshed', '.note-0000-0000-0000-000000000001.tmp'),
    'a draft from a save that was killed',
  )

  await openTab(window, 'bikeshed')
  await window.evaluate("window.dispatchEvent(new Event('focus'))")
  await window.locator('.tree__label[title="projects/fixture/fixture.md"]').click()
  await expect(window.locator('.blocks')).toContainText('The project overview')

  const footer = window.locator('.note-footer')
  await expect(footer.locator('.note-footer__section').filter({ hasText: 'Backlinks' })).toBeVisible()

  const crumb = window.locator('.bikeshed__breadcrumb')
  const before = await crumb.boundingBox()
  expect(before).not.toBeNull()

  const column = window.locator('.bikeshed__document')
  await column.evaluate((element) => {
    element.scrollTop = element.scrollHeight
  })
  await expect.poll(() => column.evaluate((element) => element.scrollTop)).toBeGreaterThan(0)

  // Sticky: the breadcrumb keeps the column's top edge while the page moves.
  const after = await crumb.boundingBox()
  expect(after?.y).toBe(before?.y)

  await expect(footer).toBeInViewport()
  await expect(footer.locator('.note-footer__title').first()).toHaveText('Backlinks')
  await expect(footer.locator('.note-footer__count').first()).toHaveText('1')
  await expect(footer.locator('.note-footer__row-name').first()).toHaveText('Index')

  await expect(footer.locator('.note-footer__recovery')).toContainText(
    'Unsaved draft kept from an interrupted save',
  )
  await expect(footer.locator('.note-footer__recovery')).toContainText('draft')
  await expect(footer.locator('.note-footer__recovery')).not.toContainText('.note-')
  await expect(footer.locator('.note-footer__recovery')).not.toContainText('.tmp')

  await expect(window.locator('.banner')).toHaveCount(0)

  await capture(app, window, scene('bikeshed'))
})

/**
 * The mode control lives in the breadcrumb: the two-button group there is
 * Write|Source, and Preview is gone with the iframe.
 */
test('the mode control lives in the breadcrumb', async ({ window }) => {
  await openTab(window, 'bikeshed')
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'write')")
  await window.locator('.tree__label[title="projects/fixture/fixture.md"]').click()

  const breadcrumb = window.locator('.bikeshed__breadcrumb')
  await expect(breadcrumb).toBeVisible()
  const mode = breadcrumb.getByRole('group')
  await expect(mode).toBeVisible()

  const write = mode.locator('button', { hasText: 'Write' })
  const source = mode.locator('button', { hasText: 'Source' })
  await expect(write).toHaveAttribute('aria-pressed', 'true')
  await expect(window.locator('.blocks')).toContainText('The project overview')

  await source.click()

  await expect(source).toHaveAttribute('aria-pressed', 'true')
  await expect(window.locator('.source')).toHaveValue(/The project overview/)
  await expect(window.locator('.editor__frame')).toHaveCount(0)
})

/** The rail is gone: no toggle in the face's chrome and no dock to open. */
test('no rail toggle and no rail', async ({ window }) => {
  await openTab(window, 'bikeshed')
  await expect(window.locator('.editor__rail-toggle')).toHaveCount(0)
  await expect(window.locator('#note-rail')).toHaveCount(0)
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
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'source')")
  await window.locator('.tree__label[title="projects/fixture/fixture.md"]').click()

  // Source mode edits the exact bytes, so the dirty buffer is the textarea's.
  const source = window.locator('.source')
  await expect(source).toBeVisible()
  await source.click()
  await window.keyboard.type(' and my unsaved words')
  await expect(source).toHaveValue(/my unsaved words/)

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
  await expect(source).toHaveValue('the CLI wrote this while the editor was dirty')
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
  await expect(window.locator('.bikeshed__crumb--current')).toHaveText('from-ui')
  await expect(window.locator('.bikeshed__crumb').first()).toHaveText('Bikeshed')
  await expect(window.locator('.banner')).toHaveCount(0)

  // An empty creation opens in Write, ready to type.
  await expect(window.locator('.editor__mode-button', { hasText: 'Write' })).toHaveAttribute(
    'aria-pressed',
    'true',
  )
  const blocks = window.locator('.blocks')
  await expect(blocks).toBeVisible()
  await blocks.locator('.block--paragraph .block__text').first().click()
  await window.keyboard.type('hello from UI-1')
  await expect(blocks).toContainText('hello from UI-1')
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
  await expect(window.locator('.bikeshed__crumb--current')).toHaveText(day)
  await expect(window.locator('.bikeshed__crumb').first()).toHaveText('daily')

  // Today's note is created empty, so it opens in Write.
  const blocks = window.locator('.blocks')
  await expect(blocks).toBeVisible()
  await blocks.locator('.block--paragraph .block__text').first().click()
  await window.keyboard.type(' words for today')
  await expect(blocks).toContainText('words for today')

  await action.click()
  await expect(window.locator(`.tree__label[title="${title}"]`)).toBeVisible()
  await expect(blocks).toContainText('words for today')
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
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'source')")
  await window.locator('.tree__label[title="projects/fixture/fixture.md"]').click()

  // Source mode is writable: the dirty buffer is the textarea's exact bytes.
  const source = window.locator('.source')
  await expect(source).toBeVisible()
  await source.click()
  await window.keyboard.type(' and words no save has seen')
  await expect(source).toHaveValue(/words no save has seen/)
  // The dirty message reaches the face as the textarea's change; the focus
  // must see the buffer, not the keystroke before it.
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

  await expect(window.locator('.source')).toHaveValue(/words no save has seen/)
  await expect(window.locator('.banner--notice')).toContainText('draft')
  await expect(window.locator('.conflict')).toBeVisible()
})

/**
 * The Write surface's send: Mod-Shift-t files the selected task, and the face
 * files it as a ticket linked to the note. Preview's iframe send is retired;
 * the block surface is the one editor path.
 */
test('the editor send files a ticket linked to the note', async ({ window }) => {
  writeFileSync(writeFile('send-note.md'), '- [ ] File this from the note\n')
  await openTab(window, 'bikeshed')
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'write')")
  await window.evaluate("window.dispatchEvent(new Event('focus'))")
  await window.locator('.tree__label[title="send-note.md"]').click()

  const write = window.locator('.editor__mode-button', { hasText: 'Write' })
  await expect(write).toHaveAttribute('aria-pressed', 'true')
  const task = window.locator('.block--task .block__text')
  await task.click()
  await window.keyboard.press(`${modKey}+Shift+t`)

  const tickets = window.locator('.note-footer__section').filter({ hasText: 'Tickets' })
  await expect(tickets.locator('.note-footer__title')).toHaveText('Tickets')
  await expect(tickets.locator('.note-footer__row-detail')).toContainText(
    'File this from the note',
  )

  const binary = join(REPO_ROOT, 'rust', 'target', 'debug', process.platform === 'win32' ? 'tandem.exe' : 'tandem')
  const listed = JSON.parse(
    execFileSync(binary, ['--home', seedPath(), 'ticket', 'list'], { encoding: 'utf8' }),
  ) as { tickets: Array<{ title: string; link: string | null }> }
  expect(
    listed.tickets.some(
      (ticket) => ticket.title === 'File this from the note' && ticket.link === 'send-note.md',
    ),
  ).toBe(true)
})

/**
 * Preview is retired: a note with content opens in Write, and the block
 * surface is the one editable view. The iframe is no longer mounted.
 */
test('a note with content opens in Write over the block surface', async ({ window }) => {
  await openTab(window, 'bikeshed')
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'write')")
  await window.locator('.tree__label[title="projects/fixture/fixture.md"]').click()

  const write = window.locator('.editor__mode-button', { hasText: 'Write' })
  const source = window.locator('.editor__mode-button', { hasText: 'Source' })
  await expect(write).toHaveAttribute('aria-pressed', 'true')
  await expect(source).toHaveAttribute('aria-pressed', 'false')

  await expect(window.locator('.blocks')).toContainText('The project overview')
  await expect(window.locator('.editor__frame')).toHaveCount(0)
})

/**
 * A wikilink's chip in Write opens its target and stays in Write: no caret
 * jump, no dirty, the buffer is the truth.
 */
test('a wikilink click in Write opens its target and stays in Write', async ({ window }) => {
  await openTab(window, 'bikeshed')
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'write')")
  await window.locator('.tree__label[title="Index.md"]').click()

  const write = window.locator('.editor__mode-button', { hasText: 'Write' })
  await expect(write).toHaveAttribute('aria-pressed', 'true')

  const chip = window.locator('.blocks .wikilink-chip').first()
  await expect(chip).toBeVisible()
  await chip.click()

  await expect(window.locator('.bikeshed__crumb--current')).toHaveText('fixture')
  await expect(write).toHaveAttribute('aria-pressed', 'true')
  await expect(window.locator('.editor__saving')).toHaveCount(0)
})

/**
 * A task checkbox in Write toggles through the existing dirty/autosave path
 * and survives a reopen: no new rpc, the buffer is the truth.
 */
test('a task checkbox toggles in Write and survives reload', async ({ window }) => {
  writeFileSync(join(seedPath(), 'bikeshed', 'checkbox-write.md'), '# Check\n\n- [ ] write toggle\n')
  await openTab(window, 'bikeshed')
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'write')")
  await window.evaluate("window.dispatchEvent(new Event('focus'))")
  await window.locator('.tree__label[title="checkbox-write.md"]').click()

  const write = window.locator('.editor__mode-button', { hasText: 'Write' })
  await expect(write).toHaveAttribute('aria-pressed', 'true')
  await expect(window.locator('.block--task .block__check')).toBeVisible()
  const before = readFileSync(join(seedPath(), 'bikeshed', 'checkbox-write.md'), 'utf8')
  expect(before).toContain('- [ ] write toggle')
  await window.locator('.block--task .block__check').click()
  await expect
    .poll(
      () => readFileSync(join(seedPath(), 'bikeshed', 'checkbox-write.md'), 'utf8'),
      { timeout: 8000 },
    )
    .toContain('- [x] write toggle')

  await window.locator('.tree__label[title="Index.md"]').click()
  await window.locator('.tree__label[title="checkbox-write.md"]').click()
  await expect(write).toHaveAttribute('aria-pressed', 'true')
  await expect(window.locator('.blocks')).toContainText('write toggle')
})

/**
 * The control flips Write and Source both ways and persists the choice. A
 * dirty buffer is flushed before the surface changes, and a Source edit is the
 * same buffer the Write surface draws. Mod+E is inline code (UI-19) and no
 * longer changes the mode.
 */
test('the control switches between Write and Source and flushes first', async ({ window }) => {
  writeFileSync(writeFile('switch.md'), '# Switch\n\nsource words')
  await openTab(window, 'bikeshed')
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'write')")
  await window.evaluate("window.dispatchEvent(new Event('focus'))")
  await window.locator('.tree__label[title="switch.md"]').click()

  const write = window.locator('.editor__mode-button', { hasText: 'Write' })
  const source = window.locator('.editor__mode-button', { hasText: 'Source' })
  await expect(write).toHaveAttribute('aria-pressed', 'true')

  // A dirty Write buffer is flushed before Source draws it.
  await window.locator('.block--paragraph .block__text').first().click()
  await caretAt(window, 1, 'end')
  await window.keyboard.type(' plus write words')
  await source.click()
  await expect(source).toHaveAttribute('aria-pressed', 'true')
  await expect.poll(() => writeText('switch.md'), { timeout: 8000 }).toBe(
    '# Switch\n\nsource words plus write words',
  )
  await expect(window.locator('.source')).toHaveValue(
    '# Switch\n\nsource words plus write words',
  )

  // A Source edit is the same buffer; switching back flushes it too.
  await window.locator('.source').evaluate((element) => {
    const area = element as HTMLTextAreaElement
    area.focus()
    area.setSelectionRange(area.value.length, area.value.length)
  })
  await window.keyboard.type(' plus source words')
  await write.click()
  await expect(write).toHaveAttribute('aria-pressed', 'true')
  await expect.poll(() => writeText('switch.md'), { timeout: 8000 }).toBe(
    '# Switch\n\nsource words plus write words plus source words',
  )
  await expect(window.locator('.blocks')).toContainText('plus source words')

  // The stored choice survived the switch.
  expect(await window.evaluate("window.localStorage.getItem('tandem.bikeshed.mode')")).toBe('write')

  // Mod+E is inline code now: it does not change the mode.
  await window.keyboard.press(`${modKey}+e`)
  await expect(write).toHaveAttribute('aria-pressed', 'true')
})

/**
 * An empty note opens in Write ready to type, regardless of the stored Source
 * choice: the buffer-is-truth rule keeps UI-1/UI-4 creations typing.
 */
test('an empty note opens in Write focused', async ({ window }) => {
  writeFileSync(join(seedPath(), 'bikeshed', 'empty-source.md'), '')
  await openTab(window, 'bikeshed')
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'source')")
  await window.evaluate("window.dispatchEvent(new Event('focus'))")
  await window.locator('.tree__label[title="empty-source.md"]').click()

  const write = window.locator('.editor__mode-button', { hasText: 'Write' })
  await expect(write).toHaveAttribute('aria-pressed', 'true')
  await expect(window.locator('.blocks')).toBeFocused()
  await expect(window.locator('.block--paragraph')).toHaveCount(1)
})

/**
 * Write is the third mode: open a note in it and the block surface is the
 * editable document, over the seeded vault. The helpers below stage a note in
 * the scratch home and switch the face to Write, which is the only gesture the
 * scenes rely on.
 */
const writeFile = (name: string): string => join(seedPath(), 'bikeshed', name)
const writeText = (name: string): string => readFileSync(writeFile(name), 'utf8')
const modKey = process.platform === 'darwin' ? 'Meta' : 'Control'

/** Put the DOM caret at the start or end of a block's text, without Home/End. */
async function caretAt(
  window: Page,
  block: number,
  where: 'start' | 'end',
): Promise<void> {
  await window.locator('.block__text').nth(block).evaluate((element, at) => {
    const range = document.createRange()
    range.selectNodeContents(element)
    range.collapse(at === 'start')
    const selection = document.getSelection()
    selection?.removeAllRanges()
    selection?.addRange(range)
  }, where)
}

async function openWrite(window: Page, name: string, content: string): Promise<void> {
  writeFileSync(writeFile(name), content)
  await openTab(window, 'bikeshed')
  await window.evaluate("window.dispatchEvent(new Event('focus'))")
  await window.locator(`.tree__label[title="${name}"]`).click()
  const write = window.locator('.editor__mode-button', { hasText: 'Write' })
  if ((await write.getAttribute('aria-pressed')) !== 'true') await write.click()
  await expect(window.locator('.blocks')).toBeVisible()
}

test('Write is reachable and typing autosaves through the real path', async ({ window }) => {
  await openWrite(window, 'write-typing.md', '# Title\n\nA paragraph.')

  await expect(window.locator('.block--heading .block__text')).toHaveText('Title')
  await expect(window.locator('.block--paragraph .block__text')).toHaveText('A paragraph.')

  await window.locator('.block--paragraph .block__text').first().click()
  await caretAt(window, 1, 'end')
  await window.keyboard.type(' Edited.')
  await expect.poll(() => writeText('write-typing.md'), { timeout: 8000 }).toBe(
    '# Title\n\nA paragraph. Edited.',
  )
})

test('Enter splits and Backspace at the start merges', async ({ window }) => {
  await openWrite(window, 'write-enter.md', 'alpha')

  await window.locator('.block--paragraph .block__text').first().click()
  await caretAt(window, 0, 'end')
  await window.keyboard.press('Enter')
  await window.keyboard.type('beta')
  await expect.poll(() => writeText('write-enter.md'), { timeout: 8000 }).toBe('alpha\nbeta')

  await caretAt(window, 1, 'start')
  await window.keyboard.press('Backspace')
  await expect.poll(() => writeText('write-enter.md'), { timeout: 8000 }).toBe('alphabeta')
})

test('the arrows cross blocks and Shift+arrow extends across them', async ({ window }) => {
  await openWrite(window, 'write-arrows.md', 'ab\n\ncd')

  await window.locator('.block--paragraph .block__text').first().click()
  await caretAt(window, 0, 'end')
  await window.keyboard.press('ArrowRight')
  await window.keyboard.type('X')
  await expect.poll(() => writeText('write-arrows.md'), { timeout: 8000 }).toBe('ab\n\nXcd')

  // From the end of the first block, Shift+ArrowRight crosses the boundary and
  // then selects the next character; the cut leaves the two outer pieces.
  await caretAt(window, 0, 'end')
  await window.keyboard.press('Shift+ArrowRight')
  await window.keyboard.press('Shift+ArrowRight')
  await window.keyboard.press(`${modKey}+x`)
  await expect.poll(() => writeText('write-arrows.md'), { timeout: 8000 }).toBe('abcd')
})

test('a multi-block selection cuts and pastes', async ({ window }) => {
  await openWrite(window, 'write-cut.md', 'one\n\ntwo')
  await window.locator('.blocks').click()

  await window.keyboard.press(`${modKey}+a`)
  await window.keyboard.press(`${modKey}+x`)
  await expect.poll(() => writeText('write-cut.md'), { timeout: 8000 }).toBe('')

  await window.keyboard.press(`${modKey}+v`)
  await expect.poll(() => writeText('write-cut.md'), { timeout: 8000 }).toBe('one\ntwo')
})

test('a multi-line paste becomes blocks', async ({ window }) => {
  await openWrite(window, 'write-paste.md', 'start')
  await window.locator('.block--paragraph .block__text').first().click()
  await caretAt(window, 0, 'end')

  await window.locator('.blocks').evaluate((element) => {
    const data = new DataTransfer()
    data.setData('text/plain', '- a\n- b')
    element.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }))
  })

  await expect.poll(() => writeText('write-paste.md'), { timeout: 8000 }).toBe('start\n- a\n- b')
})

test('a markdown shortcut converts a paragraph as it is typed', async ({ window }) => {
  await openWrite(window, 'write-shortcut.md', '')
  await window.locator('.block--paragraph .block__text').first().click()

  await window.keyboard.type('# ')
  await window.keyboard.type('Heading')
  await expect.poll(() => writeText('write-shortcut.md'), { timeout: 8000 }).toBe('# Heading')
})

test('an IME composition lands the composed text once', async ({ window }) => {
  await openWrite(window, 'write-ime.md', '')
  await window.locator('.block--paragraph .block__text').first().click()

  const session = await window.context().newCDPSession(window)
  await session.send('Input.imeSetComposition', { text: 'に', selectionStart: 1, selectionEnd: 1 })
  await session.send('Input.imeSetComposition', { text: 'にほ', selectionStart: 2, selectionEnd: 2 })
  await session.send('Input.insertText', { text: 'にほ' })
  await session.detach()

  await expect.poll(() => writeText('write-ime.md'), { timeout: 8000 }).toBe('にほ')
  const drawn = await window.locator('.blocks').innerText()
  expect(drawn.split('にほ').length - 1).toBe(1)
})

test('the conflict scene passes in Write mode', async ({ window }) => {
  await openWrite(window, 'write-conflict.md', 'original words')
  await window.locator('.block--paragraph .block__text').first().click()
  await caretAt(window, 0, 'end')
  await window.keyboard.type(' and my unsaved words')
  await window.waitForTimeout(300)

  const binary = join(
    REPO_ROOT,
    'rust',
    'target',
    'debug',
    process.platform === 'win32' ? 'tandem.exe' : 'tandem',
  )
  execFileSync(binary, [
    '--home',
    seedPath(),
    'note',
    'write',
    '--path',
    'write-conflict.md',
    '--text',
    'the CLI wrote this while the editor was dirty',
  ])

  await window.evaluate("window.dispatchEvent(new Event('focus'))")
  await expect(window.locator('.conflict')).toBeVisible()
  await expect(window.locator('.conflict__text').nth(0)).toContainText('my unsaved words')
  await expect(window.locator('.conflict__text').nth(1)).toContainText(
    'the CLI wrote this while the editor was dirty',
  )

  await window.locator('.button', { hasText: 'Take theirs' }).click()
  await expect(window.locator('.conflict')).toHaveCount(0)
  await expect(window.locator('.blocks')).toContainText('the CLI wrote this while the editor was dirty')
})

test('opening and leaving a note in Write writes the same bytes', async ({ window }) => {
  const content = '# Keep\n\n* starred item\n\n> quote\n\n```ts\nx\n```\n'
  await openWrite(window, 'write-noop.md', content)
  await window.waitForTimeout(1600)
  expect(writeText('write-noop.md')).toBe(content)
})

test('typing a task marker makes a task block', async ({ window }) => {
  await openWrite(window, 'write-task.md', '')
  await window.locator('.block--paragraph .block__text').first().click()

  await window.keyboard.type('- [ ] Task')
  await expect(window.locator('.block--task')).toBeVisible()
  await expect(window.locator('.block--task .block__check')).toHaveAttribute('aria-checked', 'false')
  await expect.poll(() => writeText('write-task.md'), { timeout: 8000 }).toBe('- [ ] Task')
})

test('the code language field keeps focus and its keys out of the note', async ({ window }) => {
  await openWrite(window, 'write-language.md', '```ts\nconst x = 1\n```')

  const language = window.locator('.block__code-lang')
  await expect(language).toHaveValue('ts')
  await language.fill('')
  await language.click()
  await window.keyboard.type('py')

  await expect(language).toBeFocused()
  await expect(language).toHaveValue('py')
  await expect(window.locator('.block--code .block__text')).toContainText('const x = 1')
  await expect.poll(() => writeText('write-language.md'), { timeout: 8000 }).toBe(
    '```py\nconst x = 1\n```',
  )
})

/** A 1x1 PNG: the bytes prove the protocol served a file, not a placeholder. */
const DOT_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9Z2S8AAAAASUVORK5CYII=',
  'base64',
)

/**
 * The image path over the real app: the Write surface loads a vault file
 * through `tandem:`, and a missing, escaping and remote target all draw the
 * placeholder without a network request.
 */
test('a vault image is served over the tandem protocol, and nothing escapes it', async ({
  window,
}) => {
  mkdirSync(join(seedPath(), 'bikeshed', 'images'), { recursive: true })
  writeFileSync(join(seedPath(), 'bikeshed', 'images', 'dot.png'), DOT_PNG)
  writeFileSync(join(seedPath(), 'bikeshed', 'image.md'), '![the dot](images/dot.png)\n')
  writeFileSync(
    join(seedPath(), 'bikeshed', 'image-bad.md'),
    '![missing](missing.png)\n\n![escape](../escape.png)\n\n![remote](https://example.com/x.png)\n',
  )

  const remote: string[] = []
  window.on('request', (request) => {
    if (/^https?:/i.test(request.url())) remote.push(request.url())
  })

  await openTab(window, 'bikeshed')
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'write')")
  await window.evaluate("window.dispatchEvent(new Event('focus'))")
  await window.locator('.tree__label[title="image.md"]').click()

  const blockImage = window.locator('.block--image img')
  await expect(blockImage).toHaveCount(1)
  await expect
    .poll(() => blockImage.evaluate((element) => (element as HTMLImageElement).naturalWidth))
    .toBe(1)
  await expect(blockImage).toHaveAttribute('alt', 'the dot')
  await expect(blockImage).toHaveAttribute('src', 'tandem:images/dot.png')

  // A missing, escaping and remote target are three placeholders, never an img
  // that could fetch one.
  await window.locator('.tree__label[title="image-bad.md"]').click()
  const blocks = window.locator('.blocks')
  await expect(blocks.locator('.block--image')).toHaveCount(3)
  await expect(blocks.locator('.block--image .block__image-placeholder')).toHaveCount(3)
  await expect(blocks.locator('.block--image img')).toHaveCount(0)
  await expect(blocks.locator('.block__image-alt')).toHaveText(['missing', 'escape', 'remote'])
  expect(remote).toEqual([])
})

/**
 * The Source scene: the fixture note as its exact markdown in the plain
 * textarea, with the titlebar control on Source. The picture and its catalogue
 * row are written by the documented shots run, never by hand.
 */
test("the Source surface holds the note's exact markdown", async ({ app, window }) => {
  const notePath = join(seedPath(), 'bikeshed', 'projects', 'fixture', 'fixture.md')
  const before = readFileSync(notePath, 'utf8')

  await openTab(window, 'bikeshed')
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'write')")
  await window.evaluate("window.dispatchEvent(new Event('focus'))")
  await window.locator('.tree__label[title="projects/fixture/fixture.md"]').click()

  const source = window.locator('.editor__mode-button', { hasText: 'Source' })
  await source.click()
  await expect(source).toHaveAttribute('aria-pressed', 'true')

  const area = window.locator('.source')
  await expect(area).toBeVisible()
  await expect(area).toHaveValue(before)
  await expect(area).toBeFocused()

  await capture(app, window, scene('bikeshed-source'))

  // Viewing the note never rewrites it.
  expect(readFileSync(notePath, 'utf8')).toBe(before)
})
