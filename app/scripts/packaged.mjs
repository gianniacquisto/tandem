import { existsSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

/**
 * Where electron-builder leaves one platform's unpacked app under `dist`, and
 * where the packaged shell resolves the core inside it (`src/main/binary.ts`:
 * `resourcesPath/tandem`, `tandem.exe` on Windows).
 *
 * The names are electron-builder's own defaults: the macOS bundle is
 * `mac`, `mac-arm64` or `mac-universal` with the executable named after
 * `productName`, Linux unpacks to `linux-unpacked` with the package name as
 * its executable, and Windows to `win-unpacked` with `productName.exe`.
 */
const LAYOUTS = {
  darwin: {
    entries: (name) => name.startsWith('mac'),
    bundle: 'Tandem.app',
    executable: 'Contents/MacOS/Tandem',
    resources: 'Contents/Resources',
    binary: 'tandem',
  },
  linux: {
    entries: (name) => name === 'linux-unpacked',
    bundle: null,
    executable: 'tandem-app',
    resources: 'resources',
    binary: 'tandem',
  },
  win32: {
    entries: (name) => name === 'win-unpacked',
    bundle: null,
    executable: 'Tandem.exe',
    resources: 'resources',
    binary: 'tandem.exe',
  },
}

/**
 * The packaged app for `platform` under `dist`: its executable, its resources
 * directory, and the core binary that must sit beside them.
 *
 * A dist with no app, or an app that does not carry the binary, is a failure:
 * that is precisely the bug an installed app would show as "nothing to spawn".
 */
export function findPackagedApp(dist, platform) {
  const layout = LAYOUTS[platform]
  if (layout === undefined) throw new Error(`no packaged app layout for ${platform}`)

  let entries
  try {
    entries = readdirSync(dist, { withFileTypes: true })
  } catch {
    throw new Error(`no packaged app for ${platform}: ${dist} does not exist`)
  }

  const candidates = entries
    .filter((entry) => entry.isDirectory() && layout.entries(entry.name))
    .map((entry) => entry.name)
    .sort()

  for (const entry of candidates) {
    const root = layout.bundle === null ? join(dist, entry) : join(dist, entry, layout.bundle)
    const executable = join(root, layout.executable)
    if (!existsSync(executable)) continue

    const resources = join(root, layout.resources)
    const binary = join(resources, layout.binary)
    if (!existsSync(binary)) {
      throw new Error(`the packaged app carries no ${layout.binary} at ${binary}`)
    }
    return { root, executable, resources, binary }
  }

  throw new Error(`no packaged app for ${platform} under ${dist}`)
}
