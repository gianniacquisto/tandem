import { join } from 'node:path'

/**
 * The app's checkout and the repository around it.
 *
 * `package.json` has no `"type": "module"`, so the test runner loads this tree
 * as CommonJS and `__dirname` is the only root that is true in both the runner
 * and a plain Node process (`home.ts` stays root-free, so vitest can import it).
 */
export const APP_ROOT = join(__dirname, '..')
export const REPO_ROOT = join(APP_ROOT, '..')
