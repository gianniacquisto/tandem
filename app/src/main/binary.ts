import { join } from 'node:path'

/**
 * Where the shell finds `tandem`. The one binary agents call is the one the
 * app trusts (`docs/port.md`, Target shape), so the lookup is the only place a
 * path is named.
 *
 * Precedence: `TANDEM_BIN` (a checkout or a test can point anywhere), then the
 * copy bundled beside a packaged app's resources, then the workspace build the
 * dev server runs against.
 */
export type BinarySource = {
  env: Record<string, string | undefined>
  packaged: boolean
  platform: NodeJS.Platform
  appPath: string
  resourcesPath: string
}

export function resolveBinary(source: BinarySource): string {
  const named = source.env['TANDEM_BIN']
  if (named !== undefined && named.trim() !== '') return named

  const name = source.platform === 'win32' ? 'tandem.exe' : 'tandem'
  if (source.packaged) return join(source.resourcesPath, name)
  return join(source.appPath, '..', 'rust', 'target', 'debug', name)
}
