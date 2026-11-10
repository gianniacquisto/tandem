/**
 * Image target resolution, the renderer side of the `tandem:` asset path.
 *
 * Pure: no DOM, no fetch. The classifier is the committed editor bundle's
 * (`Editor/src/pure/image.js`) with its rules unchanged, plus the host-path
 * cases the core also refuses (a backslash, a drive prefix, a control
 * character, or a percent-decoded `/` inside a segment). `assetUrl` percent-
 * encodes one segment at a time so `#`, `?`, `%` and a space survive the URL
 * while `/` stays the vault's separator.
 *
 * The protocol handler (`app/src/main/assets.ts`) re-checks the same rule and
 * asks the core, which owns path confinement: a target this module calls
 * `bikeshed` in the vault is still a 404 when the file is not an image.
 */

/** What a markdown image's target means to the surface. */
export type ImageTarget =
  | { kind: 'remote'; src: string }
  | { kind: 'bikeshed'; path: string }
  | { kind: 'escape' }
  | { kind: 'invalid' }

const REMOTE = /^https?:\/\//i
const DRIVE = /^[A-Za-z]:/
const CONTROL = /[\u0000-\u001f\u007f]/

/** A segment the host or the core would reinterpret, refused before it can. */
function invalidSegment(segment: string): boolean {
  if (segment.includes('\\') || DRIVE.test(segment) || CONTROL.test(segment)) return true
  let decoded: string
  try {
    decoded = decodeURIComponent(segment)
  } catch {
    // A literal `%` that is not an escape is a legal file-name byte; the core
    // decides whether that file exists, and `assetUrl` re-encodes it.
    return false
  }
  return decoded.includes('/') || decoded.includes('\\') || CONTROL.test(decoded)
}

export function resolveImageTarget(src: string, notePath: string): ImageTarget {
  if (REMOTE.test(src)) return { kind: 'remote', src }

  let raw = src
  let noteDir = ''
  if (raw.startsWith('/')) {
    raw = raw.slice(1) // bikeshed-root-relative: the note's folder does not apply
  } else {
    noteDir = notePath.includes('/')
      ? notePath.slice(0, notePath.lastIndexOf('/') + 1)
      : ''
  }
  const segments = (noteDir + raw).split('/')
  const out: string[] = []
  for (const segment of segments) {
    if (segment === '' || segment === '.') continue
    if (segment === '..') {
      if (out.length === 0) return { kind: 'escape' }
      out.pop()
      continue
    }
    if (invalidSegment(segment)) return { kind: 'invalid' }
    out.push(segment)
  }
  return { kind: 'bikeshed', path: out.join('/') }
}

/** A vault path as the `<img>` src: `tandem:` plus one encoded segment each. */
export function assetUrl(path: string): string {
  const encoded = path.split('/').map(encodeURIComponent).join('/')
  return `tandem:${encoded}`
}
