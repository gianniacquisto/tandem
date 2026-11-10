import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * The build half of UI-22's retirement: `out/renderer` is one page, with no
 * editor frame, no editor chunk and no copy of the committed editor bundle, and
 * it carries KaTeX's 20 woff2 faces as files beside the CSS rather than as
 * `data:` URLs. The policy keeps `font-src 'self'`, so an inlined face would be
 * a font the page cannot load.
 *
 * Run after `npm run build` (app.yml's "Gate: the renderer is one page with its
 * fonts as files").
 */
const appRoot = fileURLToPath(new URL('..', import.meta.url))
const out = join(appRoot, 'out', 'renderer')

function fail(message) {
  console.error(message)
  process.exit(1)
}

if (!existsSync(out)) fail(`the build carries no renderer at ${out}; run npm run build first`)

function walk(directory) {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry)
    return statSync(path).isDirectory() ? walk(path) : [path]
  })
}

const files = walk(out)
const asPath = (file) => relative(out, file).split(sep).join('/')

const pages = files.filter((file) => file.endsWith('.html'))
if (pages.length !== 1 || asPath(pages[0]) !== 'index.html') {
  fail(
    `the renderer should build one page at index.html, not ${pages.map(asPath).join(', ') || 'none'}`,
  )
}

const frame = files.filter((file) => /editor-frame/.test(asPath(file)))
if (frame.length > 0) fail(`the renderer still builds the editor frame: ${frame.map(asPath).join(', ')}`)

const chunks = files.filter((file) => {
  if (!file.endsWith('.js')) return false
  const name = asPath(file).split('/').pop() ?? ''
  return /^editor(?:[-.]|$)/.test(name)
})
if (chunks.length > 0) fail(`the renderer still builds an editor chunk: ${chunks.map(asPath).join(', ')}`)

const copied = files.filter((file) => asPath(file).startsWith('editor/') || asPath(file).includes('/editor/'))
if (copied.length > 0) {
  fail(`the renderer still copies the committed editor bundle: ${copied.map(asPath).join(', ')}`)
}

const faces = files.filter((file) => /KaTeX_.+\.woff2$/.test(asPath(file)))
if (faces.length !== 20) {
  fail(`the renderer should emit 20 KaTeX woff2 faces, not ${faces.length}`)
}

const styles = files.filter((file) => file.endsWith('.css'))
if (styles.length === 0) fail('the renderer build carries no stylesheet')
for (const style of styles) {
  const css = readFileSync(style, 'utf8')
  const inline = css.match(/url\(\s*["']?data:[^)]*/i)
  if (inline !== null) {
    fail(`the built CSS inlines a font as a data: URL, which font-src 'self' refuses: ${inline[0].slice(0, 80)}`)
  }
}

console.log(
  `the renderer builds one page with ${faces.length} KaTeX woff2 faces, no editor frame, no editor chunk and no committed bundle copy`,
)
