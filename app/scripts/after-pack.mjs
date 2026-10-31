import { rmSync, symlinkSync } from 'node:fs'
import { join } from 'node:path'

/**
 * The cask links `Tandem.app/Contents/MacOS/tandem-cli` into Homebrew's `bin`
 * (ADR 0009, ADR 0003), and every skill calls the binary by that name. The
 * cutover's one binary is the Rust core at `Contents/Resources/tandem`
 * (`src/main/binary.ts`), so macOS gets a symlink at the name the cask and the
 * agent face already speak. It is one executable, not a copy.
 *
 * Other platforms have no cask and no symlink: their installs name the core
 * directly.
 */
export default async function afterPack(context) {
  if (context.electronPlatformName !== 'darwin') return

  const app = join(context.appOutDir, `${context.packager.appInfo.productFilename}.app`)
  const link = join(app, 'Contents', 'MacOS', 'tandem-cli')
  rmSync(link, { force: true })
  symlinkSync('../Resources/tandem', link)
}
