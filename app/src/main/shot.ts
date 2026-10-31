import { resolve, sep } from 'node:path'

/**
 * The screenshot rule, as a predicate: a picture may only be taken against a
 * home under the repository's own `Fixtures/` (`docs/screenshots/README.md`),
 * so a fixture board is what reaches a public repository and never a human's
 * real tickets or paths.
 */
export function isFixtureHome(home: string | undefined, fixturesRoot: string): boolean {
  if (home === undefined || home.trim() === '') return false

  const target = resolve(home)
  const root = resolve(fixturesRoot)
  return target === root || target.startsWith(`${root}${sep}`)
}
