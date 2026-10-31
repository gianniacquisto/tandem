import { spawnSync } from 'node:child_process'
import { chmodSync, copyFileSync, cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * Build the core in release configuration and stage it under the name the
 * packaged shell resolves (`src/main/binary.ts`): `tandem` beside the app's
 * resources, `tandem.exe` on Windows.
 *
 * The same pass stages the agent face (ADR 0034): `AGENTS.md`, `skills/` and
 * `agents/` from the checkout, plus a `VERSION` marker stamped from the app's
 * version. `tandem-cli setup install` reads that copy inside the bundle, so the
 * skills and the CLI they drive ship from one tag.
 *
 * The cargo call runs from `rust/` so the proxy reads `rust-toolchain.toml`,
 * the same pinned toolchain as `rust.yml`; a checkout on a machine with a
 * different default would otherwise package a build CI never tested.
 */
const appRoot = fileURLToPath(new URL('..', import.meta.url))
const repoRoot = join(appRoot, '..')
const rust = join(repoRoot, 'rust')
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

const face = join(staged, 'agent')
rmSync(face, { recursive: true, force: true })
mkdirSync(face, { recursive: true })
copyFileSync(join(repoRoot, 'AGENTS.md'), join(face, 'AGENTS.md'))
cpSync(join(repoRoot, 'skills'), join(face, 'skills'), { recursive: true })
cpSync(join(repoRoot, 'agents'), join(face, 'agents'), { recursive: true })
const version = JSON.parse(readFileSync(join(appRoot, 'package.json'), 'utf8')).version
writeFileSync(join(face, 'VERSION'), `${version}\n`)
console.log(`staged the agent face at ${face}`)
