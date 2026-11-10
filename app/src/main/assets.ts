/**
 * The `tandem:` protocol handler: the transport half of the image path.
 *
 * The renderer loads `tandem:<encoded vault path>`; this answers a streamed
 * file for the app's own document and 404 for everything else. The core owns
 * the confinement rule (`note.asset` resolves the path, refuses a non-image,
 * a missing file and a directory) and the media type; this module owns the
 * door check, the URL parse and the response headers.
 *
 * The door is `request.initiatorOrigin`: the dev server's origin while
 * `ELECTRON_RENDERER_URL` is set, `file://` when the built page is loaded from
 * disk, and nothing else. An absent or `null` initiator is refused, so a
 * future web content in the default session cannot read the vault. The
 * response never carries `Content-Length`: a file replaced between the core's
 * stat and the read must not truncate the body.
 */

export type AssetView = {
  path: string
  file: string
  mediaType: string
  size: number
}

/** What the handler needs from the core: `note.asset`'s reply or its error. */
export type AssetAnswer = {
  asset?: AssetView
  error?: { code: string; message: string }
}

export type AssetDeps = {
  /** The one origin this window's document reports (ADR 0037: UI is a view). */
  allowedInitiator: string
  /** `note.asset` over the rpc stream, already carrying its transport error. */
  call: (path: string) => Promise<AssetAnswer>
  /** The confined file's bytes, streamed, never buffered whole. */
  readFile: (file: string) => Promise<ReadableStream<Uint8Array>>
}

export type AssetRequest = {
  url: string
  initiatorOrigin: string | null
}

const CONTROL = /[\u0000-\u001f\u007f]/
const DRIVE = /^[A-Za-z]:/

/** The origin the app's own document reports, given how it was loaded. */
export function assetInitiator(devServerUrl: string | undefined): string {
  if (devServerUrl === undefined) return 'file://'
  return new URL(devServerUrl).origin
}

/**
 * The vault path a `tandem:` URL names, or `null` for anything that is not one
 * clean path: another scheme, an empty path, an empty component, a dot or
 * double-dot segment, an escape, a query or fragment, a malformed percent
 * escape, a decoded `/` or `\` inside a segment, a control character, or a
 * drive prefix. The core checks the same rules again; this refusal keeps an
 * obvious bad URL off the rpc stream.
 */
export function assetPathFromUrl(url: string): { path: string } | null {
  if (!/^tandem:/i.test(url)) return null
  const rest = url.slice('tandem:'.length)
  if (rest === '' || rest.includes('?') || rest.includes('#')) return null

  const path: string[] = []
  for (const segment of rest.split('/')) {
    if (segment === '') return null
    let decoded: string
    try {
      decoded = decodeURIComponent(segment)
    } catch {
      return null
    }
    if (decoded === '' || decoded === '.' || decoded === '..') return null
    if (decoded.includes('/') || decoded.includes('\\')) return null
    if (CONTROL.test(decoded) || DRIVE.test(decoded)) return null
    path.push(decoded)
  }
  return { path: path.join('/') }
}

function refused(): Response {
  return new Response(null, { status: 404 })
}

/**
 * The protocol handler. Every refusal (a foreign initiator, a bad URL, a
 * transport failure, a domain error, a missing asset, an unreadable file) is a
 * bare 404, which the `<img>`'s own error path turns into the placeholder.
 */
export async function respondToAsset(
  request: AssetRequest,
  deps: AssetDeps,
): Promise<Response> {
  if (request.initiatorOrigin !== deps.allowedInitiator) return refused()

  const parsed = assetPathFromUrl(request.url)
  if (parsed === null) return refused()

  let answer: AssetAnswer
  try {
    answer = await deps.call(parsed.path)
  } catch {
    return refused()
  }
  if (answer.asset === undefined) return refused()

  try {
    const body = await deps.readFile(answer.asset.file)
    return new Response(body, {
      status: 200,
      headers: {
        'Content-Type': answer.asset.mediaType,
        'X-Content-Type-Options': 'nosniff',
        'Cache-Control': 'no-store',
      },
    })
  } catch {
    return refused()
  }
}
