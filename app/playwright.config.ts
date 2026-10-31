import { defineConfig } from '@playwright/test'

/**
 * The screenshot suite (`docs/screenshots/README.md`, `docs/port.md`).
 *
 * One worker: every scene is one app and one picture, and the run's whole
 * output is the committed catalogue, so scene order is the manifest's alone.
 * `npm run shots` builds the renderer first; the rpc binary is `npm run
 * binary` (CI builds it in its own step).
 */
export default defineConfig({
  testDir: 'tests/shots',
  testMatch: '**/*.shots.ts',
  globalSetup: './shots/globalSetup.ts',
  globalTeardown: './shots/globalTeardown.ts',
  workers: 1,
  timeout: 60_000,
  expect: { timeout: 15_000 },
  forbidOnly: process.env['CI'] !== undefined,
  reporter: process.env['CI'] !== undefined ? [['github'], ['list']] : [['list']],
})
