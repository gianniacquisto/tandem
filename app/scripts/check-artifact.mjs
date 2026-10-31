import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { findPackagedApp } from './packaged.mjs'

/**
 * The artifact-side half of the app's package gate: the platform's unpacked
 * app exists under `dist/` and carries the core where `src/main/binary.ts`
 * resolves it. `smoke-packaged.mjs` adds the launch on macOS and Linux; this
 * check is the one every platform can run.
 */
const appRoot = fileURLToPath(new URL('..', import.meta.url))
const app = findPackagedApp(join(appRoot, 'dist'), process.platform)
console.log(`the packaged app carries the core at ${app.binary}`)
