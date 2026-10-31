import { rmSync } from 'node:fs'

import { writeCatalogue } from './catalogue'
import { seedPath } from './home'
import { REPO_ROOT } from './roots'

/**
 * The catalogue is kept by the same run that takes the pictures, and the
 * scratch home is dropped afterwards. A scene whose PNG is missing throws
 * here, so a green run and a documented picture cannot disagree.
 */
export default function globalTeardown(): void {
  try {
    writeCatalogue({ repoRoot: REPO_ROOT })
  } finally {
    rmSync(seedPath(REPO_ROOT), { recursive: true, force: true })
  }
}
