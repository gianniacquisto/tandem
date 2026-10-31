import { spawnSync } from 'node:child_process'
import { existsSync, lstatSync, readlinkSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { findPackagedApp } from './packaged.mjs'

/**
 * The artifact-side half of the app's package gate: the platform's unpacked
 * app exists under `dist/`, carries the core where `src/main/binary.ts`
 * resolves it, and carries the agent face ADR 0034 installs from. On macOS it
 * also carries the `tandem-cli` symlink the cask links into Homebrew's `bin`,
 * so a packaging change that drops it fails here rather than at install time.
 *
 * The smoke launch (`smoke-packaged.mjs`) adds the runtime proof on macOS and
 * Linux; this check is the one every platform can run.
 *
 * macOS is ad-hoc signed at package time (`identity: "-"` in
 * `electron-builder.yml`), and the v1.0.0 shape shipped without that: the
 * main executable kept the linker's ad-hoc signature with
 * `Sealed Resources=none`, `codesign --verify --deep --strict` failed, and
 * macOS called the app damaged. So the signature is a gate, not a detail:
 * a bundle that does not verify, or does not seal its resources, fails here.
 */
const appRoot = fileURLToPath(new URL('..', import.meta.url))
const app = findPackagedApp(join(appRoot, 'dist'), process.platform)
const face = join(app.resources, 'agent')
for (const file of ['AGENTS.md', 'VERSION', join('skills', 'tandem', 'SKILL.md')]) {
  if (!existsSync(join(face, file))) {
    console.error(`the packaged app carries no agent face at ${join(face, file)}`)
    process.exit(1)
  }
}

if (process.platform === 'darwin') {
  const link = join(app.root, 'Contents', 'MacOS', 'tandem-cli')
  if (!existsSync(link) || !lstatSync(link).isSymbolicLink()) {
    console.error(`the packaged app carries no tandem-cli symlink at ${link}`)
    process.exit(1)
  }
  const target = readlinkSync(link)
  if (target !== '../Resources/tandem') {
    console.error(`the tandem-cli symlink names ${target}, expected ../Resources/tandem`)
    process.exit(1)
  }

  const verify = spawnSync('codesign', ['--verify', '--deep', '--strict', app.root], {
    encoding: 'utf8',
  })
  if (verify.status !== 0) {
    console.error(
      `the packaged app does not verify: codesign --verify --deep --strict ${app.root}\n${`${verify.stdout}${verify.stderr}`.trim()}`,
    )
    process.exit(1)
  }

  // `--verbose=2` prints the seal: `Sealed Resources version=2 rules=… files=…`
  // on a signed bundle, `Sealed Resources=none` on the linker-signed shape.
  const display = spawnSync('codesign', ['--display', '--verbose=2', app.root], {
    encoding: 'utf8',
  })
  const signature = `${display.stdout}${display.stderr}`
  if (display.status !== 0) {
    console.error(
      `codesign cannot read the packaged app's signature: codesign --display --verbose=2 ${app.root}\n${signature.trim()}`,
    )
    process.exit(1)
  }
  if (!/Sealed Resources(?: version)?=(?!none)\S/.test(signature)) {
    console.error(
      `the packaged app does not seal its resources: codesign --display --verbose=2 ${app.root}\n${signature.trim()}`,
    )
    process.exit(1)
  }
}

console.log(`the packaged app carries the core at ${app.binary}, the face at ${face}, and the tandem-cli link`)
