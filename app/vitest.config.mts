import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    // The lazy-chunk guard reads the built renderer, so it runs after
    // `npm run build` through `npm run test:build` (vitest.build.config.mts).
    exclude: ['tests/lazy-chunks.test.ts'],
  },
})
