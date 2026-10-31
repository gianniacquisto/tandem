import { spawnSync } from 'node:child_process'
import { chmodSync, copyFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * Build the core in release configuration and stage it under the name the
 * packaged shell resolves (`src/main/binary.ts`): `tandem` beside the app's
 * resources, `tandem.exe` on Windows.
 *
 * The cargo call runs from `rust/` so the proxy reads `rust-toolchain.toml`,
 * the same pinned toolchain as `rust.yml`; a checkout on a machine with a
 * different default would otherwise package a build CI never tested.
 */
const appRoot = fileURLToPath(new URL('..', import.meta.url))
const rust = join(appRoot, '..', 'rust')
const name = process.platform === 'win32' ? 'tandem.exe' : 'tandem'

const built = spawnSync('cargo', ['build', '--release', '-p', 'tandem'], {
  cwd: rust,
  stdio: 'inherit',
})
if (built.error !== undefined) {
  console.error(`could not run cargo: ${built.error.message}`)
  process.exit(1)
}
if (built.status !== 0) process.exit(built.status ?? 1)

const staged = join(appRoot, 'build')
mkdirSync(staged, { recursive: true })
const destination = join(staged, name)
copyFileSync(join(rust, 'target', 'release', name), destination)
chmodSync(destination, 0o755)
console.log(`staged the core at ${destination}`)
