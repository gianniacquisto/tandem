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
 * The inline runs in Write: a staged note holds a bold run, a wikilink chip and
 * a display-math block drawn as its source lines, the document column scrolls
 * under a sticky breadcrumb, and the footer names the draft a killed save left.
 * The picture is re-taken from this head by the repo's shot run.
 */
test('the Write surface draws the inline runs and its footer', async ({ app, window }) => {
  writeFileSync(
    join(seedPath(), 'bikeshed', '.note-0000-0000-0000-000000000001.tmp'),
    'a draft from a save that was killed',
  )
  writeFileSync(
    writeFile('inline-runs.md'),
    '# Inline runs\n\nA **bold run** and a [[Index]] chip.\n\n$$\nx = 1\n$$\n',
  )

  await openTab(window, 'bikeshed')
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'write')")
  await window.evaluate("window.dispatchEvent(new Event('focus'))")
  await window.locator('.tree__label[title="inline-runs.md"]').click()

  const write = window.locator('.editor__mode-button', { hasText: 'Write' })
  await expect(write).toHaveAttribute('aria-pressed', 'true')

  const blocks = window.locator('.blocks')
  await expect(blocks.locator('strong')).toHaveText('bold run')
  await expect(blocks.locator('.run--wikilink .wikilink-chip')).toHaveText('Index')
  await expect(blocks.locator('.run--wikilink')).toHaveAttribute('data-target', 'Index')
  await expect(blocks.locator('.block--math')).toContainText('x = 1')

  const footer = window.locator('.note-footer')

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
  await expect(footer.locator('.note-footer__recovery')).toContainText(
    'Unsaved draft kept from an interrupted save',
  )
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

/**
 * The `/` menu over the real app: a `/` in an empty text block opens the
 * listbox, typing filters it, the arrows and Home/End move the selection, Enter
 * and Tab apply it, Escape closes it and leaves the typed query, and an empty
 * result keeps the menu open on its no-match row.
 */
test('the slash menu opens, filters and applies', async ({ window }) => {
  await openWrite(window, 'write-slash.md', '')
  const blocks = window.locator('.blocks')
  const menu = window.locator('.block__menu')

  await expect(window.locator('.block__placeholder')).toHaveText("Type '/' for commands")
  await blocks.locator('.block--paragraph .block__text').first().click()
  await window.keyboard.type('/')

  await expect(menu).toBeVisible()
  await expect(menu).toHaveAttribute('role', 'listbox')
  await expect(menu.locator('.block__menu-head')).toHaveText('Blocks')
  await expect(menu.locator('.block__menu-label')).toHaveText([
    'Text',
    'Heading 1',
    'Heading 2',
    'Heading 3',
    'Bulleted list',
    'Numbered list',
    'Task list',
    'Quote',
    'Code',
    'Divider',
  ])
  // The block keeps focus and names the menu and its selected row.
  await expect(blocks).toBeFocused()
  await expect(blocks).toHaveAttribute('aria-controls', 'block-command-menu')
  await expect(blocks).toHaveAttribute('aria-activedescendant', 'block-command-menu-0')

  await window.keyboard.type('heading')
  await expect(menu.locator('.block__menu-label')).toHaveText([
    'Heading 1',
    'Heading 2',
    'Heading 3',
  ])
  // Spaces are part of a query: the filter is a substring over label and keywords.
  await window.keyboard.type(' 2')
  await expect(menu.locator('.block__menu-label')).toHaveText(['Heading 2'])

  // Tab applies the selected row: a Heading 2 that still holds the caret.
  await window.keyboard.press('Tab')
  await expect(menu).toHaveCount(0)
  await expect(window.locator('.block--heading')).toHaveAttribute('data-level', '2')
  await expect(blocks).toBeFocused()
  await window.keyboard.type('Title')
  await expect.poll(() => writeText('write-slash.md'), { timeout: 8000 }).toBe('## Title')

  // A no-match query keeps the menu open on its row; Escape is the way out.
  await window.keyboard.press('Enter')
  await window.keyboard.type('/zzz')
  await expect(menu).toBeVisible()
  await expect(menu.locator('.block__menu-empty')).toHaveText('No blocks match')
  await expect(menu.locator('[role="option"]')).toHaveCount(0)

  await window.keyboard.press('Escape')
  await expect(menu).toHaveCount(0)
  await window.keyboard.type('kept')
  await expect(menu).toHaveCount(0)
  await expect(window.locator('.block--paragraph').last()).toContainText('/zzzkept')

  // Backspace back to the bare `/` reopens the menu; the keys navigate it.
  for (let i = 0; i < 7; i++) await window.keyboard.press('Backspace')
  await expect(menu).toBeVisible()
  await window.keyboard.press('ArrowDown')
  await expect(blocks).toHaveAttribute('aria-activedescendant', 'block-command-menu-1')
  await expect(menu.locator('.block__menu-row--active .block__menu-label')).toHaveText(
    'Heading 1',
  )
  await window.keyboard.press('End')
  await expect(menu.locator('.block__menu-row--active .block__menu-label')).toHaveText('Divider')
  await window.keyboard.press('Home')
  await expect(menu.locator('.block__menu-row--active .block__menu-label')).toHaveText('Text')
  await window.keyboard.press('ArrowUp')
  await expect(menu.locator('.block__menu-row--active .block__menu-label')).toHaveText('Divider')

  // Enter applies: a divider that leaves an editable block after it.
  await window.keyboard.press('Enter')
  await expect(menu).toHaveCount(0)
  await expect(window.locator('.block--rule')).toHaveCount(1)
  await expect(blocks).toBeFocused()
  await window.keyboard.type('after')
  await expect.poll(() => writeText('write-slash.md'), { timeout: 8000 }).toBe(
    '## Title\n---\n\nafter',
  )
})

test('the slash menu stays out of non-text blocks', async ({ window }) => {
  const content = 'a paragraph\n\n```\ncode\n```\n\n| a | b |\n| - | - |'
  await openWrite(window, 'write-slash-literal.md', content)
  const menu = window.locator('.block__menu')

  // A non-empty paragraph keeps a `/` literal: the menu needs an empty block.
  await window.locator('.block--paragraph .block__text').first().click()
  await caretAt(window, 0, 'end')
  await window.keyboard.type('/')
  await expect(menu).toHaveCount(0)
  await expect(window.locator('.block--paragraph').first()).toContainText('a paragraph/')

  // A code block takes a literal `/` too.
  await window.locator('.block--code .block__text').click()
  await caretAt(window, 1, 'start')
  await window.keyboard.type('/')
  await expect(menu).toHaveCount(0)
  await expect(window.locator('.block--code .block__text')).toContainText('/code')

  // The read-only source card has no caret and no menu.
  await window.locator('.block--opaque').click()
  await expect(menu).toHaveCount(0)

  await expect.poll(() => writeText('write-slash-literal.md'), { timeout: 8000 }).toBe(
    'a paragraph/\n\n```\n/code\n```\n\n| a | b |\n| - | - |',
  )

  // A block with more than one row has no single query: the menu stays out, so
  // a two-item list cannot lose its second item to a conversion.
  await openWrite(window, 'write-slash-list.md', '- x\n- b')
  await window.locator('.block--list .block__text').first().click()
  await caretAt(window, 0, 'end')
  await window.keyboard.press('Backspace')
  await window.keyboard.type('/')
  await expect(menu).toHaveCount(0)
  await expect.poll(() => writeText('write-slash-list.md'), { timeout: 8000 }).toBe('- /\n- b')
})

test('the add-below handle adds and focuses a paragraph', async ({ window }) => {
  await openWrite(window, 'write-add-below.md', 'one\n\n- a\n- b')
  const shells = window.locator('.block-shell')
  const first = shells.first()

  await first.hover()
  await expect(first.locator('.block__handles')).toHaveCSS('opacity', '1')
  await first.getByRole('button', { name: 'Add a block below' }).click()

  await expect(window.locator('.block--paragraph')).toHaveCount(2)
  await expect(window.locator('.blocks')).toBeFocused()
  await window.keyboard.type('two')
  await expect.poll(() => writeText('write-add-below.md'), { timeout: 8000 }).toBe(
    'one\n\ntwo\n\n- a\n- b',
  )

  // The handles also show on focus alone, on the block that holds the caret.
  await window.mouse.move(2, 2)
  await window.locator('.block--list .block__text').first().click()
  await expect(shells.nth(2).locator('.block__handles')).toHaveCSS('opacity', '1')
  await expect(shells.nth(0).locator('.block__handles')).toHaveCSS('opacity', '0')

  // A read-only card takes an add-below: the plus is the way past a card.
  await openWrite(window, 'write-add-card.md', '| a | b |\n| - | - |\n\ntail')
  const card = shells.first()
  await expect(card.locator('.block--opaque')).toBeVisible()
  await card.hover()
  await card.getByRole('button', { name: 'Add a block below' }).click()
  await window.keyboard.type('inserted')
  await expect.poll(() => writeText('write-add-card.md'), { timeout: 8000 }).toBe(
    '| a | b |\n| - | - |\n\ninserted\n\ntail',
  )

  // A last-block divider conversion leaves somewhere to type.
  await openWrite(window, 'write-divider.md', '')
  await window.locator('.block--paragraph .block__text').first().click()
  await window.keyboard.type('/divider')
  await window.keyboard.press('Enter')
  await expect(window.locator('.block--rule')).toHaveCount(1)
  await expect(window.locator('.blocks')).toBeFocused()
  await window.keyboard.type('after')
  await expect.poll(() => writeText('write-divider.md'), { timeout: 8000 }).toBe(
    '---\n\nafter',
  )
})

test('the drag and the keyboard move reorder a block', async ({ window }) => {
  await openWrite(window, 'write-drag.md', 'a\n\nb\n\nc')
  const shells = window.locator('.block-shell')
  await expect(shells).toHaveCount(3)

  // The keyboard path moves the focused block one position and keeps it focused.
  await window.locator('.block--paragraph .block__text').first().click()
  await caretAt(window, 0, 'start')
  await window.keyboard.press(`${modKey}+Shift+ArrowDown`)
  await expect(window.locator('.blocks')).toBeFocused()
  await expect.poll(() => writeText('write-drag.md'), { timeout: 8000 }).toBe('b\n\na\n\nc')

  const grip = shells
    .first()
    .getByRole('button', { name: 'Drag to reorder; Mod+Shift+Arrow moves it' })

  await shells.first().hover()
  const from = await grip.boundingBox()
  const to = await shells.nth(2).boundingBox()
  expect(from).not.toBeNull()
  expect(to).not.toBeNull()

  await window.mouse.move(from!.x + from!.width / 2, from!.y + from!.height / 2)
  await window.mouse.down()
  await window.mouse.move(to!.x + 20, to!.y + to!.height - 3, { steps: 8 })

  // The source dims and a 2px accent line marks the target's bottom edge.
  const drop = window.locator('.block-shell--drop-below .block__drop')
  await expect(drop).toBeVisible()
  await expect(drop).toHaveCSS('height', '2px')
  await expect(drop).toHaveCSS('background-color', token('accent'))
  await expect(shells.first()).toHaveClass(/block-shell--dragging/)

  await window.mouse.up()
  await expect(window.locator('.block__drop')).toHaveCount(0)
  await expect.poll(() => writeText('write-drag.md'), { timeout: 8000 }).toBe('a\n\nc\n\nb')

  // A drop on the source is a no-op.
  const before = writeText('write-drag.md')
  const self = shells.first()
  const selfBox = await self.boundingBox()
  const selfGrip = await self
    .getByRole('button', { name: 'Drag to reorder; Mod+Shift+Arrow moves it' })
    .boundingBox()
  await window.mouse.move(selfGrip!.x + 8, selfGrip!.y + 8)
  await window.mouse.down()
  await window.mouse.move(selfBox!.x + 20, selfBox!.y + 4, { steps: 6 })
  await window.mouse.up()
  await window.waitForTimeout(400)
  expect(writeText('write-drag.md')).toBe(before)
})

test('the handles are absent outside Write', async ({ window }) => {
  await openWrite(window, 'write-hands.md', 'one\n\ntwo')
  await expect(window.locator('.block__handles')).toHaveCount(2)
  await expect(window.locator('.block__menu')).toHaveCount(0)

  // Source is the exact bytes in a textarea: no blocks, no gutter, no menu.
  await window.locator('.editor__mode-button', { hasText: 'Source' }).click()
  await expect(window.locator('.blocks')).toHaveCount(0)
  await expect(window.locator('.block__handles')).toHaveCount(0)
  await expect(window.locator('.block__menu')).toHaveCount(0)
  await window.locator('.editor__mode-button', { hasText: 'Write' }).click()
  await expect(window.locator('.blocks')).toBeVisible()

  // A read-only card in Write keeps both handles, and its grip drags the block.
  await openWrite(window, 'write-card-drag.md', '| a | b |\n| - | - |\n\ntail')
  const card = window.locator('.block-shell').first()
  await expect(card.locator('.block--opaque')).toBeVisible()
  await expect(card.locator('.block__handles')).toHaveCount(1)
  await expect(
    card.getByRole('button', { name: 'Add a block below' }),
  ).toBeVisible()

  const grip = await card
    .getByRole('button', { name: 'Drag to reorder; Mod+Shift+Arrow moves it' })
    .boundingBox()
  const tail = await window.locator('.block-shell').nth(1).boundingBox()
  await window.mouse.move(grip!.x + 8, grip!.y + 8)
  await window.mouse.down()
  await window.mouse.move(tail!.x + 20, tail!.y + tail!.height - 3, { steps: 8 })
  await window.mouse.up()
  await expect.poll(() => writeText('write-card-drag.md'), { timeout: 8000 }).toBe(
    'tail\n\n| a | b |\n| - | - |',
  )
})

/**
 * The menu and the drop line are the app's own surfaces: paper, line, shadow
 * and the card radius, the selected row on parchment, the glyph dim, and the
 * same in night. Neither reflows the text column, and a block at the window's
 * floor opens its menu upward.
 */
test('the menu and indicator use the tokens', async ({ window }) => {
  await openWrite(window, 'write-tokens.md', 'a\n\nb')
  const blocks = window.locator('.blocks')
  await window.locator('.block--paragraph .block__text').nth(1).click()
  await caretAt(window, 1, 'end')
  await window.keyboard.press('Enter')

  const textX = (await window.locator('.block--paragraph .block__text').last().boundingBox())?.x
  await window.keyboard.type('/')
  const menu = window.locator('.block__menu')
  await expect(menu).toBeVisible()

  await expect(menu).toHaveCSS('background-color', token('paper'))
  await expect(menu).toHaveCSS('border-top-color', token('line'))
  await expect(menu).toHaveCSS('border-top-left-radius', token('radius-card'))
  const active = menu.locator('.block__menu-row--active')
  await expect(active).toHaveCSS('background-color', token('parchment'))
  await expect(active.locator('.block__menu-glyph')).toHaveCSS('color', token('ink-dim'))

  // The menu hangs off the block: the text column does not move for it.
  expect((await window.locator('.block--paragraph .block__text').last().boundingBox())?.x).toBe(
    textX,
  )

  await window.emulateMedia({ colorScheme: 'dark' })
  await expect(menu).toHaveCSS('background-color', token('paper', 'night'))
  await expect(active).toHaveCSS('background-color', token('parchment', 'night'))
  await window.emulateMedia({ colorScheme: 'light' })
  await expect(menu).toHaveCSS('background-color', token('paper'))

  // A block at the bottom of the window opens its menu upward.
  const lines = Array.from({ length: 40 }, (_, index) => `line ${index}`).join('\n\n')
  await openWrite(window, 'write-flip.md', lines)
  await window.locator('.bikeshed__document').evaluate((element) => {
    element.scrollTop = element.scrollHeight
  })
  await window.locator('.block--paragraph .block__text').last().click()
  await caretAt(window, 39, 'end')
  await window.keyboard.press('Enter')
  await window.keyboard.type('/')

  const flipped = window.locator('.block__menu--above')
  await expect(flipped).toBeVisible()
  const menuBox = await flipped.boundingBox()
  const blockBox = await window.locator('.block-shell').last().boundingBox()
  expect(menuBox!.y + menuBox!.height).toBeLessThanOrEqual(blockBox!.y + 2)
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
