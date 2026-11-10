import 'katex/dist/katex.min.css'
import katex from 'katex'

/**
 * The KaTeX chunk's entry: the library and its stylesheet together, imported
 * only by `loadMath`'s dynamic import. Nothing may import this statically; the
 * build manifest guard (`tests/lazy-chunks.test.ts`) fails if the entry reaches
 * it.
 *
 * The CSS rides this chunk, so a note without math never downloads KaTeX's
 * stylesheet or the woff2 faces it names.
 */
export default katex
