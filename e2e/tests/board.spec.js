// @ts-check
const { test, expect } = require('@playwright/test');
const { startServer } = require('./harness');

test.describe('Tandem serve browser suite', () => {
  test('1. drag a card from Backlog to Ready; store updates and a second tab redraws via SSE', async ({ page, context }, testInfo) => {
    const srv = await startServer();
    try {
      const page2 = await context.newPage();
      await page.goto(`${srv.baseURL}/board`);
      await page2.goto(`${srv.baseURL}/board`);

      const backlogCol = page.locator('.board__column[data-status="backlog"]');
      const readyCol = page.locator('.board__column[data-status="ready"]');
      await expect(backlogCol.locator('.ticket-card')).toContainText('First card');
      await expect(page2.locator('.board__column[data-status="backlog"] .ticket-card')).toContainText('First card');

      await backlogCol.locator('.ticket-card').dragTo(readyCol);

      await expect(readyCol.locator('.ticket-card')).toContainText('First card');
      await expect(page2.locator('.board__column[data-status="ready"] .ticket-card')).toContainText('First card');

      const shown = JSON.parse(srv.cli(['ticket', 'show', '--id', srv.ticket.ref]));
      expect(shown.status).toBe('ready');

      await testInfo.attach('end-state', {
        body: await page.screenshot({ fullPage: true }),
        contentType: 'image/png',
      });
    } finally {
      await srv.stop();
    }
  });

  test('2. type in a note, wait for autosave, reload, the text is there', async ({ page }, testInfo) => {
    const srv = await startServer();
    try {
      await page.goto(`${srv.baseURL}/notes?path=Index.md`);
      const editor = page.locator('[data-live]');
      await expect(editor).toBeVisible();

      const savePromise = page.waitForResponse((res) => res.url().includes('/notes/save') && res.status() === 200);
      await editor.click();
      await page.keyboard.press('End');
      await page.keyboard.type('\nAutosaved line here');
      await savePromise;

      await page.reload();
      await expect(page.locator('[data-live]')).toContainText('Autosaved line here');

      const noteOnDisk = { text: srv.readNote() };
      expect(noteOnDisk.text).toContain('Autosaved line here');

      await testInfo.attach('end-state', {
        body: await page.screenshot({ fullPage: true }),
        contentType: 'image/png',
      });
    } finally {
      await srv.stop();
    }
  });

  test('3. type in a note and navigate away immediately; the flush on leave saves it', async ({ page }, testInfo) => {
    const srv = await startServer();
    try {
      await page.goto(`${srv.baseURL}/notes?path=Index.md`);
      const editor = page.locator('[data-live]');
      await expect(editor).toBeVisible();

      await editor.click();
      await page.keyboard.press('End');
      await page.keyboard.type('\nFlushed on leave');

      // Navigate away immediately inside the 1000ms debounce window.
      await page.locator('a.shell__tab[href^="/board"]').click();
      // The keepalive save outlives the page, so assert on the file, not the response.
      await expect.poll(() => srv.readNote()).toContain('Flushed on leave');
      await expect(page.locator('#board-root')).toBeVisible();

      await page.goto(`${srv.baseURL}/notes?path=Index.md`);
      await expect(page.locator('[data-live]')).toContainText('Flushed on leave');

      await testInfo.attach('end-state', {
        body: await page.screenshot({ fullPage: true }),
        contentType: 'image/png',
      });
    } finally {
      await srv.stop();
    }
  });

  test('4. edit a note in the browser after changing it on disk; conflict notice appears and Keep mine wins', async ({ page }, testInfo) => {
    const srv = await startServer();
    try {
      await page.goto(`${srv.baseURL}/notes?path=Index.md`);
      const editor = page.locator('[data-live]');
      await expect(editor).toBeVisible();

      // Modify the note on disk behind the browser's back.
      srv.writeNote('# Welcome\nDisk change from outside\n');

      // Edit in the browser so the debounced save hits the stale baseline.
      await editor.click();
      await page.keyboard.press('End');
      await page.keyboard.type('\nBrowser change wins');

      const conflict = page.locator('.conflict');
      await expect(conflict).toBeVisible();
      await expect(conflict).toContainText('This note changed on disk');
      await expect(conflict).toContainText('Disk change from outside');

      await conflict.getByRole('button', { name: 'Keep mine' }).click();
      await expect(conflict).toBeHidden();
      await expect(page.locator('[data-live]')).toContainText('Browser change wins');

      const noteOnDisk = { text: srv.readNote() };
      expect(noteOnDisk.text).toContain('Browser change wins');
      expect(noteOnDisk.text).not.toContain('Disk change from outside');

      await testInfo.attach('end-state', {
        body: await page.screenshot({ fullPage: true }),
        contentType: 'image/png',
      });
    } finally {
      await srv.stop();
    }
  });

  test('5. open a ticket sheet, add a comment, the comment is in the log', async ({ page }, testInfo) => {
    const srv = await startServer();
    try {
      await page.goto(`${srv.baseURL}/board`);
      await page.locator('.ticket-card', { hasText: 'First card' }).click();

      const sheet = page.locator('.sheet');
      await expect(sheet).toBeVisible();
      await expect(sheet).toContainText('Nothing in the log yet.');

      await sheet.getByLabel('Add a comment').fill('Verified from the browser suite');
      await sheet.getByRole('button', { name: 'Post' }).click();

      await expect(page.locator('.sheet__comment')).toContainText('Verified from the browser suite');

      const comments = JSON.parse(srv.cli(['ticket', 'comment', 'list', '--id', srv.ticket.ref]));
      expect(comments.comments).toHaveLength(1);
      expect(comments.comments[0].body).toBe('Verified from the browser suite');

      await testInfo.attach('end-state', {
        body: await page.screenshot({ fullPage: true }),
        contentType: 'image/png',
      });
    } finally {
      await srv.stop();
    }
  });

  test('6. keyboard: the shortcuts the board documents move a card and open a ticket', async ({ page }, testInfo) => {
    const srv = await startServer();
    try {
      await page.goto(`${srv.baseURL}/board`);
      const card = page.locator('.board__column[data-status="backlog"] .ticket-card');
      await card.focus();
      await expect(card).toBeFocused();

      // Alt+Right moves the focused card to the next column (On hold).
      await page.keyboard.press('Alt+ArrowRight');

      const movedCard = page.locator('.board__column[data-status="on-hold"] .ticket-card');
      await expect(movedCard).toContainText('First card');
      await expect(movedCard).toBeFocused();

      // Enter on the focused card opens its ticket sheet.
      await page.keyboard.press('Enter');
      await expect(page.locator('.sheet')).toBeVisible();
      await expect(page.locator('.sheet__handle')).toContainText(srv.ticket.ref);

      await testInfo.attach('end-state', {
        body: await page.screenshot({ fullPage: true }),
        contentType: 'image/png',
      });
    } finally {
      await srv.stop();
    }
  });

  test('7. no console error and no CSP violation on the board, a sheet and a note page', async ({ page }, testInfo) => {
    const srv = await startServer();
    try {
      /** @type {string[]} */
      const consoleErrors = [];
      /** @type {string[]} */
      const pageErrors = [];
      /** @type {string[]} */
      const cspViolations = [];

      await page.exposeFunction('recordCspViolation', (detail) => {
        cspViolations.push(detail);
      });
      await page.addInitScript(() => {
        document.addEventListener('securitypolicyviolation', (e) => {
          // @ts-ignore
          window.recordCspViolation(`${e.violatedDirective}: ${e.blockedURI}`);
        });
      });

      page.on('console', (msg) => {
        if (msg.type() === 'error') {
          consoleErrors.push(msg.text());
        }
      });
      page.on('pageerror', (err) => {
        pageErrors.push(String(err));
      });

      await page.goto(`${srv.baseURL}/board`);
      await expect(page.locator('#board-root')).toBeVisible();

      await page.goto(`${srv.baseURL}/tickets/${srv.ticket.id}`);
      await expect(page.locator('.sheet')).toBeVisible();

      await page.goto(`${srv.baseURL}/notes?path=Index.md`);
      await expect(page.locator('#notes-root')).toBeVisible();
      await expect(page.locator('[data-live]')).toBeVisible();

      expect(consoleErrors).toEqual([]);
      expect(pageErrors).toEqual([]);
      expect(cspViolations).toEqual([]);

      await testInfo.attach('end-state', {
        body: await page.screenshot({ fullPage: true }),
        contentType: 'image/png',
      });
    } finally {
      await srv.stop();
    }
  });

  test('8. the project switcher opens one project at a time, keeps it across Board and Notes, and a bare visit reopens it', async ({ page }, testInfo) => {
    const srv = await startServer();
    try {
      srv.cli(['project', 'add', '--name', 'Other project with a long name that has to truncate']);
      srv.cli(['ticket', 'add', '--project', 'Other project with a long name that has to truncate', '--title', 'Other card']);
      const other = 'Other project with a long name that has to truncate';

      // A bare visit lands on the first project's board.
      await page.goto(`${srv.baseURL}/`);
      await expect(page).toHaveURL(/\/board\?project=Tandem$/);
      const switcher = page.locator('.project-switch__select');
      await expect(switcher).toHaveValue('Tandem');
      await expect(page.locator('.ticket-card', { hasText: 'First card' })).toBeVisible();
      await expect(page.locator('.ticket-card', { hasText: 'Other card' })).toHaveCount(0);

      // Changing the select reloads the same face for the other project.
      await switcher.selectOption(other);
      await expect(page).toHaveURL(/\/board\?project=Other/);
      await expect(page.locator('.ticket-card', { hasText: 'Other card' })).toBeVisible();
      await expect(page.locator('.ticket-card', { hasText: 'First card' })).toHaveCount(0);
      await expect(page.locator('.shell__tab[aria-current="page"]')).toHaveText('Board');

      // Board and Notes keep the project in both directions.
      await page.locator('.shell__tab', { hasText: 'Notes' }).click();
      await expect(page).toHaveURL(/\/notes\?project=Other/);
      await expect(page.locator('.project-switch__select')).toHaveValue(other);
      await expect(page.locator('.shell__tab[aria-current="page"]')).toHaveText('Notes');
      await expect(page.locator('.focus')).toHaveCount(0);
      await page.locator('.shell__tab', { hasText: 'Board' }).click();
      await expect(page).toHaveURL(/\/board\?project=Other/);

      // Back on Notes, then a fresh visit to `/` reopens the last project and face.
      await page.locator('.shell__tab', { hasText: 'Notes' }).click();
      await page.goto(`${srv.baseURL}/`);
      await expect(page).toHaveURL(/\/notes\?project=Other/);

      await testInfo.attach('end-state', {
        body: await page.screenshot({ fullPage: true }),
        contentType: 'image/png',
      });
    } finally {
      await srv.stop();
    }
  });
});
