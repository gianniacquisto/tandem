import { defineConfig } from 'vitest/config'

/**
 * The build gate's suite: tests that read `out/renderer`, so they run after
 * `npm run build` (`npm run test:build`; app.yml's build gate). The default
 * `npm test` excludes them because a clean checkout has no build to read.
 */
export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/lazy-chunks.test.ts'],
  },
})
