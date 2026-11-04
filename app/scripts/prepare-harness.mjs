import { spawnSync } from 'node:child_process'
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * Stage the pinned harness binary into the artifact (ADR 0038, PRT-1): fetch
 * the release asset, verify the extracted binary's sha256 against the pin,
 * and copy it to `build/harness/opencode`, where `electron-builder.yml`'s
 * linux `extraResources` carries it at `resources/harness/opencode`.
 *
 * The pin (version, asset, digest) is read from
 * `Scripts/check-harness-home.py --print-pin`, the one source of truth, so a
 * version bump changes one file and this script follows it.
 *
 * Linux only for now: the pin table also has darwin-arm64, but the macOS
 * bundle does not embed the harness yet (the launcher that spawns it is built
 * separately), and Windows has no pin at all (PRT-1's decided-against). Other
 * platforms skip loudly rather than fail, so the shared `dist` script keeps
 * working on the macOS and Windows runners.
 */
const appRoot = fileURLToPath(new URL('..', import.meta.url))
const repoRoot = join(appRoot, '..')

if (process.platform !== 'linux') {
  console.log(`no harness staging on ${process.platform}: PRT-1 stages the linux-x64 pin only`)
  process.exit(0)
}

const pin = spawnSync(
  'python3',
  [join(repoRoot, 'Scripts', 'check-harness-home.py'), '--print-pin'],
  { encoding: 'utf8' },
)
if (pin.status !== 0) {
  console.error(`could not read the pin: ${pin.stderr.trim()}`)
  process.exit(pin.status ?? 1)
}
const vars = Object.fromEntries(
  pin.stdout
    .trim()
    .split('\n')
    .map((line) => line.split('=')),
)
const { PINNED_VERSION, PINNED_ASSET, PINNED_SHA256 } = vars

const work = mkdtempSync(join(tmpdir(), 'tandem-harness-'))
try {
  const asset = join(work, PINNED_ASSET)
  const url = `https://github.com/anomalyco/opencode/releases/download/v${PINNED_VERSION}/${PINNED_ASSET}`
  console.log(`fetching ${url}`)
  const download = spawnSync('curl', ['-sfL', '-o', asset, url], { stdio: 'inherit' })
  if (download.status !== 0) process.exit(download.status ?? 1)

  const extract = spawnSync('tar', ['xzf', asset, '-C', work], { stdio: 'inherit' })
  if (extract.status !== 0) process.exit(extract.status ?? 1)

  const binary = join(work, 'opencode')
  if (!existsSync(binary)) {
    console.error(`the asset ${PINNED_ASSET} carries no opencode binary`)
    process.exit(1)
  }
  const digest = spawnSync('sha256sum', [binary], { encoding: 'utf8' }).stdout.split(' ')[0]
  if (digest !== PINNED_SHA256) {
    console.error(`sha256 ${digest} is not the pin ${PINNED_SHA256}; refusing to stage`)
    process.exit(1)
  }

  const staged = join(appRoot, 'build', 'harness')
  mkdirSync(staged, { recursive: true })
  const destination = join(staged, 'opencode')
  copyFileSync(binary, destination)
  chmodSync(destination, 0o755)
  console.log(`staged the pinned harness v${PINNED_VERSION} (sha256 verified) at ${destination}`)
} finally {
  rmSync(work, { recursive: true, force: true })
}
